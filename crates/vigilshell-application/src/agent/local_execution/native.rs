use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::atomic::Ordering;
struct Output {
    database: Arc<dyn WorkspaceDatabase>,
    identity: ExecutionIdentity,
    limit: Arc<std::sync::atomic::AtomicU64>,
    events: tokio::sync::broadcast::Sender<LocalExecutionEvent>,
}
impl NativeOperationOutput for Output {
    fn append(&self, stream: &'static str, bytes: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if bytes.len() > 65536 || !matches!(stream, "stdout" | "stderr") {
                return Err(WorkspaceError::InvalidInput);
            }
            let record = ActivityRecord {
                session_id: self.identity.session_id.clone(),
                task_id: Some(self.identity.task_id.clone()),
                parent_task_id: None,
                sequence: 0,
                timestamp_ms: now(),
                actor: ActivityActor::Agent,
                policy_revision: self.identity.policy_revision,
                body: ActivityBody::Output {
                    format: vigilshell_domain::agent::ActivityOutputFormat::Text,
                    stream: stream.into(),
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                    data: Some(STANDARD.encode(bytes)),
                },
            };
            match self
                .database
                .execute(DbCommand::Agent(AgentDbCommand::recording(
                    RecordingCommand::Append {
                        record,
                        limit_bytes: self.limit.load(Ordering::Relaxed),
                    },
                )))
                .await?
            {
                DbReply::Agent(AgentDbReply::Recorded(record)) => {
                    let _ = self
                        .events
                        .send(LocalExecutionEvent::Activity(Box::new(record)));
                    Ok(())
                }
                _ => Err(WorkspaceError::StorageUnavailable),
            }
        })
    }
}
impl LocalExecutor {
    pub(super) async fn run_native(
        &self,
        mut task: ExecutionTask,
        plan: Box<dyn PreparedNativeOperation>,
        cancel: CancellationToken,
        remaining: Option<Duration>,
    ) -> AgentResult<()> {
        task.state.phase = ExecutionPhase::Running;
        self.save(&mut task).await?;
        let control = FileExecutionControl::default();
        let output = Arc::new(Output {
            database: self.database.clone(),
            identity: task.request.identity.clone(),
            limit: self.recording_limit_bytes.clone(),
            events: self.events.clone(),
        });
        let work = plan.execute(output, control.clone());
        tokio::pin!(work);
        let timer = async {
            if let Some(d) = remaining {
                tokio::time::sleep(d).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::pin!(timer);
        let grace = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(grace);
        let mut stopping = None;
        let result = loop {
            tokio::select! {biased;
                value=&mut work=>break value.ok(),
                _=cancel.cancelled(),if stopping.is_none()=>stopping=Some(ExecutionStopReason::Cancelled),
                _=self.shutdown.cancelled(),if stopping.is_none()=>stopping=Some(ExecutionStopReason::Cancelled),
                _=&mut timer,if stopping.is_none()=>stopping=Some(ExecutionStopReason::TimedOut),
                _=&mut grace,if stopping.is_some()=>break None,
            }
            if stopping.is_some() && !control.cancel.swap(true, Ordering::AcqRel) {
                task.state.phase = ExecutionPhase::Stopping;
                self.save(&mut task).await?;
                grace
                    .as_mut()
                    .reset(tokio::time::Instant::now() + Duration::from_secs(35));
            }
        };
        let outcome = if let Some(mut report) = result {
            let outcome = if let Some(reason) = stopping {
                ExecutionOutcome::Unknown { reason }
            } else {
                report.outcome.clone().unwrap_or(ExecutionOutcome::Unknown {
                    reason: ExecutionStopReason::Disconnected,
                })
            };
            if stopping.is_some() {
                report.effects = FileCommitState::Unknown;
                report.outcome = Some(outcome.clone());
            }
            task.error = report.error.clone();
            task.native_report = Some(report);
            outcome
        } else {
            let outcome = ExecutionOutcome::Unknown {
                reason: stopping.unwrap_or(ExecutionStopReason::Disconnected),
            };
            if let Some(report) = task.native_report.as_mut() {
                report.effects = FileCommitState::Unknown;
                report.outcome = Some(outcome.clone());
                report.error = Some("native_result_unknown".into());
            }
            task.error = Some("native_result_unknown".into());
            outcome
        };
        self.finish(&mut task, outcome).await
    }
}
