//! 工作区用例及生命周期。存储和平台资源由 Host 注入。
use crate::ports::*;
pub use vigilshell_application_contracts::workspace::*;
mod agent_connection;
mod agent_control;
mod codec;
mod credential_access;
mod groups;
mod mutations;
mod network;
mod proxies;
mod tools;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use vigilshell_domain::workspace::*;

struct Draft {
    intent: CredentialIntent,
    proxy: Option<ProxyRecord>,
    owner: String,
    connection: Option<ConnectionRecord>,
    secret: Option<SecretMaterial>,
    trust: Option<(ConnectionInput, HostKey)>,
    busy: bool,
}
struct Operation {
    owner: String,
    view: OperationView,
    cancel: CancellationToken,
    flag: Arc<AtomicBool>,
    cancellable: bool,
}
struct Session {
    control_deadline: Option<std::time::Instant>,
    control_idle_deadline: Option<std::time::Instant>,
    control_idle_seconds: Option<u32>,
    control: vigilshell_domain::workspace_control::WorkspaceControl,
    write_gate: Arc<tokio::sync::Mutex<()>>,
    output_history: VecDeque<(u64, Vec<u8>)>,
    output_bytes: usize,
    output_cursor: u64,
    agent_inputs: HashMap<String, (String, Option<bool>)>,

    owner: String,
    view: SessionView,
    cancel: CancellationToken,
    controls: Option<mpsc::Sender<SessionCommand>>,
}
struct Challenge {
    owner: String,
    operation: String,
    sender: oneshot::Sender<bool>,
}
enum SessionCommand {
    AgentInput {
        request_key: String,
        control: vigilshell_domain::workspace_control::AgentControl,
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<(), WorkspaceError>>,
    },
    Input {
        sequence: u64,
        bytes: Vec<u8>,
        generation: Option<u64>,
    },
    Credit {
        sequence: u64,
        length: u32,
    },
    Resize(u16, u16),
}
#[derive(Default)]
struct State {
    display_owner: Option<String>,
    file_edits: HashMap<String, Arc<tools::editing::EditLease>>,
    file_jobs: HashMap<String, tools::management::FileJob>,
    transfers: HashMap<String, tools::TransferTask>,
    transfer_pages: HashMap<String, tools::TransferSnapshot>,
    grants: HashMap<String, tools::FileGrant>,
    listings: HashMap<String, tools::Listing>,
    listing_reads: HashMap<String, Arc<tokio::sync::Semaphore>>,
    drafts: HashMap<String, Draft>,
    operations: HashMap<String, Operation>,
    order: VecDeque<String>,
    sessions: HashMap<String, Session>,
    closed: VecDeque<String>,
    challenges: HashMap<String, Challenge>,
    changing: HashSet<String>,
    stopping: bool,
    groups: HashMap<(String, String), Arc<groups::RuntimeGroup>>,
}

struct ChangeReservation<'a> {
    state: &'a Mutex<State>,
    id: String,
}
impl Drop for ChangeReservation<'_> {
    fn drop(&mut self) {
        self.state.lock().unwrap().changing.remove(&self.id);
    }
}
pub struct WorkspaceService {
    database: Arc<dyn WorkspaceDatabase>,
    secrets: Arc<dyn SecretStore>,
    prompt: Arc<dyn NativePrompt>,
    transport: Arc<dyn SshTransport>,
    sink: Arc<dyn WorkspaceSink>,
    state: Mutex<State>,
    tasks: TaskTracker,
    network: Arc<Semaphore>,
    monitor_slots: Semaphore,
    namespace: tokio::sync::OnceCell<String>,
    group_gate: tokio::sync::Mutex<()>,
}
impl WorkspaceService {
    pub fn new(
        database: Arc<dyn WorkspaceDatabase>,
        secrets: Arc<dyn SecretStore>,
        prompt: Arc<dyn NativePrompt>,
        transport: Arc<dyn SshTransport>,
        sink: Arc<dyn WorkspaceSink>,
    ) -> Arc<Self> {
        Arc::new(Self {
            database,
            secrets,
            prompt,
            transport,
            sink,
            state: Mutex::new(State::default()),
            tasks: TaskTracker::new(),
            network: Arc::new(Semaphore::new(4)),
            monitor_slots: Semaphore::new(2),
            namespace: tokio::sync::OnceCell::new(),
            group_gate: tokio::sync::Mutex::new(()),
        })
    }
    pub fn register_display_owner(&self, owner: String) {
        self.state.lock().unwrap().display_owner = Some(owner);
    }

