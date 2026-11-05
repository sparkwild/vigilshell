mod directory;
pub(super) mod editing;
pub(super) mod management;
mod monitor;
mod network;
mod transfers;
use super::groups::RuntimeGroup;
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::AtomicU64,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::ports::{
    ArchiveIo, DbCommand, DbReply, ServerTools, TransferIo, TransferTreeIo, TransferTreeState,
};

use crate::tools::{ToolAction, ToolResult};
use vigilshell_domain::tools::*;

pub(super) struct GroupTools {
    pub id: String,
    client: tokio::sync::Mutex<Option<Arc<dyn ServerTools>>>,
    sampling: tokio::sync::Mutex<()>,
    visible: AtomicBool,
    monitor_generation: AtomicU64,
    wake: tokio::sync::Notify,
    monitoring: AtomicBool,
    suspended: AtomicBool,
    sample: Mutex<ServerSample>,
    previous_network: Mutex<network::NetworkRate>,
    previous_cpu: Mutex<Option<(u64, u64)>>,
    last_full: Mutex<Option<Instant>>,
}
impl Default for GroupTools {
    fn default() -> Self {
        Self {
            id: new_id(),
            client: tokio::sync::Mutex::new(None),
            sampling: tokio::sync::Mutex::new(()),
            visible: AtomicBool::new(false),
            monitor_generation: AtomicU64::new(0),
            wake: tokio::sync::Notify::new(),
            monitoring: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            sample: Mutex::new(ServerSample {
                phase: "loading".into(),
                ..Default::default()
            }),
            previous_network: Mutex::new(network::NetworkRate::default()),
            previous_cpu: Mutex::new(None),
            last_full: Mutex::new(None),
        }
    }
}
pub(super) struct FileGrant {
    pub owner: String,
    direction: String,
    paths: Vec<PathBuf>,
}
pub(super) struct Listing {
    pub owner: String,
    session: String,
    path: String,
    pub cancel: Arc<AtomicBool>,
    entries: Option<Vec<RemoteEntry>>,
    touched: Instant,
}
pub(super) struct TransferSnapshot {
    pub owner: String,
    touched: Instant,
    tasks: Vec<TransferView>,
}
pub(super) struct TransferTask {
    tree: Arc<Mutex<TransferTreeState>>,
    archive_paths: Option<Vec<String>>,
    batch_policy: Arc<Mutex<Option<bool>>>,
    pub owner: String,
    pub group: Arc<RuntimeGroup>,
    pub view: TransferView,
    local: PathBuf,
    remote: String,
    overwrite: bool,
    pub cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
}
pub(super) fn transfer_active(phase: &str) -> bool {
    ["queued", "running", "awaitingDecision", "cancelling"].contains(&phase)
}
impl WorkspaceService {
    fn tool_group(&self, owner: &str, session: &str) -> Result<Arc<RuntimeGroup>, WorkspaceError> {
        let state = self.state.lock().unwrap();
        let session = state
            .sessions
            .get(session)
            .filter(|s| s.owner == owner && s.view.phase == "running")
            .ok_or(WorkspaceError::Disconnected)?;
        state
            .groups
            .get(&(owner.into(), session.view.connection.id.clone()))
            .cloned()
            .ok_or(WorkspaceError::Disconnected)
    }
    async fn tools_client(
        &self,
        group: &Arc<RuntimeGroup>,
    ) -> Result<Arc<dyn ServerTools>, WorkspaceError> {
        let mut client = group.tools.client.lock().await;
        if let Some(client) = client.as_ref() {
            return Ok(client.clone());
        }
        let key = group
            .trusted
            .get()
            .cloned()
            .ok_or(WorkspaceError::TrustRequired)?;
        let secret = self.group_secret(group).await?;
        let route = self.group_route(group).await?;
        let created = self
            .transport
            .tools(group.profile.input.clone(), key, secret, route)
            .await?;
        *client = Some(created.clone());
        Ok(created)
    }
    pub(super) async fn stop_group_tools(&self, group: &Arc<RuntimeGroup>) {
        group.tools.visible.store(false, Ordering::Release);
        group.tools.wake.notify_one();
        let leased = self
            .state
            .lock()
            .unwrap()
            .transfers
            .values()
            .any(|t| Arc::ptr_eq(&t.group, group) && transfer_active(&t.view.phase));
        let leased = leased
            || self
                .state
                .lock()
                .unwrap()
                .file_jobs
                .values()
                .any(|j| Arc::ptr_eq(&j.group, group) && j.phase == "working");
        let leased = leased
            || self
                .state
                .lock()
                .unwrap()
                .file_edits
                .values()
                .any(|e| Arc::ptr_eq(&e.group, group));
        if !leased && group.refs.load(Ordering::Acquire) == 0 {
            group.release_credentials();
        }
        if !leased && let Some(client) = group.tools.client.lock().await.take() {
            let _ = client.close().await;
        }
    }
}

