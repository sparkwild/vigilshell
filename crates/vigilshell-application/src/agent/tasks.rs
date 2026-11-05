use super::*;
impl AgentService {
    async fn history_task(&self, id: &str) -> AgentResult<HistoricalTask> {
        match self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Task {
                id: id.into(),
            }))
            .await
            .map_err(|_| "agent_storage_unavailable")?
        {
            DbReply::AgentHistory(AgentHistoryReply::Task(Some(task))) => Ok(*task),
            _ => Err("not_found"),
        }
    }
    pub(super) async fn get_task(&self, id: &str) -> AgentResult<AgentTask> {
        let history = self.history_task(id).await?;
        match serde_json::from_str::<ExecutionTask>(&history.raw_body) {
            Ok(mut task) => {
                if let Some(error) = self.executor.failure(id) {
                    task.error = Some(error.into());
                    let outcome = if matches!(task.state.dispatch, DispatchState::Attempted { .. })
                    {
                        ExecutionOutcome::Unknown {
                            reason: ExecutionStopReason::Disconnected,
                        }
                    } else {
                        ExecutionOutcome::NotExecuted {
                            reason: NotExecutedReason::Denied,
                        }
                    };
                    let _ = task.state.settle(outcome);
                }
                let mut view = self.project_task(&task);
                if (view.connection_name.is_empty() || view.entry_name.is_empty())
                    && let Ok(AgentReply::Recordings { sessions, .. }) = self
                        .history_recordings(RecordingFilter {
                            session_id: Some(task.context_id.clone()),
                            ..Default::default()
                        })
                        .await
                    && let Some(session) = sessions.first()
                {
                    view.connection_name = session.connection_name.clone();
                    view.entry_name = session.entry_name.clone();
                }
                if let Ok(AgentDbReply::OutputRange { start, end }) = self
                    .db(AgentDbCommand::OutputRange {
                        session: task.context_id.clone(),
                        task: task.id.clone(),
                    })
                    .await
                {
                    view.output_start = start;
                    view.output_end = end;
                }
                if view.phase.active()
                    && !self
                        .state
                        .lock()
                        .unwrap()
                        .contexts
                        .contains_key(&view.context_id)
                {
                    view.phase = TaskPhase::InterruptedUnknown;
                    view.termination_confirmed = false;
                }
                Ok(view)
            }
            Err(_) => Ok(self.project_history(history)),
        }
    }
    pub(super) async fn owned_task(&self, entry: &str, id: &str) -> AgentResult<AgentTask> {
        let task = self.get_task(id).await?;
        if task.entry_id != entry {
            return Err("not_found");
        }
        Ok(task)
    }
    pub(super) async fn list_tasks(
        &self,
        session: Option<String>,
        entry: Option<String>,
        connection: Option<String>,
        phase: Option<String>,
        offset: u32,
    ) -> AgentResult<AgentReply> {
        let filter = TaskHistoryFilter {
            session_id: session,
            entry_id: entry,
            connection_id: connection,
            phase,
            offset,
        };
        let DbReply::AgentHistory(AgentHistoryReply::Tasks(page)) = self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Tasks {
                filter,
            }))
            .await
            .map_err(|_| "agent_storage_unavailable")?
        else {
            return Err("agent_storage_unavailable");
        };
        let mut tasks = Vec::new();
        for row in page.tasks {
            tasks.push(self.get_task(&row.id).await?);
        }
        Ok(AgentReply::Tasks { tasks })
    }
    pub(super) async fn output(&self, id: &str, cursor: u64) -> AgentResult<AgentReply> {
        let task = self.get_task(id).await?;
        let AgentReply::Activity { page } = self
            .activity(task.context_id, Some(id.into()), cursor, true)
            .await?
        else {
            return Err("activity_recording_unavailable");
        };
        let chunks = page
            .records
            .into_iter()
            .filter_map(|record| {
                if let ActivityBody::Output { stream, text, .. } = record.body {
                    Some(OutputChunk {
                        sequence: record.sequence,
                        stream,
                        text,
                    })
                } else {
                    None
                }
            })
            .collect();
        Ok(AgentReply::Output {
            output: TaskOutput {
                screen: None,
                chunks,
                next_cursor: page.next_cursor,
                incomplete: page.incomplete,
            },
        })
    }
    pub(super) async fn start_operation(
        self: &Arc<Self>,
        entry: AccessEntry,
        call: RuntimeCall,
    ) -> AgentResult<AgentReply> {
        contexts::validate_key(&call.request_key)?;
        let context = self.context(&entry.id, &call.session_id)?;
        let _closing = context.closing.lock().await;
        let view = context.view.lock().unwrap().clone();
        if !context.created_terminal
            && context.workspace_tab.lock().unwrap().is_some()
            && matches!(
                call.operation,
                RuntimeOperation::Command { .. }
                    | RuntimeOperation::Terminal { .. }
                    | RuntimeOperation::Script { .. }
            )
        {
            return Err("use_workspace_terminal");
        }
        if context.cancel.is_cancelled() || view.phase != ContextPhase::Ready {
            return Err("session_not_ready");
        }
        let attached = context.workspace_tab.lock().unwrap().is_some();
        if attached
            && !self
                .connector
                .workspace_control_live(view.id.clone())
                .await?
        {
            return Err("terminal_control_released");
        }
        let policy = self.policy(&view.connection_id);
        if !policy.enabled() || policy.connection_revision != view.connection_revision {
            return Err("connection_changed");
        }
        if self.state.lock().unwrap().recording_blocked {
            return Err("activity_recording_unavailable");
        }
        if call.operation.is_terminal()
            && self
                .state
                .lock()
                .unwrap()
                .tasks
                .values()
                .any(|task| task.context_id == view.id && task.terminal && task.phase.active())
        {
            return Err("terminal_already_open");
        }
        self.record_policy(&context, &policy).await?;
        let task = self
            .executor
            .submit(ExecutionRequest {
                identity: ExecutionIdentity {
                    task_id: new_id(),
                    entry_id: entry.id,
                    entry_generation: entry.generation,
                    connection_id: view.connection_id,
                    connection_revision: view.connection_revision,
                    session_id: call.session_id,
                    policy_revision: policy.revision,
                },
                request_key: call.request_key,
                timeout_seconds: call.timeout_seconds,
                operation: call.operation,
            })
            .await?;
        let task = self.get_task(&task.id).await?;
        Ok(AgentReply::Task { task })
    }
    pub(super) async fn cancel_task(&self, id: &str, actor: ActivityActor) -> AgentResult<()> {
        let task = self.get_task(id).await?;
        if !task.phase.active() {
            return Ok(());
        }
        self.record(
            &task.context_id,
            Some(id.into()),
            actor,
            task.policy_revision,
            ActivityBody::Control {
                action: "stop_task".into(),
            },
        )
        .await?;
        self.executor.cancel(&task.entry_id, id).await
    }
    pub(super) async fn stop_tasks(&self, entry: Option<&str>) {
        self.executor.cancel_scope(entry, None, None).await;
        let contexts = self
            .state
            .lock()
            .unwrap()
            .contexts
            .values()
            .filter(|c| entry.is_none_or(|e| c.view.lock().unwrap().entry_id == e))
            .cloned()
            .collect::<Vec<_>>();
        for context in contexts {
            context.cancel.cancel();
            let id = context.view.lock().unwrap().id.clone();
            self.close_context(&id).await;
        }
    }
}
