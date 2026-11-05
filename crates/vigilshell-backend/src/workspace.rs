//! 桌面工作区协议到应用用例的适配。
use crate::rpc::Job;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use vigilshell_application::workspace::WorkspaceService;

use vigilshell_application::ports::{
    DbCommand, DbReply, DisplayContext, FutureResult, WorkspaceDatabase, WorkspaceSink,
};

use vigilshell_application::workspace::{
    DraftView, OperationView, SaveRequest, SessionEndKind, SessionView, TestRequest,
    WorkspaceAction, WorkspaceNotice, WorkspaceResult,
};

use vigilshell_desktop_protocol::{terminal::*, workspace::*};
use vigilshell_domain::workspace::*;

#[derive(Clone)]
pub struct WorkspaceOptions {
    pub data_root: PathBuf,
    pub helper: PathBuf,
    pub development: bool,
    pub terminal_pipes: bool,
}
struct Database(mpsc::Sender<Job>);
impl WorkspaceDatabase for Database {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.0
                .try_send(Job::Workspace {
                    command: Box::new(command),
                    reply: tx,
                })
                .map_err(|_| WorkspaceError::Busy)?;
            rx.await.map_err(|_| WorkspaceError::StorageUnavailable)?
        })
    }
}
struct UnlockedDatabase {
    database: Arc<dyn WorkspaceDatabase>,
    secrets: Arc<vigilshell_credentials::EncryptedSecretStore>,
}
impl WorkspaceDatabase for UnlockedDatabase {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if !command.needs_connection_configuration() {
                return self.database.execute(command).await;
            }
            let cipher = self.secrets.configuration_cipher()?;
            let result = self
                .database
                .execute(DbCommand::WithConfigurationCipher {
                    cipher,
                    command: Box::new(command),
                })
                .await;
            if let Err(
                error @ (WorkspaceError::ConfigurationCorrupt
                | WorkspaceError::ConfigurationFormatUnsupported
                | WorkspaceError::VaultKeyInvalid),
            ) = result
            {
                self.secrets.block_configuration(error);
            }
            result
        })
    }
}
struct Sink {
    output: mpsc::Sender<Value>,
    data: Option<mpsc::Sender<TerminalFrame>>,
    runtime_id: String,
}
impl WorkspaceSink for Sink {
    fn event(&self, owner: String, event: WorkspaceNotice) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let event = match event {
                WorkspaceNotice::Tools(snapshot) => WorkspaceEvent::ToolSnapshot {
                    snapshot: (*snapshot).into(),
                },
                WorkspaceNotice::Transfer(task) => WorkspaceEvent::Transfer {
                    task: (*task).into(),
                },
                WorkspaceNotice::Tunnels {
                    connection_id,
                    states,
                } => WorkspaceEvent::Tunnels {
                    connection_id,
                    states: states.into_iter().map(Into::into).collect(),
                },
                WorkspaceNotice::InputRejected { session_id, code } => {
                    WorkspaceEvent::InputRejected { session_id, code }
                }
                WorkspaceNotice::Catalogue => WorkspaceEvent::Catalogue {},
                WorkspaceNotice::Operation(o) => WorkspaceEvent::Operation {
                    operation: Box::new(operation(*o)),
                },
                WorkspaceNotice::Session(s) => WorkspaceEvent::Session {
                    session: Box::new(session(*s)),
                },
                WorkspaceNotice::Detached => WorkspaceEvent::Detached {},
            };
            self.output.send(json!({"jsonrpc":"2.0","method":"workspace.changed","params":{"runtimeId":self.runtime_id,"owner":owner,"event":event}})).await.map_err(|_|WorkspaceError::StreamUnavailable)
        })
    }
    fn output(
        &self,
        owner: String,
        session: String,
        sequence: u64,
        bytes: Vec<u8>,
    ) -> FutureResult<'_, ()> {
        self.frame(TerminalFrame {
            kind: OUTPUT,
            owner,
            session,
            sequence,
            bytes,
        })
    }
    fn input_ack(
        &self,
        owner: String,
        session: String,
        sequence: u64,
        length: u32,
    ) -> FutureResult<'_, ()> {
        self.frame(TerminalFrame {
            kind: INPUT_ACK,
            owner,
            session,
            sequence,
            bytes: length.to_be_bytes().to_vec(),
        })
    }
    fn end(&self, owner: String, session: String, sequence: u64) -> FutureResult<'_, ()> {
        self.frame(TerminalFrame {
            kind: END,
            owner,
            session,
            sequence,
            bytes: Vec::new(),
        })
    }
}
impl Sink {
    fn frame(&self, frame: TerminalFrame) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.data
                .as_ref()
                .ok_or(WorkspaceError::StreamUnavailable)?
                .send(frame)
                .await
                .map_err(|_| WorkspaceError::StreamUnavailable)
        })
    }
}

