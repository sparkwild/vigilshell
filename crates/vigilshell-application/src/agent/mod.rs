//! MCP 的权威状态与任务归 Application；HTTP、窗口及终端显示不拥有执行生命周期。
mod administration;
mod audit;
mod contexts;
mod inspection;
pub mod local_execution;
mod workspace_terminal;

mod controls;
mod recording;
mod scheduling;
mod session_authority;
mod task_projection;
mod tasks;
#[cfg(test)]
mod tests;

use crate::ports::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use vigilshell_domain::{
    agent::*,
    workspace::{WorkspaceError, new_id},
};

pub struct AgentService {
    database: Arc<dyn WorkspaceDatabase>,
    connector: Arc<dyn AgentConnector>,
    executor: Arc<local_execution::LocalExecutor>,
    sink: Arc<dyn AgentSink>,
    state: Mutex<State>,
    initialized: tokio::sync::OnceCell<()>,
    administration: tokio::sync::Mutex<()>,
    admissions: tokio::sync::Mutex<()>,
    inspectors: inspection::InspectorPool,
    jobs: TaskTracker,
    stop: CancellationToken,
}
#[derive(Default)]
struct State {
    recording_blocked: bool,
    recording_cleanup_started: bool,
    config: AgentConfiguration,
    last_prune: u64,
    challenges: HashMap<String, Challenge>,
    contexts: HashMap<String, Arc<Context>>,
    tasks: HashMap<String, AgentTask>,
    connections: HashMap<String, Arc<dyn AgentConnection>>,
    connection_gates: HashMap<String, Arc<tokio::sync::Mutex<()>>>,
    input_gates: HashMap<String, Arc<tokio::sync::Mutex<()>>>,
    input_cache: std::collections::VecDeque<(String, String, String)>,
    receipts: HashMap<String, (String, AgentReply)>,
}
struct Challenge {
    change: AgentMutation,
    revision: u64,
    expires: Instant,
    script: Option<ScriptReviewBinding>,
}
struct Context {
    target_tab: Option<String>,
    created_terminal: bool,
    workspace_tab: Mutex<Option<String>>,
    closing: tokio::sync::Mutex<()>,
    recorded_policies: Mutex<std::collections::HashSet<u64>>,
    unconfirmed: std::sync::atomic::AtomicBool,
    entry_generation: u64,
    view: Mutex<AgentContext>,
    cancel: CancellationToken,
}
pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(super) fn ws(_: WorkspaceError) -> &'static str {
    "connection_failed"
}