    pub fn update_context(&self, context: DisplayContext) {
        self.prompt.update_context(context);
    }
    async fn namespace(&self) -> Result<String, WorkspaceError> {
        self.namespace
            .get_or_try_init(|| async {
                match self.database.execute(DbCommand::Namespace).await? {
                    DbReply::Namespace(n) => Ok(n),
                    _ => Err(WorkspaceError::StorageUnavailable),
                }
            })
            .await
            .cloned()
    }
    async fn connection(&self, id: &str) -> Result<ConnectionRecord, WorkspaceError> {
        match self.database.execute(DbCommand::Get(id.into())).await? {
            DbReply::Connection(p) => Ok(p),
            _ => Err(WorkspaceError::NotFound),
        }
    }
    async fn trust(&self, id: &str) -> Result<Option<HostTrust>, WorkspaceError> {
        match self.database.execute(DbCommand::Trust(id.into())).await? {
            DbReply::Trust(t) => Ok(t),
            _ => Err(WorkspaceError::StorageUnavailable),
        }
    }
    fn operation(&self, owner: &str, id: &str) -> Result<OperationView, WorkspaceError> {
        self.state
            .lock()
            .unwrap()
            .operations
            .get(id)
            .filter(|o| o.owner == owner)
            .map(|o| o.view.clone())
            .ok_or(WorkspaceError::NotFound)
    }
    fn begin(
        &self,
        owner: &str,
        kind: &str,
        cancellable: bool,
    ) -> Result<OperationView, WorkspaceError> {
        let mut state = self.state.lock().unwrap();
        if state.stopping {
            return Err(WorkspaceError::Busy);
        }
        let active = state
            .operations
            .values()
            .filter(|o| matches!(o.view.phase.as_str(), "working" | "trust"))
            .count();
        if active >= 64 {
            return Err(WorkspaceError::Busy);
        }
        while state.order.len() >= 256 {
            let Some(index) = state.order.iter().position(|id| {
                state.operations.get(id).is_none_or(|o| {
                    !matches!(o.view.phase.as_str(), "working" | "trust")
                        && o.view.session_id.as_ref().is_none_or(|sid| {
                            state.sessions.get(sid).is_none_or(|s| {
                                matches!(s.view.phase.as_str(), "closed" | "failed" | "cancelled")
                            })
                        })
                })
            }) else {
                break;
            };
            let id = state.order.remove(index).unwrap();
            state.operations.remove(&id);
        }
        let id = new_id();
        let view = OperationView {
            file_summary: None,
            proxy: None,
            id: id.clone(),
            kind: kind.into(),
            phase: "working".into(),
            stage: kind.into(),
            error: None,
            draft: None,
            connection: None,
            challenge: None,
            session_id: None,
        };
        state.order.push_back(id.clone());
        state.operations.insert(
            id,
            Operation {
                owner: owner.into(),
                view: view.clone(),
                cancel: CancellationToken::new(),
                flag: Arc::new(AtomicBool::new(false)),
                cancellable,
            },
        );
        Ok(view)
    }
    async fn update(&self, id: &str, change: impl FnOnce(&mut OperationView)) {
        let value = {
            let mut state = self.state.lock().unwrap();
            state.operations.get_mut(id).map(|o| {
                change(&mut o.view);
                (o.owner.clone(), o.view.clone())
            })
        };
        if let Some((owner, value)) = value {
            let _ = self
                .sink
                .event(owner, WorkspaceNotice::Operation(Box::new(value)))
                .await;
        }
    }
    async fn finish(&self, id: &str, result: Result<(), WorkspaceError>) {
        self.update(id, |o| {
            o.phase = match result {
                Ok(()) => "success",
                Err(WorkspaceError::Cancelled) => "cancelled",
                Err(_) => "failed",
            }
            .into();
            o.error = result.err();
            o.challenge = None;
        })
        .await;
    }
    async fn changed(&self, owner: &str) {
        let _ = self
            .sink
            .event(owner.into(), WorkspaceNotice::Catalogue)
            .await;
    }
    fn draft_view(id: &str, d: &Draft) -> DraftView {
        DraftView {
            intent: d.intent,
            proxy_id: d
                .proxy
                .as_ref()
                .filter(|p| !p.id.is_empty())
                .map(|p| p.id.clone()),
            id: id.into(),
            connection_id: d.connection.as_ref().map(|p| p.id.clone()),
            prepared: d.secret.is_some(),
            saved: d.proxy.as_ref().is_some_and(|p| p.credential.is_some())
                || d.connection
                    .as_ref()
                    .is_some_and(|p| p.credential.is_some()),
            auth: d
                .secret
                .as_ref()
                .map(|s| s.kind)
                .or_else(|| d.connection.as_ref().map(|p| p.input.auth)),
            label: d.secret.as_ref().map(|s| s.label.clone()).or_else(|| {
                d.connection
                    .as_ref()
                    .and_then(|p| p.credential_label.clone())
            }),
        }
    }
    pub async fn handle(
        self: &Arc<Self>,
        owner: &str,
        action: WorkspaceAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        if !valid_id(owner) || self.state.lock().unwrap().stopping {
            return Err(WorkspaceError::Busy);
        }
        // Protected mutations must fail before capturing input, reserving a job or
        // starting network work. Unlocking is owned by the explicit runtime coordinator.
        if matches!(
            &action,
            WorkspaceAction::Save(_)
                | WorkspaceAction::Open { .. }
                | WorkspaceAction::Delete { .. }
                | WorkspaceAction::RetryTunnel { .. }
        ) || matches!(&action, WorkspaceAction::Test(request) if request.connection_id.is_some())
        {
            self.database.execute(DbCommand::ConfigurationReady).await?;
        }
        match action {
            WorkspaceAction::Tools(action) => self.tool_action(owner, action).await,
            WorkspaceAction::BeginProxyDraft(id) => self.begin_proxy_draft(owner, id).await,
            WorkspaceAction::SaveProxy(request) => self.start_proxy_save(owner, request).await,
            WorkspaceAction::DeleteProxy { id, version } => {
                self.delete_proxy(owner, &id, version).await
            }
            WorkspaceAction::TunnelStatuses(id) => {
                Ok(WorkspaceResult::Tunnels(self.tunnel_statuses(owner, &id)))
            }
            WorkspaceAction::RetryTunnel { connection, rule } => {
                self.retry_tunnel(owner, &connection, &rule).await
            }
            WorkspaceAction::ValidateInput { session, text } => {
                if text.len() > 32 * 1024 {
                    return Err(WorkspaceError::InvalidInput);
                }
                let settings = self
                    .state
                    .lock()
                    .unwrap()
                    .sessions
                    .get(&session)
                    .filter(|s| s.owner == owner)
                    .map(|s| s.view.connection.input.terminal.clone())
                    .ok_or(WorkspaceError::NotFound)?;
                codec::encode_text(&settings.encoding, &text)?;
                Ok(WorkspaceResult::Done)
            }

            WorkspaceAction::Database(command) => {
                let read = matches!(
                    &command,
                    DbCommand::ListProxies { .. }
                        | DbCommand::GetProxy(_)
                        | DbCommand::ProxyReferences { .. }
                        | DbCommand::List { .. }
                        | DbCommand::Get(_)
                        | DbCommand::Trust(_)
                );
                let reply = self.database.execute(command).await?;
                if !read {
                    self.changed(owner).await;
                }
                Ok(WorkspaceResult::Database(reply))
            }
            WorkspaceAction::ReconcileCredentials(id) => {
                if self.state.lock().unwrap().changing.contains(&id) {
                    return Err(WorkspaceError::Busy);
                }
                self.recover_credentials_for(Some(&id)).await?;
                self.changed(owner).await;
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Get(id) => Ok(WorkspaceResult::Connection(
                self.connection(&id).await?,
                self.trust(&id).await?,
            )),
            WorkspaceAction::BeginDraft(id) => {
                let connection = if let Some(id) = id {
                    let p = self.connection(&id).await?;
                    if p.deleting {
                        return Err(WorkspaceError::Busy);
                    }
                    Some(p)
                } else {
                    None
                };
                let mut state = self.state.lock().unwrap();
                if state
                    .drafts
                    .values()
                    .any(|d| d.owner == owner && d.proxy.is_none())
                {
                    return Err(WorkspaceError::Busy);
                }
                let id = new_id();
                let draft = Draft {
                    intent: CredentialIntent::Keep,
                    proxy: None,
                    owner: owner.into(),
                    connection,
                    secret: None,
                    trust: None,
                    busy: false,
                };
                let view = Self::draft_view(&id, &draft);
                state.drafts.insert(id, draft);
                Ok(WorkspaceResult::Draft(view))
            }
            WorkspaceAction::ConfigureDraft { draft, kind } => {
                {
                    let mut state = self.state.lock().unwrap();
                    let d = state
                        .drafts
                        .get_mut(&draft)
                        .filter(|d| d.owner == owner)
                        .ok_or(WorkspaceError::NotFound)?;
                    if d.busy {
                        return Err(WorkspaceError::Busy);
                    }
                    d.busy = true;
                }
                if kind != AuthKind::Password
                    && self
                        .state
                        .lock()
                        .unwrap()
                        .drafts
                        .get(&draft)
                        .is_some_and(|d| d.proxy.is_some())
                {
                    if let Some(d) = self.state.lock().unwrap().drafts.get_mut(&draft) {
                        d.busy = false;
                    }
                    return Err(WorkspaceError::InvalidInput);
                }
                let operation = match self.begin(owner, "credential", true) {
                    Ok(o) => o,
                    Err(e) => {
                        if let Some(d) = self.state.lock().unwrap().drafts.get_mut(&draft) {
                            d.busy = false;
                        }
                        return Err(e);
                    }
                };
                let service = self.clone();
                let id = operation.id.clone();
                let owner = owner.to_owned();
                let flag = self.state.lock().unwrap().operations[&id].flag.clone();
                self.tasks.spawn(async move {
                    let captured = service.prompt.capture(kind, flag).await;
                    let result = {
                        let mut state = service.state.lock().unwrap();
                        match state.drafts.get_mut(&draft).filter(|d| d.owner == owner) {
                            Some(d) => {
                                d.busy = false;
                                match captured {
                                    Ok(secret) => {
                                        d.secret = Some(secret);
                                        d.intent = CredentialIntent::Replace;
                                        d.trust = None;
                                        Ok(Self::draft_view(&draft, d))
                                    }
                                    Err(e) => Err(e),
                                }
                            }
                            None => Err(WorkspaceError::Cancelled),
                        }
                    };
                    if let Ok(view) = &result {
                        service.update(&id, |o| o.draft = Some(view.clone())).await;
                    }
                    service.finish(&id, result.map(|_| ())).await;
                });
                Ok(WorkspaceResult::Operation(Box::new(operation)))
            }
            WorkspaceAction::ClearDraft(id) => {
                let mut state = self.state.lock().unwrap();
                let d = state
                    .drafts
                    .get_mut(&id)
                    .filter(|d| d.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if d.proxy.is_some() {
                    return Err(WorkspaceError::InvalidInput);
                }
                if d.busy {
                    return Err(WorkspaceError::Busy);
                }
                d.secret = None;
                d.trust = None;
                d.intent = CredentialIntent::Remove;
                Ok(WorkspaceResult::Draft(Self::draft_view(&id, d)))
            }
            WorkspaceAction::DiscardDraft(id) => {
                let mut state = self.state.lock().unwrap();
                let Some(d) = state.drafts.get(&id) else {
                    return Ok(WorkspaceResult::Done);
                };
                if d.owner != owner {
                    return Err(WorkspaceError::NotFound);
                }
                if d.busy {
                    return Err(WorkspaceError::Busy);
                }
                state.drafts.remove(&id);
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Save(request) => self.start_save(owner, request).await,
            WorkspaceAction::Test(request) => self.start_test(owner, request).await,
            WorkspaceAction::Open { id, columns, rows } => {
                self.start_open(owner, &id, columns, rows).await
            }
            WorkspaceAction::Sessions => Ok(WorkspaceResult::Sessions(
                self.state
                    .lock()
                    .unwrap()
                    .sessions
                    .values()
                    .filter(|s| s.owner == owner)
                    .map(|s| s.view.clone())
                    .collect(),
            )),
            WorkspaceAction::Resize { id, columns, rows } => {
                if columns == 0 || rows == 0 {
                    return Err(WorkspaceError::InvalidInput);
                }
                self.control(owner, &id, SessionCommand::Resize(columns, rows))?;
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Close(id) => {
                self.close_session(owner, &id).await?;
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Operation(id) => self
                .operation(owner, &id)
                .map(|o| WorkspaceResult::Operation(Box::new(o))),
            WorkspaceAction::Cancel(id) => {
                let mut state = self.state.lock().unwrap();
                let o = state
                    .operations
                    .get_mut(&id)
                    .filter(|o| o.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if !o.cancellable || !matches!(o.view.phase.as_str(), "working" | "trust") {
                    return Err(WorkspaceError::TooLate);
                }
                o.flag.store(true, Ordering::Release);
                o.cancel.cancel();
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Confirm {
                challenge,
                accepted,
            } => {
                let value = {
                    let mut state = self.state.lock().unwrap();
                    if state
                        .challenges
                        .get(&challenge)
                        .is_none_or(|c| c.owner != owner)
                    {
                        return Err(WorkspaceError::TrustExpired);
                    }
                    state.challenges.remove(&challenge).unwrap()
                };
                value
                    .sender
                    .send(accepted)
                    .map_err(|_| WorkspaceError::TrustExpired)?;
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::ForgetTrust { id, version } => {
                self.database
                    .execute(DbCommand::ForgetTrust { id, version })
                    .await?;
                self.changed(owner).await;
                Ok(WorkspaceResult::Done)
            }
            WorkspaceAction::Delete {
                id,
                version,
                disconnect,
                sessions,
            } => {
                self.start_delete(owner, &id, version, disconnect, sessions)
                    .await
            }
        }
    }
    fn control(
        &self,
        owner: &str,
        id: &str,
        command: SessionCommand,
    ) -> Result<(), WorkspaceError> {
        let state = self.state.lock().unwrap();
        let session = state
            .sessions
            .get(id)
            .filter(|s| s.owner == owner)
            .ok_or(WorkspaceError::NotFound)?;
        session
            .controls
            .as_ref()
            .ok_or(WorkspaceError::Disconnected)?
            .try_send(command)
            .map_err(|_| WorkspaceError::Busy)
    }
    pub fn input(
        &self,
        owner: &str,
        id: &str,
        sequence: u64,
        bytes: Vec<u8>,
    ) -> Result<(), WorkspaceError> {
        if bytes.len() > 32 * 1024 {
            return Err(WorkspaceError::InvalidInput);
        }
        let generation = self
            .state
            .lock()
            .unwrap()
            .sessions
            .get(id)
            .filter(|s| s.owner == owner)
            .ok_or(WorkspaceError::NotFound)?
            .control
            .manual_generation();
        self.control(
            owner,
            id,
            SessionCommand::Input {
                sequence,
                bytes,
                generation,
            },
        )
    }
    pub fn credit(
        &self,
        owner: &str,
        id: &str,
        sequence: u64,
        length: u32,
    ) -> Result<(), WorkspaceError> {
        if length == 0 || length > 32 * 1024 {
            return Err(WorkspaceError::InvalidInput);
        }
        self.control(owner, id, SessionCommand::Credit { sequence, length })
    }
    async fn close_session(&self, owner: &str, id: &str) -> Result<(), WorkspaceError> {
        {
            let state = self.state.lock().unwrap();
            let session = state
                .sessions
                .get(id)
                .filter(|s| s.owner == owner)
                .ok_or(WorkspaceError::NotFound)?;
            if !session.control.manual_input_allowed() {
                return Err(WorkspaceError::Busy);
            }
            session.cancel.cancel();
            if let Some(o) = state.operations.get(&session.view.operation_id) {
                o.flag.store(true, Ordering::Release);
                o.cancel.cancel();
            }
        }
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if self.state.lock().unwrap().sessions.get(id).is_none_or(|s| {
                    matches!(s.view.phase.as_str(), "closed" | "failed" | "cancelled")
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| WorkspaceError::Timeout)
    }
    pub async fn detach(&self, owner: &str) {
        {
            let mut state = self.state.lock().unwrap();
            if state.display_owner.as_deref() == Some(owner) {
                state.display_owner = None;
            }
        }
        {
            let mut state = self.state.lock().unwrap();
            for o in state
                .operations
                .values()
                .filter(|o| o.owner == owner && o.cancellable)
            {
                o.flag.store(true, Ordering::Release);
                o.cancel.cancel();
            }
            for s in state.sessions.values().filter(|s| s.owner == owner) {
                s.cancel.cancel();
            }
            for task in state.transfers.values().filter(|t| t.owner == owner) {
                task.cancel.store(true, Ordering::Release);
            }
            for ((group_owner, _), group) in &state.groups {
                if group_owner == owner
                    && !state
                        .file_edits
                        .values()
                        .any(|edit| Arc::ptr_eq(&edit.group, group))
                {
                    group.release_credentials();
                }
            }
            for task in state.transfers.values().filter(|t| t.owner == owner) {
                task.group.release_credentials();
            }
            state.grants.retain(|_, g| g.owner != owner);
            for listing in state.listings.values().filter(|l| l.owner == owner) {
                listing.cancel.store(true, Ordering::Release);
            }
            state.listings.retain(|_, l| l.owner != owner);
            state.transfer_pages.retain(|_, l| l.owner != owner);
            state.listing_reads.remove(owner);
            state.drafts.retain(|_, d| d.owner != owner);
        }
        let _ = self
            .sink
            .event(owner.into(), WorkspaceNotice::Detached)
            .await;
    }
    pub async fn shutdown(&self) -> Result<(), WorkspaceError> {
        self.secrets.close();
        {
            let mut state = self.state.lock().unwrap();
            state.stopping = true;
            for group in state.groups.values() {
                group.release_credentials();
            }
            for task in state.transfers.values() {
                task.group.release_credentials();
            }
            for task in state.transfers.values() {
                task.cancel.store(true, Ordering::Release);
            }
            for listing in state.listings.values() {
                listing.cancel.store(true, Ordering::Release);
            }
            for o in state.operations.values().filter(|o| o.cancellable) {
                o.flag.store(true, Ordering::Release);
                o.cancel.cancel();
            }
            for s in state.sessions.values() {
                s.cancel.cancel();
            }
        }
        self.tasks.close();
        let result = tokio::time::timeout(Duration::from_secs(2), self.tasks.wait())
            .await
            .map_err(|_| WorkspaceError::Timeout);
        self.state.lock().unwrap().drafts.clear();
        result
    }
}

#[cfg(test)]
mod tests;
