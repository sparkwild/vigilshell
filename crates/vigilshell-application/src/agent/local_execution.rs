//! 共同本地执行用例：持久登记 → 排队复核 → 记录分派尝试 → SSH → 退出证据。
use super::{now, scheduling::Scheduler};
use crate::ports::*;
mod files;
mod native;
mod run;
mod script_origin;
mod scripts;
mod terminal;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
pub use terminal::TerminalDelivery;
use tokio_util::sync::CancellationToken;
use vigilshell_domain::{agent::*, workspace::WorkspaceError};

pub struct LocalExecutor {
    database: Arc<dyn WorkspaceDatabase>,
    authority: Arc<dyn ExecutionAuthority>,
    local_files: Arc<dyn AgentLocalFiles>,
    scheduler: Arc<Scheduler>,
    recording_limit_bytes: Arc<std::sync::atomic::AtomicU64>,
    admissions: tokio::sync::Mutex<()>,
    active: Mutex<HashMap<String, ActiveExecution>>,
    shutdown: CancellationToken,
    failed: Mutex<HashMap<String, &'static str>>,
    reviews_changed: tokio::sync::watch::Sender<u64>,
    events: tokio::sync::broadcast::Sender<LocalExecutionEvent>,
    terminals: Mutex<HashMap<String, tokio::sync::mpsc::Sender<terminal::TerminalRequest>>>,
}
#[derive(Clone)]
struct ActiveExecution {
    identity: ExecutionIdentity,
    cancel: CancellationToken,
}
impl LocalExecutor {
    /// recording_limit_bytes 来自现有记录设置，不另建命令记录配置。
    pub fn new(
        database: Arc<dyn WorkspaceDatabase>,
        authority: Arc<dyn ExecutionAuthority>,
        local_files: Arc<dyn AgentLocalFiles>,
        recording_limit_bytes: u64,
    ) -> AgentResult<Arc<Self>> {
        if !(16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&recording_limit_bytes) {
            return Err("invalid_recording_limit");
        }
        Ok(Arc::new(Self {
            database,
            authority,
            local_files,
            scheduler: Arc::default(),
            recording_limit_bytes: Arc::new(std::sync::atomic::AtomicU64::new(
                recording_limit_bytes,
            )),
            admissions: tokio::sync::Mutex::new(()),
            active: Mutex::default(),
            shutdown: CancellationToken::new(),
            failed: Mutex::default(),
            reviews_changed: tokio::sync::watch::channel(0).0,
            events: tokio::sync::broadcast::channel(256).0,
            terminals: Mutex::default(),
        }))
    }
    /// 启动时独占调用，先恢复未结算任务；没有重放接口。
    pub async fn recover(&self) -> AgentResult<()> {
        let _admission = self.admissions.lock().await;
        if !self.active.lock().unwrap().is_empty() {
            return Err("tasks_still_running");
        }
        self.db(ExecutionDbCommand::Recover).await?;
        self.failed.lock().unwrap().clear();
        Ok(())
    }
    pub async fn submit(
        self: &Arc<Self>,
        mut request: ExecutionRequest,
    ) -> AgentResult<ExecutionTask> {
        let _admission = self.admissions.lock().await;
        if self.shutdown.is_cancelled() {
            return Err("application_stopping");
        }
        if !self.failed.lock().unwrap().is_empty() {
            return Err("agent_storage_unavailable");
        }
        if self.active.lock().unwrap().len() >= 128 {
            return Err("execution_queue_full");
        }
        request.validate()?;
        let access = tokio::time::timeout(
            Duration::from_secs(10),
            self.authority.current(request.identity.clone()),
        )
        .await
        .map_err(|_| "authorization_timeout")?
        .map_err(|_| "access_revoked")?;
        if let ExecutionDbReply::Task(Some(task)) = self
            .db(ExecutionDbCommand::FindRequest(Box::new(request.clone())))
            .await?
        {
            return Ok(*task);
        }
        let mut source_request = None;
        let prepared = async {
            request.authorize(&access.policy)?;
            if access.policy.execution.level == OperationLevel::Operate
                && let Some((path, arguments, interpreter)) = script_command_candidate(&request)?
            {
                let interpreter = if let Some(interpreter) = interpreter {
                    Some(interpreter)
                } else {
                    access
                        .connection
                        .probe_script(path.clone())
                        .await
                        .map_err(|_| "script_entry_required")?
                };
                if let Some(interpreter) = interpreter {
                    let RuntimeOperation::Command { directory, .. } = &request.operation else {
                        unreachable!()
                    };
                    let operation = RuntimeOperation::Script {
                        source: ScriptSource::RemoteFile {
                            target: ScopedPath {
                                scope_id: ACCOUNT_SCOPE_ID.into(),
                                path: path.trim_start_matches('/').into(),
                            },
                            expected_hash: None,
                        },
                        interpreter,
                        arguments,
                        directory: directory.clone(),
                    };
                    source_request = Some(request.clone());
                    request.operation = operation;
                }
            }
            checked(&request, &access.policy)?;
            let script = if (access.policy.execution.script_review
                || matches!(
                    request.operation,
                    RuntimeOperation::Script {
                        source: ScriptSource::Inline { .. },
                        ..
                    }
                ))
                && matches!(request.operation, RuntimeOperation::Script { .. })
            {
                Some(
                    tokio::time::timeout(
                        Duration::from_secs(30),
                        access
                            .connection
                            .prepare_script(check_script(&request, &access.policy)?),
                    )
                    .await
                    .map_err(|_| "script_preparation_timeout")?
                    .map_err(|_| "script_preparation_failed")?,
                )
            } else {
                None
            };
            Ok::<_, &'static str>(script)
        }
        .await;
        let (script, denial) = match prepared {
            Ok(script) => (script, None),
            Err(code) => (None, Some(code)),
        };
        let review = if access.policy.execution.script_review {
            if let Some(prepared) = &script {
                ScriptReviewState::WaitingReview {
                    binding: ScriptReviewBinding {
                        identity: request.identity.clone(),
                        snapshot: prepared.snapshot.clone(),
                    },
                }
            } else {
                ScriptReviewState::NotRequired {}
            }
        } else {
            ScriptReviewState::NotRequired {}
        };
        let phase = if denial.is_some() {
            ExecutionPhase::Settled
        } else if matches!(review, ScriptReviewState::WaitingReview { .. }) {
            ExecutionPhase::WaitingReview
        } else {
            ExecutionPhase::Queued
        };
        let terminal = request.operation.is_terminal();
        let task = ExecutionTask {
            id: request.identity.task_id.clone(),
            entry_id: request.identity.entry_id.clone(),
            connection_id: request.identity.connection_id.clone(),
            context_id: request.identity.session_id.clone(),
            created_at: now(),
            finished_at: denial.map(|_| now()),
            revision: 0,
            request,
            source_request,
            policy: access.policy,
            state: ExecutionState {
                phase,
                review,
                dispatch: DispatchState::NotDispatched {},
                outcome: denial.map(|_| ExecutionOutcome::NotExecuted {
                    reason: NotExecutedReason::Denied,
                }),
                terminal,
            },
            phase: None,
            error: denial.map(str::to_owned),
            output_incomplete: false,
            file_report: None,
            native_report: None,
            script_snapshot: script.as_ref().map(|p| p.snapshot.clone()),
            script_staging_path: script.and_then(|p| p.staging_path),
        };
        let ExecutionDbReply::Registered { mut task, inserted } = self
            .db(ExecutionDbCommand::Register(Box::new(task)))
            .await?
        else {
            return Err("agent_storage_unavailable");
        };
        if !inserted {
            return Ok(*task);
        }
        if let Some(snapshot) = task.script_snapshot.clone() {
            let origin = self.script_origin(&task, &snapshot).await;
            let record = ActivityRecord {
                session_id: task.context_id.clone(),
                task_id: Some(task.id.clone()),
                parent_task_id: None,
                sequence: 0,
                timestamp_ms: now(),
                actor: ActivityActor::Agent,
                policy_revision: task.policy.revision,
                body: ActivityBody::Script { snapshot, origin },
            };
            let result = self
                .database
                .execute(DbCommand::Agent(AgentDbCommand::recording(
                    RecordingCommand::Append {
                        record,
                        limit_bytes: self
                            .recording_limit_bytes
                            .load(std::sync::atomic::Ordering::Relaxed),
                    },
                )))
                .await;
            match result {
                Ok(DbReply::Agent(AgentDbReply::Recorded(record))) => {
                    let _ = self
                        .events
                        .send(LocalExecutionEvent::Activity(Box::new(record)));
                }
                _ => {
                    task.error = Some("activity_recording_unavailable".into());
                    self.finish(
                        &mut task,
                        ExecutionOutcome::NotExecuted {
                            reason: NotExecutedReason::Denied,
                        },
                    )
                    .await?;
                    return Err("activity_recording_unavailable");
                }
            }
        }
        if !task.state.active() {
            let task = *task;
            let _ = self
                .events
                .send(LocalExecutionEvent::Task(Box::new(task.clone())));
            return Ok(task);
        }
        let cancel = CancellationToken::new();
        self.active.lock().unwrap().insert(
            task.id.clone(),
            ActiveExecution {
                identity: task.request.identity.clone(),
                cancel: cancel.clone(),
            },
        );
        let returned = (*task).clone();
        let _ = self
            .events
            .send(LocalExecutionEvent::Task(Box::new(returned.clone())));
        if task.state.phase == ExecutionPhase::WaitingReview {
            self.watch_review(*task, cancel);
        } else {
            self.spawn_run(*task, cancel);
        }
        Ok(returned)
    }
    pub fn set_recording_limit(&self, bytes: u64) -> AgentResult<()> {
        if !(16 * 1024 * 1024..=1024 * 1024 * 1024).contains(&bytes) {
            return Err("invalid_recording_limit");
        }
        self.recording_limit_bytes
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LocalExecutionEvent> {
        self.events.subscribe()
    }
    fn spawn_run(self: &Arc<Self>, task: ExecutionTask, cancel: CancellationToken) {
        let service = self.clone();
        tokio::spawn(async move {
            let id = task.id.clone();
            if let Err(error) = service.run(task, cancel).await {
                service.failed.lock().unwrap().insert(id.clone(), error);
                let _ = service.events.send(LocalExecutionEvent::Failed {
                    id: id.clone(),
                    error,
                });
            }
            service.active.lock().unwrap().remove(&id);
            service.terminals.lock().unwrap().remove(&id);
        });
    }
    pub async fn get(&self, entry: &str, id: &str) -> AgentResult<ExecutionTask> {
        match self
            .db(ExecutionDbCommand::Get {
                entry: entry.into(),
                id: id.into(),
            })
            .await?
        {
            ExecutionDbReply::Task(Some(task)) => {
                if let Some(error) = self.failed.lock().unwrap().get(id) {
                    return Err(error);
                }
                Ok(*task)
            }
            _ => Err("not_found"),
        }
    }
    pub async fn cancel(&self, entry: &str, id: &str) -> AgentResult<()> {
        self.get(entry, id).await?;
        if let Some(cancel) = self.active.lock().unwrap().get(id) {
            cancel.cancel.cancel();
        }
        Ok(())
    }
    pub fn active_for_context(&self, id: &str) -> usize {
        self.active
            .lock()
            .unwrap()
            .values()
            .filter(|v| v.identity.session_id == id)
            .count()
    }
    pub(super) fn active_for_entry(&self, id: &str) -> usize {
        self.active
            .lock()
            .unwrap()
            .values()
            .filter(|v| v.identity.entry_id == id)
            .count()
    }
    pub fn failure(&self, id: &str) -> Option<&'static str> {
        self.failed.lock().unwrap().get(id).copied()
    }
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
    async fn db(&self, command: ExecutionDbCommand) -> AgentResult<ExecutionDbReply> {
        match self.database.execute(DbCommand::Execution(command)).await {
            Ok(DbReply::Execution(reply)) => Ok(reply),
            Err(WorkspaceError::Conflict) => Err("request_conflict"),
            Err(WorkspaceError::NotFound) => Err("result_expired_no_replay"),
            _ => Err("agent_storage_unavailable"),
        }
    }
    async fn save(&self, task: &mut ExecutionTask) -> AgentResult<()> {
        let expected = task.revision;
        task.revision += 1;
        self.db(ExecutionDbCommand::Save {
            task: Box::new(task.clone()),
            expected,
        })
        .await?;
        let _ = self
            .events
            .send(LocalExecutionEvent::Task(Box::new(task.clone())));
        Ok(())
    }
    async fn finish(&self, task: &mut ExecutionTask, outcome: ExecutionOutcome) -> AgentResult<()> {
        task.state.settle(outcome)?;
        task.finished_at = Some(now());
        if let Some(json) = task
            .native_report
            .as_ref()
            .and_then(|r| r.json.clone())
            .or_else(|| task.file_report.as_ref().and_then(|r| r.json.clone()))
        {
            let mut start = 0;
            while start < json.len() {
                let mut end = (start + 65536).min(json.len());
                while !json.is_char_boundary(end) {
                    end -= 1;
                }
                let text = json[start..end].to_owned();
                self.output_formatted(
                    task,
                    "stdout",
                    text.clone(),
                    Some(base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        text.as_bytes(),
                    )),
                    ActivityOutputFormat::Structured,
                )
                .await?;
                start = end;
            }
        }
        self.save(task).await
    }
}

fn checked(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<Option<CheckedCommand>> {
    if request.operation.is_file_operation() {
        check_file_operation(request, policy)?;
        Ok(None)
    } else if request.operation.is_native_operation() {
        check_native_operation(request, policy)?;
        Ok(None)
    } else if matches!(request.operation, RuntimeOperation::Script { .. }) {
        check_script(request, policy)?;
        Ok(None)
    } else if request.operation.is_terminal() {
        check_terminal(request, policy)?;
        Ok(None)
    } else {
        check_command(request, policy).map(Some)
    }
}

#[derive(Clone)]
pub enum LocalExecutionEvent {
    Failed { id: String, error: &'static str },
    Task(Box<ExecutionTask>),
    Activity(Box<ActivityRecord>),
}