type Receipt = Result<WorkspaceReply, WorkspaceError>;
#[derive(Default)]
struct Receipts {
    order: VecDeque<String>,
    results: HashMap<String, Receipt>,
}
pub(crate) struct WorkspaceRouter {
    pub service: Arc<WorkspaceService>,
    agent: Arc<vigilshell_application::agent::AgentService>,
    vault_keys: Arc<vigilshell_credentials::PlatformVaultKeyStore>,
    owners: Mutex<HashSet<String>>,
    secrets: Arc<vigilshell_credentials::EncryptedSecretStore>,
    access_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    receipts: Mutex<Receipts>,
    runtime_id: String,
    has_data: bool,
    data_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl WorkspaceRouter {
    pub(crate) fn new(
        jobs: mpsc::Sender<Job>,
        output: mpsc::Sender<Value>,
        options: WorkspaceOptions,
        runtime_id: String,
        failed: CancellationToken,
    ) -> io::Result<Arc<Self>> {
        let (data, reader) = if options.terminal_pipes {
            let (tx, rx) = pipes(failed.clone())?;
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let agent_output = output.clone();
        let access_output = output.clone();
        let sink = Arc::new(Sink {
            output,
            data,
            runtime_id: runtime_id.clone(),
        });
        let database: Arc<dyn WorkspaceDatabase> = Arc::new(Database(jobs));
        let vault_keys = Arc::new(vigilshell_credentials::PlatformVaultKeyStore::new(
            options.development,
        ));
        let secrets = Arc::new(vigilshell_credentials::EncryptedSecretStore::new(
            database.clone(),
            vault_keys.clone(),
        ));
        let database: Arc<dyn WorkspaceDatabase> = Arc::new(UnlockedDatabase {
            database,
            secrets: secrets.clone(),
        });
        let service = WorkspaceService::new(
            database.clone(),
            secrets.clone(),
            Arc::new(vigilshell_credentials::NativePrompter::new(options.helper)),
            Arc::new(vigilshell_transport_russh::RusshTransport),
            sink,
        );
        let agent = crate::agent::create(
            database,
            service.clone(),
            agent_output,
            runtime_id.clone(),
            options.data_root,
        );
        let router = Arc::new(Self {
            agent,
            service: service.clone(),
            vault_keys,
            owners: Mutex::new(HashSet::new()),
            secrets: secrets.clone(),
            access_task: Mutex::new(None),
            receipts: Mutex::new(Receipts::default()),
            runtime_id,
            has_data: options.terminal_pipes,
            data_task: Mutex::new(None),
        });
        let weak = Arc::downgrade(&router);
        let mut access = secrets.subscribe_access();
        *router.access_task.lock().unwrap() = Some(tokio::spawn(async move {
            while access.changed().await.is_ok() {
                let state = access.borrow_and_update().clone();
                let Some(router) = weak.upgrade() else { break };
                let owners = router
                    .owners
                    .lock()
                    .unwrap()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                for owner in owners {
                    let note = WorkspaceNotification {
                        jsonrpc: "2.0".into(),
                        method: "workspace.changed".into(),
                        params: WorkspaceNotificationParams {
                            runtime_id: router.runtime_id.clone(),
                            owner,
                            event: WorkspaceEvent::ConnectionAccess {
                                state: state.clone(),
                            },
                        },
                    };
                    if access_output
                        .send(serde_json::to_value(note).unwrap())
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                if state.phase == ConnectionAccessPhase::Ready {
                    router.service.recover_credentials_background();
                }
            }
        }));
        if let Some(mut reader) = reader {
            *router.data_task.lock().unwrap() = Some(tokio::spawn(async move {
                while let Some(frame) = reader.recv().await {
                    let result = match frame.kind {
                        INPUT => {
                            service.input(&frame.owner, &frame.session, frame.sequence, frame.bytes)
                        }
                        CREDIT => service.credit(
                            &frame.owner,
                            &frame.session,
                            frame.sequence,
                            u32::from_be_bytes(frame.bytes.as_slice().try_into().unwrap()),
                        ),
                        _ => Err(WorkspaceError::InvalidInput),
                    };
                    if matches!(
                        result,
                        Err(WorkspaceError::InvalidInput
                            | WorkspaceError::StreamUnavailable
                            | WorkspaceError::Busy)
                    ) {
                        failed.cancel();
                        break;
                    }
                }
            }));
        }
        Ok(router)
    }
    pub(crate) async fn request(&self, request: WorkspaceRequest) -> Receipt {
        if !valid_id(&request.owner) {
            return Err(WorkspaceError::InvalidInput);
        }
        if let WorkspaceCommand::Negotiate { version } = request.command {
            if version != vigilshell_desktop_protocol::workspace::WORKSPACE_PROTOCOL_VERSION {
                return Err(WorkspaceError::InvalidInput);
            }
            self.service.register_display_owner(request.owner.clone());
            self.owners.lock().unwrap().insert(request.owner);
            return Ok(WorkspaceReply::Ready {
                connection_access: self.secrets.access_state(),
                version: vigilshell_desktop_protocol::workspace::WORKSPACE_PROTOCOL_VERSION,
            });
        }
        if !self.owners.lock().unwrap().contains(&request.owner) {
            return Err(WorkspaceError::InvalidInput);
        }
        if matches!(
            request.command,
            WorkspaceCommand::InitializeConnections {} | WorkspaceCommand::UnlockConnections {}
        ) {
            return Ok(WorkspaceReply::ConnectionAccess {
                state: self.secrets.initialize(matches!(
                    request.command,
                    WorkspaceCommand::UnlockConnections {}
                )),
            });
        }
        if let WorkspaceCommand::Agent { request: operation } = request.command {
            let result = self.agent.request(operation).await.unwrap_or_else(|code| {
                vigilshell_domain::agent::AgentReply::Error { code: code.into() }
            });
            return Ok(WorkspaceReply::Agent {
                result: Box::new(result),
            });
        }
        if let WorkspaceCommand::Context { locale, theme } = request.command {
            if !["en", "zh-CN"].contains(&locale.as_str())
                || !["light", "dark"].contains(&theme.as_str())
            {
                return Err(WorkspaceError::InvalidInput);
            }
            self.vault_keys.update_locale(&locale);
            self.service
                .update_context(DisplayContext { locale, theme });
            return Ok(WorkspaceReply::Done {});
        }
        if matches!(request.command, WorkspaceCommand::Detach {}) {
            self.service.detach(&request.owner).await;
            self.owners.lock().unwrap().remove(&request.owner);
            return Ok(WorkspaceReply::Done {});
        }
        if let WorkspaceCommand::Reconcile {
            request_id,
            runtime_id,
        } = request.command
        {
            let result = if runtime_id == self.runtime_id {
                self.receipts
                    .lock()
                    .unwrap()
                    .results
                    .get(&format!("{}:{request_id}", request.owner))
                    .cloned()
            } else {
                None
            };
            return Ok(match result {
                Some(Ok(value)) => WorkspaceReply::Reconciled {
                    result: Some(Box::new(value)),
                    error: None,
                },
                Some(Err(e)) => WorkspaceReply::Reconciled {
                    result: None,
                    error: Some(e.code().into()),
                },
                None => WorkspaceReply::Reconciled {
                    result: None,
                    error: None,
                },
            });
        }
        let retain = !matches!(
            &request.command,
            WorkspaceCommand::FilesStat { .. }
                | WorkspaceCommand::FilesOperation { .. }
                | WorkspaceCommand::ToolsSubscribe { .. }
                | WorkspaceCommand::ToolsRefresh { .. }
                | WorkspaceCommand::FilesList { .. }
                | WorkspaceCommand::FilesIdentities { .. }
                | WorkspaceCommand::FilesReleaseList { .. }
                | WorkspaceCommand::FilesCancelList { .. }
                | WorkspaceCommand::Transfers { .. }
                | WorkspaceCommand::TransferItems { .. }
                | WorkspaceCommand::ListProxies { .. }
                | WorkspaceCommand::ProxyReferences { .. }
                | WorkspaceCommand::TunnelStatuses { .. }
                | WorkspaceCommand::ValidateInput { .. }
                | WorkspaceCommand::List(_)
                | WorkspaceCommand::Get { .. }
                | WorkspaceCommand::Sessions {}
                | WorkspaceCommand::Operation { .. }
        );
        if !self.has_data && matches!(&request.command, WorkspaceCommand::Open { .. }) {
            return Err(WorkspaceError::StreamUnavailable);
        }
        let action = action(request.command)?;
        let result = self.service.handle(&request.owner, action).await.map(reply);
        if retain {
            let mut receipts = self.receipts.lock().unwrap();
            let key = format!("{}:{}", request.owner, request.id);
            receipts.order.push_back(key.clone());
            receipts.results.insert(key, result.clone());
            while receipts.order.len() > 256 {
                let id = receipts.order.pop_front().unwrap();
                receipts.results.remove(&id);
            }
        }
        result
    }
    pub(crate) async fn stop(&self) -> io::Result<()> {
        if let Some(task) = self.access_task.lock().unwrap().take() {
            task.abort();
        }
        self.agent.shutdown().await;
        let result = self.service.shutdown().await;
        if let Some(task) = self.data_task.lock().unwrap().take() {
            task.abort();
        }
        result.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "workspace_shutdown_timeout"))
    }
}
fn action(command: WorkspaceCommand) -> Result<WorkspaceAction, WorkspaceError> {
    use WorkspaceCommand as C;
    let value = match command {
        C::FileEdit(command) => WorkspaceAction::Tools(
            vigilshell_application::tools::ToolAction::Edit(command.into()),
        ),
        C::ToolsSubscribe { group_id, visible } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Subscribe {
                group: group_id,
                visible,
            })
        }
        C::ToolsRefresh { session_id } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Refresh {
                session: session_id,
            })
        }
        C::FilesList {
            session_id,
            path,
            cursor,
            request_id,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::List {
            session: session_id,
            path,
            cursor,
            request: request_id,
        }),
        C::FilesReleaseList { request_id } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::ReleaseList {
                request: request_id,
            })
        }
        C::FilesIdentities {
            session_id,
            uids,
            gids,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Identities {
            session: session_id,
            uids,
            gids,
        }),
        C::FilesCancelList { request_id } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::CancelList {
                request: request_id,
            })
        }
        C::FilesStat { session_id, path } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Stat {
                session: session_id,
                path,
            })
        }
        C::FilesCreate { session_id, path } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::CreateFile {
                session: session_id,
                path,
            })
        }
        C::FilesStartOperation {
            session_id,
            paths,
            operation,
            recursive,
            set_mask,
            clear_mask,
        } => WorkspaceAction::Tools(
            vigilshell_application::tools::ToolAction::StartFileOperation {
                session: session_id,
                paths,
                operation,
                recursive,
                set_mask,
                clear_mask,
            },
        ),
        C::FilesOperation { id, offset } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::FileOperation {
                id,
                offset,
            })
        }
        C::FilesRetryOperation { id } => WorkspaceAction::Tools(
            vigilshell_application::tools::ToolAction::RetryFileOperation { id },
        ),
        C::FilesMkdir { session_id, path } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::CreateDirectory {
                session: session_id,
                path,
            })
        }
        C::FilesRename {
            session_id,
            from,
            to,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Rename {
            session: session_id,
            from,
            to,
        }),
        C::FilesRemove {
            session_id,
            path,
            directory,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Remove {
            session: session_id,
            path,
            directory,
        }),
        C::FilesPick { .. } => return Err(WorkspaceError::InvalidInput),
        C::FilesGrant { direction, paths } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Grant {
                direction,
                paths: paths.into_iter().map(Into::into).collect(),
            })
        }
        C::FilesArchive {
            session_id,
            grant_id,
            paths,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Archive {
            session: session_id,
            grant: grant_id,
            paths,
        }),
        C::TransferItems { id, offset } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::TransferItems {
                id,
                offset,
            })
        }
        C::FilesTransfer {
            session_id,
            grant_id,
            paths,
            overwrite,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::Transfer {
            session: session_id,
            grant: grant_id,
            paths,
            overwrite,
        }),
        C::TransferCancel { id } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::CancelTransfer { id })
        }
        C::TransferRetry { id } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::RetryTransfer { id })
        }
        C::Transfers { request_id, cursor } => {
            WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::TransferPage {
                request: request_id,
                cursor,
            })
        }
        C::ResolveConflict {
            id,
            version,
            decision,
            batch,
        } => WorkspaceAction::Tools(vigilshell_application::tools::ToolAction::ResolveConflict {
            id,
            version,
            decision,
            batch,
        }),
        C::ListProxies { offset, version } => {
            WorkspaceAction::Database(DbCommand::ListProxies { offset, version })
        }
        C::ProxyReferences {
            id,
            offset,
            version,
        } => WorkspaceAction::Database(DbCommand::ProxyReferences {
            id,
            offset,
            version,
        }),
        C::BeginProxyDraft { proxy_id } => WorkspaceAction::BeginProxyDraft(proxy_id),
        C::DeleteProxy(v) => WorkspaceAction::DeleteProxy {
            id: v.id,
            version: v.version,
        },
        C::SaveProxy(v) => {
            if v.proxy_id.is_some() != v.expected_version.is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
            WorkspaceAction::SaveProxy(vigilshell_domain::workspace::ProxySave {
                proxy: vigilshell_domain::workspace::ProxyRecord {
                    id: v.proxy_id.unwrap_or_else(new_id),
                    version: v.expected_version.unwrap_or(0),
                    name: v.config.name,
                    kind: v.config.kind,
                    host: v.config.host,
                    port: v.config.port,
                    username: v.config.username,
                    credential: None,
                },
                expected_version: v.expected_version,
                draft_id: v.draft_id,
                clear_password: v.clear_password,
            })
        }
        C::TunnelStatuses { connection_id } => WorkspaceAction::TunnelStatuses(connection_id),
        C::RetryTunnel {
            connection_id,
            rule_id,
        } => WorkspaceAction::RetryTunnel {
            connection: connection_id,
            rule: rule_id,
        },
        C::ValidateInput { session_id, text } => WorkspaceAction::ValidateInput {
            session: session_id,
            text,
        },

        C::UnlockConnections {} | C::InitializeConnections {} => {
            return Err(WorkspaceError::InvalidInput);
        }
        C::List(q) => WorkspaceAction::Database(DbCommand::List {
            query: q.query,
            offset: q.offset,
            limit: 50,
            kind: match q.kind {
                CatalogueKind::Groups => "groups",
                CatalogueKind::Connections => "connections",
            }
            .into(),
            version: q.version,
        }),
        C::CreateGroup { name } => WorkspaceAction::Database(DbCommand::CreateGroup(name)),
        C::UpdateGroup { id, version, name } => {
            WorkspaceAction::Database(DbCommand::UpdateGroup { id, version, name })
        }
        C::DeleteGroup(v) => WorkspaceAction::Database(DbCommand::DeleteGroup {
            id: v.id,
            version: v.version,
        }),
        C::UpdateLayout(v) => WorkspaceAction::Database(DbCommand::UpdateLayout(v.into())),
        C::Create(v) => {
            if v.connection_id.is_some() || v.expected_version.is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
            WorkspaceAction::Save(save(v))
        }
        C::Update(v) => {
            if v.connection_id.is_none() || v.expected_version.is_none() {
                return Err(WorkspaceError::InvalidInput);
            }
            WorkspaceAction::Save(save(v))
        }
        C::Get { id } => WorkspaceAction::Get(id),
        C::Delete {
            id,
            version,
            disconnect_sessions,
            session_ids,
        } => WorkspaceAction::Delete {
            id,
            version,
            disconnect: disconnect_sessions,
            sessions: session_ids,
        },
        C::ReconcileCredentials { id } => WorkspaceAction::ReconcileCredentials(id),
        C::BeginDraft { connection_id } => WorkspaceAction::BeginDraft(connection_id),
        C::ConfigureDraft { draft_id, auth } => WorkspaceAction::ConfigureDraft {
            draft: draft_id,
            kind: auth.into(),
        },
        C::ClearDraft { draft_id } => WorkspaceAction::ClearDraft(draft_id),
        C::DiscardDraft { draft_id } => WorkspaceAction::DiscardDraft(draft_id),
        C::Test(v) => WorkspaceAction::Test(TestRequest {
            connection_id: v.connection_id,
            input: v.config.map(Into::into),
            draft_id: v.draft_id,
        }),
        C::ConfirmTrust { challenge_id } => WorkspaceAction::Confirm {
            challenge: challenge_id,
            accepted: true,
        },
        C::RejectTrust { challenge_id } => WorkspaceAction::Confirm {
            challenge: challenge_id,
            accepted: false,
        },
        C::ForgetTrust(v) => WorkspaceAction::ForgetTrust {
            id: v.id,
            version: v.version,
        },
        C::Open {
            connection_id,
            columns,
            rows,
        } => WorkspaceAction::Open {
            id: connection_id,
            columns,
            rows,
        },
        C::Sessions {} => WorkspaceAction::Sessions,
        C::Resize {
            session_id,
            columns,
            rows,
        } => WorkspaceAction::Resize {
            id: session_id,
            columns,
            rows,
        },
        C::Close { session_id } => WorkspaceAction::Close(session_id),
        C::Operation { id } => WorkspaceAction::Operation(id),
        C::Cancel { id } => WorkspaceAction::Cancel(id),
        _ => return Err(WorkspaceError::InvalidInput),
    };
    Ok(value)
}
fn save(v: ConnectionSave) -> SaveRequest {
    SaveRequest {
        connection_id: v.connection_id,
        expected_version: v.expected_version,
        group_id: v.group_id,
        input: v.config.into(),
        draft_id: v.draft_id,
    }
}
fn draft(d: DraftView) -> CredentialDraftView {
    CredentialDraftView {
        intent: match d.intent {
            vigilshell_application::workspace::CredentialIntent::Keep => {
                vigilshell_desktop_protocol::workspace::CredentialIntent::Keep
            }
            vigilshell_application::workspace::CredentialIntent::Replace => {
                vigilshell_desktop_protocol::workspace::CredentialIntent::Replace
            }
            vigilshell_application::workspace::CredentialIntent::Remove => {
                vigilshell_desktop_protocol::workspace::CredentialIntent::Remove
            }
        },
        proxy_id: d.proxy_id,
        id: d.id,
        connection_id: d.connection_id,
        prepared: d.prepared,
        saved: d.saved,
        auth: d.auth.map(Into::into),
        label: d.label,
    }
}
fn operation(o: OperationView) -> WorkspaceOperation {
    WorkspaceOperation {
        file_summary: o.file_summary.map(|v| {
            vigilshell_desktop_protocol::workspace::FileOperationSummary {
                connection_id: v.connection_id,
                attempt: v.attempt,
                elapsed_ms: v.elapsed_ms,
                completed: v.completed,
                failed: v.failed,
                skipped: v.skipped,
            }
        }),
        proxy: o.proxy.map(Into::into),
        id: o.id,
        kind: o.kind,
        phase: match o.phase.as_str() {
            "trust" => OperationPhase::Trust,
            "success" => OperationPhase::Success,
            "failed" => OperationPhase::Failed,
            "cancelled" => OperationPhase::Cancelled,
            _ => OperationPhase::Working,
        },
        stage: o.stage,
        error: o.error.map(|e| e.code().into()),
        draft: o.draft.map(draft),
        connection: o.connection.map(Into::into),
        challenge: o.challenge.map(|c| TrustChallenge {
            id: c.id,
            name: c.input.name,
            host: c.input.host,
            port: c.input.port,
            fingerprint: c.key.fingerprint,
            previous_fingerprint: c.previous,
            algorithm: c.key.algorithm,
            expires_in_seconds: 60,
            temporary: c.temporary,
        }),
        session_id: o.session_id,
    }
}
fn session(s: SessionView) -> TerminalSessionView {
    TerminalSessionView {
        agent_control: s.agent_control,
        group_id: s.group_id,
        terminal: s.connection.input.terminal.clone().into(),
        id: s.id,
        operation_id: s.operation_id,
        connection_id: s.connection.id,
        connection_revision: s.connection.revision,
        name: s.connection.input.name,
        host: s.connection.input.host,
        port: s.connection.input.port,
        username: s.connection.input.username,
        phase: match s.phase.as_str() {
            "trust" => TerminalPhase::Trust,
            "authenticating" => TerminalPhase::Authenticating,
            "opening" => TerminalPhase::Opening,
            "running" => TerminalPhase::Running,
            "closed" => TerminalPhase::Closed,
            "failed" => TerminalPhase::Failed,
            "cancelled" => TerminalPhase::Cancelled,
            _ => TerminalPhase::Connecting,
        },
        error: s.error.map(|e| e.code().into()),
        exit_code: s.exit_code,
        end_reason: s.end_reason.map(|reason| TerminalEndReason {
            kind: match reason.kind {
                SessionEndKind::UserDisconnected => TerminalEndKind::UserDisconnected,
                SessionEndKind::Cancelled => TerminalEndKind::Cancelled,
                SessionEndKind::RemoteExit => TerminalEndKind::RemoteExit,
                SessionEndKind::RemoteClosed => TerminalEndKind::RemoteClosed,
                SessionEndKind::TransportError => TerminalEndKind::TransportError,
            },
            message: reason.message,
        }),
    }
}
fn reply(value: WorkspaceResult) -> WorkspaceReply {
    match value {
        WorkspaceResult::Tools(result) => match result {
            vigilshell_application::tools::ToolResult::FileEdit(edit) => {
                WorkspaceReply::FileEdit { edit: edit.into() }
            }
            vigilshell_application::tools::ToolResult::FileEntry(entry) => {
                WorkspaceReply::FileEntry {
                    entry: entry.into(),
                }
            }
            vigilshell_application::tools::ToolResult::FileOperation(page) => {
                WorkspaceReply::FileOperation { page: page.into() }
            }
            vigilshell_application::tools::ToolResult::Snapshot(snapshot) => {
                WorkspaceReply::ToolSnapshot {
                    snapshot: (*snapshot).into(),
                }
            }
            vigilshell_application::tools::ToolResult::Directory(page) => {
                WorkspaceReply::Directory { page: page.into() }
            }
            vigilshell_application::tools::ToolResult::Grant(grant) => WorkspaceReply::FileGrant {
                grant: grant.into(),
            },
            vigilshell_application::tools::ToolResult::Identities(names) => {
                WorkspaceReply::Identities {
                    names: names.into_iter().map(Into::into).collect(),
                }
            }
            vigilshell_application::tools::ToolResult::TransferPage {
                request,
                tasks,
                next,
            } => WorkspaceReply::TransferPage {
                request_id: request,
                tasks: tasks.into_iter().map(Into::into).collect(),
                next,
            },
            vigilshell_application::tools::ToolResult::Transfers(tasks) => {
                WorkspaceReply::Transfers {
                    tasks: tasks.into_iter().map(Into::into).collect(),
                }
            }
            vigilshell_application::tools::ToolResult::TransferItems(page) => {
                WorkspaceReply::TransferItems { page: page.into() }
            }
            vigilshell_application::tools::ToolResult::Done => WorkspaceReply::Done {},
        },
        WorkspaceResult::Tunnels(states) => WorkspaceReply::Tunnels {
            states: states.into_iter().map(Into::into).collect(),
        },
        WorkspaceResult::Database(DbReply::Proxies {
            proxies,
            version,
            next,
        }) => WorkspaceReply::Proxies {
            version,
            next,
            proxies: proxies.into_iter().map(Into::into).collect(),
        },
        WorkspaceResult::Database(DbReply::References {
            names: connection_names,
            version,
            next,
        }) => WorkspaceReply::ProxyReferences {
            connection_names,
            version,
            next,
        },
        WorkspaceResult::Database(DbReply::Catalogue(c)) => {
            WorkspaceReply::Catalogue { page: c.into() }
        }
        WorkspaceResult::Database(DbReply::Group(p)) => WorkspaceReply::Group { group: p.into() },
        WorkspaceResult::Database(DbReply::LayoutRevision(revision)) => {
            WorkspaceReply::Layout { revision }
        }
        WorkspaceResult::Database(DbReply::Connection(p)) => WorkspaceReply::Connection {
            connection: Box::new(p.into()),
            trust: None,
        },
        WorkspaceResult::Connection(p, t) => WorkspaceReply::Connection {
            connection: Box::new(p.into()),
            trust: t.map(Into::into),
        },
        WorkspaceResult::Draft(d) => WorkspaceReply::Draft { draft: draft(d) },
        WorkspaceResult::Operation(o) => WorkspaceReply::Operation {
            operation: Box::new(operation(*o)),
        },
        WorkspaceResult::Sessions(s) => WorkspaceReply::Sessions {
            sessions: s.into_iter().map(session).collect(),
        },
        _ => WorkspaceReply::Done {},
    }
}
#[cfg(unix)]
fn pipes(
    failed: CancellationToken,
) -> io::Result<(mpsc::Sender<TerminalFrame>, mpsc::Receiver<TerminalFrame>)> {
    use std::os::fd::FromRawFd;
    for fd in [3, 4] {
        // Node passes two private pipe/socket endpoints. Never adopt an arbitrary file.
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mode = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
        if mode != libc::S_IFIFO && mode != libc::S_IFSOCK {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // The validated inherited descriptors are exclusively owned by these stream threads.
    let mut input = unsafe { std::fs::File::from_raw_fd(3) };
    let mut output = unsafe { std::fs::File::from_raw_fd(4) };
    let (tx, mut writes) = mpsc::channel::<TerminalFrame>(32);
    let (reads, rx) = mpsc::channel::<TerminalFrame>(64);
    let reader_failed = failed.clone();
    std::thread::Builder::new()
        .name("terminal-reader".into())
        .spawn(move || {
            loop {
                match TerminalFrame::read(&mut input) {
                    Ok(Some(frame)) => {
                        if reads.blocking_send(frame).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        reader_failed.cancel();
                        break;
                    }
                    Err(_) => {
                        reader_failed.cancel();
                        break;
                    }
                }
            }
        })?;
    std::thread::Builder::new()
        .name("terminal-writer".into())
        .spawn(move || {
            while let Some(frame) = writes.blocking_recv() {
                if frame.write(&mut output).is_err() {
                    failed.cancel();
                    break;
                }
            }
        })?;
    Ok((tx, rx))
}
#[cfg(not(unix))]
fn pipes(
    _: CancellationToken,
) -> io::Result<(mpsc::Sender<TerminalFrame>, mpsc::Receiver<TerminalFrame>)> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod unlock_boundary_tests {
    use super::*;
    #[tokio::test]
    async fn negotiation_admits_owner_without_touching_storage_or_keychain() {
        let (jobs, mut pending) = mpsc::channel(8);
        let (output, _events) = mpsc::channel(8);
        let root = tempfile::tempdir().unwrap();
        let router = WorkspaceRouter::new(
            jobs,
            output,
            WorkspaceOptions {
                data_root: root.path().into(),
                helper: root.path().join("unused"),
                development: true,
                terminal_pipes: false,
            },
            "runtime".into(),
            CancellationToken::new(),
        )
        .unwrap();
        let owner = new_id();
        let reply = router
            .request(WorkspaceRequest {
                jsonrpc: "2.0".into(),
                id: "handshake".into(),
                owner: owner.clone(),
                command: WorkspaceCommand::Negotiate {
                    version: WORKSPACE_PROTOCOL_VERSION,
                },
            })
            .await
            .unwrap();
        assert!(matches!(
            reply,
            WorkspaceReply::Ready {
                connection_access: ConnectionAccessState {
                    phase: ConnectionAccessPhase::Locked,
                    ..
                },
                ..
            }
        ));
        assert!(router.owners.lock().unwrap().contains(&owner));
        assert!(pending.try_recv().is_err());
        router.access_task.lock().unwrap().take().unwrap().abort();
    }
}