fn json_string_size(text: &str) -> usize {
    2 + text
        .chars()
        .map(|c| match c {
            '\"' | '\\' => 2,
            c if c < ' ' => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}
fn directory_page(
    path: &str,
    entries: &[RemoteEntry],
    offset: usize,
) -> Result<DirectoryPage, WorkspaceError> {
    if offset > entries.len() {
        return Err(WorkspaceError::InvalidInput);
    }
    let mut bytes = 1024 + json_string_size(path);
    let mut page = Vec::new();
    for entry in entries.iter().skip(offset).take(100) {
        let size = 256
            + json_string_size(&entry.name)
            + json_string_size(&entry.path)
            + json_string_size(&entry.kind);
        if bytes + size > 60 * 1024 {
            if page.is_empty() {
                return Err(WorkspaceError::FilesTooLarge);
            }
            break;
        }
        bytes += size;
        page.push(entry.clone());
    }
    let end = offset + page.len();
    Ok(DirectoryPage {
        path: path.into(),
        entries: page,
        next: (end < entries.len()).then(|| end.to_string()),
    })
}

fn transfer_size(task: &TransferView) -> usize {
    1024 + [&task.name, &task.remote_path, &task.remote_identity]
        .iter()
        .map(|s| json_string_size(s))
        .sum::<usize>()
        + task.conflict.as_ref().map_or(0, |c| {
            json_string_size(&c.target_path)
                + c.source_path.as_ref().map_or(0, |p| json_string_size(p))
        })
}

impl WorkspaceService {
    pub(super) async fn tool_action(
        self: &Arc<Self>,
        owner: &str,
        action: ToolAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        // 与终端接管使用同一把写入锁，校验后到提交期间不能被接管穿插。
        let target = match &action {
            ToolAction::CreateFile { session, .. }
            | ToolAction::CreateDirectory { session, .. }
            | ToolAction::Rename { session, .. }
            | ToolAction::Remove { session, .. }
            | ToolAction::StartFileOperation { session, .. }
            | ToolAction::Transfer { session, .. }
            | ToolAction::Archive { session, .. } => Some(session.as_str()),
            ToolAction::Edit(
                vigilshell_application_contracts::file_editing::FileEditCommand::Open {
                    session,
                    ..
                },
            ) => Some(session.as_str()),
            _ => None,
        };
        let gate = if let Some(id) = target {
            let state = self.state.lock().unwrap();
            Some(
                state
                    .sessions
                    .get(id)
                    .filter(|s| s.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?
                    .write_gate
                    .clone(),
            )
        } else {
            None
        };
        let _guard = if let Some(gate) = &gate {
            Some(gate.lock().await)
        } else {
            None
        };
        if let Some(id) = target
            && !self
                .state
                .lock()
                .unwrap()
                .sessions
                .get(id)
                .is_some_and(|s| s.control.manual_input_allowed())
        {
            return Err(WorkspaceError::Busy);
        }
        // 历史文件任务绑定运行组；先锁定对应标签，防止重试校验与接管交错。
        let retry_tabs = {
            let state = self.state.lock().unwrap();
            let group = match &action {
                ToolAction::RetryFileOperation { id } => state
                    .file_jobs
                    .get(id)
                    .filter(|j| j.owner == owner)
                    .map(|j| &j.group.tools.id),
                ToolAction::RetryTransfer { id } | ToolAction::ResolveConflict { id, .. } => state
                    .transfers
                    .get(id)
                    .filter(|t| t.owner == owner)
                    .map(|t| &t.group.tools.id),
                _ => None,
            };
            let mut tabs: Vec<_> = state
                .sessions
                .values()
                .filter(|s| s.owner == owner && group.is_some_and(|g| *g == s.view.group_id))
                .map(|s| (s.view.id.clone(), s.write_gate.clone()))
                .collect();
            tabs.sort_by(|a, b| a.0.cmp(&b.0));
            tabs
        };
        let mut retry_guards = Vec::new();
        for (_, gate) in &retry_tabs {
            retry_guards.push(gate.lock().await);
        }
        if !retry_tabs.is_empty() {
            let state = self.state.lock().unwrap();
            if retry_tabs.iter().any(|(id, _)| {
                state
                    .sessions
                    .get(id)
                    .is_none_or(|s| !s.control.manual_input_allowed())
            }) {
                return Err(WorkspaceError::Busy);
            }
        }
        match action {
            ToolAction::Edit(command) => self.edit_action(owner, command).await,
            ToolAction::Stat { .. }
            | ToolAction::CreateFile { .. }
            | ToolAction::StartFileOperation { .. }
            | ToolAction::FileOperation { .. }
            | ToolAction::RetryFileOperation { .. } => self.file_management(owner, action).await,
            ToolAction::Subscribe { .. } | ToolAction::Refresh { .. } => {
                self.monitor_action(owner, action).await
            }
            ToolAction::List { .. }
            | ToolAction::ReleaseList { .. }
            | ToolAction::CancelList { .. }
            | ToolAction::Identities { .. }
            | ToolAction::CreateDirectory { .. }
            | ToolAction::Rename { .. }
            | ToolAction::Remove { .. } => self.directory_action(owner, action).await,
            ToolAction::Grant { .. }
            | ToolAction::Transfer { .. }
            | ToolAction::Archive { .. }
            | ToolAction::TransferItems { .. }
            | ToolAction::CancelTransfer { .. }
            | ToolAction::RetryTransfer { .. }
            | ToolAction::Transfers
            | ToolAction::TransferPage { .. }
            | ToolAction::ResolveConflict { .. } => self.transfers_action(owner, action).await,
        }
    }
}
#[cfg(test)]
mod page_tests {
    use super::*;
    fn entry(n: usize) -> RemoteEntry {
        RemoteEntry {
            name: format!("{n}{}", "\"\n中".repeat(500)),
            path: format!("/root/{n}{}", "a".repeat(4000)),
            kind: "file".into(),
            size: Some(u64::MAX),
            modified: Some(u64::MAX),
            permissions: Some(u32::MAX),
            uid: Some(u32::MAX),
            gid: Some(u32::MAX),
        }
    }
    #[test]
    fn directory_pages_respect_byte_budget_and_do_not_drop_entries() {
        let entries = (0..205).map(entry).collect::<Vec<_>>();
        let mut offset = 0;
        loop {
            let page = directory_page("/root", &entries, offset).unwrap();
            assert!(!page.entries.is_empty());
            assert!(page.entries.len() < 100);
            let bytes = 1024
                + json_string_size(&page.path)
                + page
                    .entries
                    .iter()
                    .map(|e| {
                        256 + json_string_size(&e.name)
                            + json_string_size(&e.path)
                            + json_string_size(&e.kind)
                    })
                    .sum::<usize>();
            assert!(bytes <= 60 * 1024);
            offset += page.entries.len();
            if page.next.is_none() {
                break;
            }
            assert_eq!(page.next.unwrap().parse::<usize>().unwrap(), offset);
        }
        assert_eq!(offset, 205);
        let mut huge = entry(0);
        huge.path = "\n".repeat(16384);
        assert!(matches!(
            directory_page("/", &[huge], 0),
            Err(WorkspaceError::FilesTooLarge)
        ));
    }
}