impl AgentService {
    pub fn new(
        database: Arc<dyn WorkspaceDatabase>,
        connector: Arc<dyn AgentConnector>,
        sink: Arc<dyn AgentSink>,
        local_files: Arc<dyn AgentLocalFiles>,
    ) -> Arc<Self> {
        let service = Arc::new_cyclic(|weak| Self {
            executor: local_execution::LocalExecutor::new(
                database.clone(),
                Arc::new(session_authority::SessionAuthority(weak.clone())),
                local_files,
                16 * 1024 * 1024,
            )
            .expect("valid recording limit"),
            database,
            connector,
            sink,
            state: Mutex::new(State::default()),
            initialized: tokio::sync::OnceCell::new(),
            administration: tokio::sync::Mutex::new(()),
            admissions: tokio::sync::Mutex::new(()),
            inspectors: inspection::InspectorPool::default(),
            jobs: TaskTracker::new(),
            stop: CancellationToken::new(),
        });
        let mut events = service.executor.subscribe();
        let owner = Arc::downgrade(&service);
        let stop = service.stop.clone();
        service.jobs.spawn(async move {
            loop {
                let event = tokio::select! {_=stop.cancelled()=>break,event=events.recv()=>event};
                let Some(service) = owner.upgrade() else {
                    break;
                };
                match event {
                    Ok(event) => service.execution_event(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(_) => {
                        service.event(AgentEvent::Configuration {}).await;
                    }
                }
            }
        });
        service
    }
    async fn db(&self, command: AgentDbCommand) -> AgentResult<AgentDbReply> {
        let recording = matches!(&command, AgentDbCommand::SaveAuditTask { .. })
            || matches!(&command,AgentDbCommand::Recording(c) if matches!(c.as_ref(),RecordingCommand::Create{..}|RecordingCommand::Append{..}|RecordingCommand::End{..}));
        let result = self.database.execute(DbCommand::Agent(command)).await;
        if recording && result.is_err() {
            self.recording_failed().await;
        }
        match result.map_err(agent_storage_error)? {
            DbReply::Agent(r) => Ok(r),
            _ => Err("agent_storage_unavailable"),
        }
    }
    async fn initialize(&self) -> AgentResult<()> {
        self.initialized
            .get_or_try_init(|| async {
                let AgentDbReply::Configuration(c) = self.db(AgentDbCommand::Configuration).await?
                else {
                    return Err("agent_storage_unavailable");
                };
                self.executor.recover().await?;
                self.db(AgentDbCommand::recording(RecordingCommand::Recover {
                    now: now(),
                }))
                .await?;
                self.executor
                    .set_recording_limit(u64::from(c.settings.storage_mib) * 1024 * 1024)?;
                self.state.lock().unwrap().config = c;
                Ok(())
            })
            .await
            .map(|_| ())
    }
    async fn event(&self, event: AgentEvent) {
        let _ = self.sink.publish(event).await;
    }
    async fn refresh_configuration(&self) -> AgentResult<()> {
        let AgentDbReply::Configuration(config) = self.db(AgentDbCommand::Configuration).await?
        else {
            return Err("agent_storage_unavailable");
        };
        let mut state = self.state.lock().unwrap();
        // 连接删除由工作区事务推进版本；迟到的读取不能覆盖已提交的新配置。
        if config.settings.revision > state.config.settings.revision {
            self.executor
                .set_recording_limit(u64::from(config.settings.storage_mib) * 1024 * 1024)?;
            state.config = config;
        }
        Ok(())
    }
    fn policy(&self, id: &str) -> ConnectionPolicy {
        self.state
            .lock()
            .unwrap()
            .config
            .policies
            .iter()
            .find(|p| p.connection_id == id)
            .map(|settings| {
                let mut policy = settings.policy.clone();
                if !settings.access_enabled {
                    policy.execution.level = OperationLevel::Disabled;
                }
                policy
            })
            .unwrap_or_else(|| ConnectionPolicy {
                connection_id: id.into(),
                ..Default::default()
            })
    }
    fn authenticate(&self, digest: &str) -> AgentResult<AccessEntry> {
        let state = self.state.lock().unwrap();
        if self.stop.is_cancelled() || !state.config.settings.enabled || digest.len() != 64 {
            return Err("access_denied");
        }
        state
            .config
            .entries
            .iter()
            .find(|v| {
                v.entry.enabled
                    && v.digest.len() == 64
                    && v.digest
                        .bytes()
                        .zip(digest.bytes())
                        .fold(0u8, |x, (a, b)| x | (a ^ b))
                        == 0
            })
            .map(|e| e.entry.clone())
            .ok_or("access_denied")
    }
    fn valid_entry(&self, entry: &str, generation: u64) -> bool {
        let s = self.state.lock().unwrap();
        s.config.settings.enabled
            && s.config
                .entries
                .iter()
                .any(|e| e.entry.id == entry && e.entry.generation == generation && e.entry.enabled)
            && !self.stop.is_cancelled()
    }
    pub async fn request(self: &Arc<Self>, operation: AgentOperation) -> AgentResult<AgentReply> {
        if matches!(operation, AgentOperation::ActivitySources {}) {
            return match self
                .database
                .execute(DbCommand::AgentHistory(
                    AgentHistoryCommand::ActivitySources,
                ))
                .await
            {
                Ok(DbReply::AgentHistory(AgentHistoryReply::ActivitySources(sources))) => {
                    Ok(AgentReply::ActivitySources { sources })
                }
                _ => Err("agent_storage_unavailable"),
            };
        }
        if let AgentOperation::ActivitySummaries { filter } = operation {
            return match self
                .database
                .execute(DbCommand::AgentHistory(
                    AgentHistoryCommand::ActivitySummaries { filter },
                ))
                .await
            {
                Ok(DbReply::AgentHistory(AgentHistoryReply::ActivitySummaries(mut items))) => {
                    let state = self.state.lock().unwrap();
                    for item in &mut items {
                        if item.phase.as_ref().is_some_and(TaskPhase::active)
                            && !state.contexts.contains_key(&item.session_id)
                        {
                            item.uncertain = true;
                        }
                    }
                    let more = items.len() > 100;
                    items.truncate(100);
                    Ok(AgentReply::ActivitySummaries { items, more })
                }
                _ => Err("agent_storage_unavailable"),
            };
        }
        match &operation {
            AgentOperation::Activity {
                session_id,
                task_id,
                cursor,
                include_output,
            } => {
                return self
                    .activity(
                        session_id.clone(),
                        task_id.clone(),
                        *cursor,
                        include_output.unwrap_or(true),
                    )
                    .await;
            }
            AgentOperation::Recordings { filter } => {
                return self.history_recordings(filter.clone()).await;
            }
            AgentOperation::Task { task_id } => {
                return self
                    .get_task(task_id)
                    .await
                    .map(|task| AgentReply::Task { task });
            }
            AgentOperation::Tasks {
                session_id,
                entry_id,
                connection_id,
                phase,
                offset,
            } => {
                return self
                    .list_tasks(
                        session_id.clone(),
                        entry_id.clone(),
                        connection_id.clone(),
                        phase.clone(),
                        *offset,
                    )
                    .await;
            }
            AgentOperation::Output { task_id, cursor } => {
                return self.output(task_id, *cursor).await;
            }
            AgentOperation::LoadScreen { task_id } => return self.load_screen(task_id).await,
            _ => {}
        }
        self.initialize().await?;
        let prune = {
            let mut state = self.state.lock().unwrap();
            if matches!(&operation, AgentOperation::Configuration {})
                && now().saturating_sub(state.last_prune) >= 60_000
            {
                state.last_prune = now();
                Some(state.config.settings.clone())
            } else {
                None
            }
        };
        if let Some(settings) = prune {
            self.db(AgentDbCommand::recording(RecordingCommand::Prune {
                before: now().saturating_sub(settings.retention_days as u64 * 86_400_000),
                limit_bytes: settings.storage_mib as u64 * 1024 * 1024,
            }))
            .await?;
        }
        match operation {
            AgentOperation::ActivitySummaries { .. }
            | AgentOperation::ActivitySources {}
            | AgentOperation::Activity { .. }
            | AgentOperation::Recordings { .. }
            | AgentOperation::Task { .. }
            | AgentOperation::Tasks { .. }
            | AgentOperation::Output { .. }
            | AgentOperation::LoadScreen { .. } => {
                unreachable!()
            }
            AgentOperation::RejectScript { task_id } => {
                let view = self.get_task(&task_id).await?;
                let task = self
                    .executor
                    .reject_script(&view.entry_id, &task_id)
                    .await?;
                Ok(AgentReply::Task {
                    task: self.project_task(&task),
                })
            }
            AgentOperation::PreviewPolicy { policy } => {
                policy.validate()?;
                let state = self.state.lock().unwrap();
                let context = state
                    .contexts
                    .values()
                    .map(|context| context.view.lock().unwrap().clone())
                    .find(|c| {
                        c.connection_id == policy.connection_id
                            && c.connection_revision == policy.connection_revision
                            && c.phase == ContextPhase::Ready
                            && c.support.is_some()
                    });
                let support = context.as_ref().and_then(|c| c.support.as_ref());
                Ok(AgentReply::PolicyCapabilities {
                    tools: policy
                        .tool_capabilities(support, context.is_some())
                        .into_iter()
                        .map(|mut tool| {
                            if !state.config.policies.iter().any(|p| {
                                p.connection_id == policy.connection_id && p.access_enabled
                            }) {
                                tool.usable = false;
                                tool.reason = Some("policy_access_closed".into());
                            }
                            tool
                        })
                        .collect(),
                    environment_known: support.is_some(),
                })
            }
            AgentOperation::OpenPolicyBrowser {
                connection_id,
                connection_revision,
            } => {
                self.open_policy_browser(connection_id, connection_revision)
                    .await
            }
            AgentOperation::ReadPolicyResources { browser_id, query } => {
                self.read_policy_resources(&browser_id, query).await
            }
            AgentOperation::ClosePolicyBrowser { browser_id } => {
                self.close_policy_browser(&browser_id).await;
                Ok(AgentReply::Done {})
            }
            AgentOperation::RetryRecording {} => {
                if !self.state.lock().unwrap().recording_blocked {
                    return Ok(AgentReply::Done {});
                }
                self.stop_tasks(None).await;
                let settings = self.state.lock().unwrap().config.settings.clone();
                self.db(AgentDbCommand::recording(RecordingCommand::Prune {
                    before: now().saturating_sub(u64::from(settings.retention_days) * 86_400_000),
                    limit_bytes: u64::from(settings.storage_mib) * 1024 * 1024,
                }))
                .await?;
                self.db(AgentDbCommand::recording(RecordingCommand::Probe))
                    .await?;
                {
                    let mut state = self.state.lock().unwrap();
                    state.recording_blocked = false;
                    state.recording_cleanup_started = false;
                }
                self.event(AgentEvent::Configuration {}).await;
                Ok(AgentReply::Done {})
            }

            AgentOperation::CloseSession { session_id } => {
                self.end_context(&session_id, false).await?;
                Ok(AgentReply::Done {})
            }
            AgentOperation::Reject { digest, tool, code } => {
                if !["unsupported_tool", "invalid_tool_arguments"].contains(&code.as_str()) {
                    return Err("invalid_request");
                }
                let entry = self.authenticate(&digest)?;
                self.record_rejection(&entry, &tool, &code).await?;
                Ok(AgentReply::Done {})
            }
            AgentOperation::Authenticate { digest } => {
                let entry = self.authenticate(&digest)?;
                let endpoint_revision =
                    self.state.lock().unwrap().config.settings.endpoint_revision;
                Ok(AgentReply::Authenticated {
                    entry,
                    endpoint_revision,
                })
            }
            AgentOperation::Observations {} => match self.db(AgentDbCommand::Observations).await? {
                AgentDbReply::Observations(observations) => {
                    Ok(AgentReply::Observations { observations })
                }
                _ => Err("agent_storage_unavailable"),
            },
            AgentOperation::Observe {
                entry_id,
                generation,
                port,
                endpoint_revision,
            } => {
                let settings = self.state.lock().unwrap().config.settings.clone();
                if settings.endpoint_revision != endpoint_revision
                    || !self.valid_entry(&entry_id, generation)
                    || settings.port != port
                {
                    return Err("access_revoked");
                }
                let at = now();
                self.db(AgentDbCommand::Observe {
                    observation: AccessObservation {
                        entry_id,
                        endpoint_revision,
                        generation,
                        endpoint: format!("http://127.0.0.1:{port}/mcp"),
                        first_at: at,
                        last_at: at,
                    },
                })
                .await?;
                self.event(AgentEvent::AccessChanged {}).await;
                Ok(AgentReply::Done {})
            }
            AgentOperation::Configuration {} => {
                self.refresh_configuration().await?;
                let state = self.state.lock().unwrap();
                Ok(AgentReply::Configuration {
                    protections: system_protections(),
                    configuration: state.config.clone(),
                    recording_error: state
                        .recording_blocked
                        .then(|| "activity_recording_unavailable".into()),
                })
            }
            AgentOperation::Prepare { change } => self.prepare(change).await,
            AgentOperation::Commit { challenge_id } => self.commit(&challenge_id).await,
            AgentOperation::Abort { challenge_id } => {
                self.state.lock().unwrap().challenges.remove(&challenge_id);
                Ok(AgentReply::Done {})
            }
            AgentOperation::Invoke { digest, call } => {
                let entry = self.authenticate(&digest)?;
                // 业务执行入口登记步骤；轮询与读取不额外生成审计任务。
                self.invoke(entry, call.into_action()?).await
            }
            AgentOperation::Contexts {} => Ok(AgentReply::Contexts {
                contexts: self
                    .state
                    .lock()
                    .unwrap()
                    .contexts
                    .values()
                    .map(|c| c.view.lock().unwrap().clone())
                    .collect(),
            }),
            AgentOperation::SaveScreen {
                task_id,
                checkpoint,
            } => self.save_screen(&task_id, checkpoint).await,
            AgentOperation::Cancel { task_id } => {
                self.cancel_task(&task_id, ActivityActor::User).await?;
                Ok(AgentReply::Done {})
            }
            AgentOperation::Stop {} => {
                self.stop_tasks(None).await;
                Ok(AgentReply::Done {})
            }
        }
    }
    async fn invoke(
        self: &Arc<Self>,
        entry: AccessEntry,
        call: AgentAction,
    ) -> AgentResult<AgentReply> {
        let call = match call.into_runtime() {
            Ok(call) => return self.start_operation(entry, call).await,
            Err(call) => *call,
        };
        match call {
            AgentAction::WorkspaceTerminal {
                session_id,
                command,
            } => self.workspace_terminal(entry, session_id, command).await,
            AgentAction::ConnectionsList {} => self.connections().await,
            AgentAction::SessionOpen {
                tab_id,
                connection_id,
                request_key,
            } => {
                self.open_context(entry, &connection_id, &request_key, tab_id)
                    .await
            }
            AgentAction::SessionGet { session_id } => Ok(AgentReply::Context {
                context: self
                    .context(&entry.id, &session_id)?
                    .view
                    .lock()
                    .unwrap()
                    .clone(),
            }),
            AgentAction::SessionClose {
                session_id,
                request_key,
            } => {
                contexts::validate_key(&request_key)?;
                self.context(&entry.id, &session_id)?;
                self.end_context(&session_id, true).await?;
                Ok(AgentReply::Done {})
            }
            AgentAction::CapabilitiesGet { session_id } => {
                let context = self
                    .context(&entry.id, &session_id)?
                    .view
                    .lock()
                    .unwrap()
                    .clone();
                let policy = self.policy(&context.connection_id);
                let tools = policy.tool_capabilities(
                    context.support.as_ref(),
                    context.phase == ContextPhase::Ready,
                );
                Ok(AgentReply::Capabilities {
                    tools,
                    protections: system_protections(),
                    policy: Box::new(policy),
                    support: context.support,
                })
            }
            AgentAction::TasksList {
                session_id,
                connection_id,
                phase,
                offset,
            } => {
                self.list_tasks(
                    session_id,
                    Some(entry.id),
                    connection_id,
                    phase,
                    offset.unwrap_or(0),
                )
                .await
            }
            AgentAction::TaskGet { task_id } => Ok(AgentReply::Task {
                task: self.owned_task(&entry.id, &task_id).await?,
            }),
            AgentAction::TaskReadOutput {
                task_id, cursor, ..
            } => {
                self.owned_task(&entry.id, &task_id).await?;
                self.output(&task_id, cursor.unwrap_or(0)).await
            }
            AgentAction::TaskCancel {
                task_id,
                request_key,
            }
            | AgentAction::TerminalClose {
                task_id,
                request_key,
            } => {
                contexts::validate_key(&request_key)?;
                self.owned_task(&entry.id, &task_id).await?;
                self.cancel_task(&task_id, ActivityActor::Agent).await?;
                Ok(AgentReply::Done {})
            }
            AgentAction::TerminalWrite {
                task_id,
                text,
                request_key,
            } => {
                self.terminal_input(
                    &entry.id,
                    &task_id,
                    request_key,
                    TerminalControl::Write { text },
                )
                .await
            }
            AgentAction::TerminalResize {
                task_id,
                columns,
                rows,
                request_key,
            } => {
                self.terminal_input(
                    &entry.id,
                    &task_id,
                    request_key,
                    TerminalControl::Resize { columns, rows },
                )
                .await
            }
            _ => Err("unsupported_tool"),
        }
    }
    pub async fn shutdown(self: &Arc<Self>) {
        self.stop.cancel();
        self.executor.shutdown();
        self.close_policy_browsers().await;
        self.stop_tasks(None).await;
        self.jobs.close();
        self.jobs.wait().await;
    }
}

mod policy_settings;
