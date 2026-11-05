use super::*;
impl AgentService {
    pub(super) fn project_task(&self, task: &ExecutionTask) -> AgentTask {
        let state = self.state.lock().unwrap();
        let context = state
            .contexts
            .get(&task.context_id)
            .map(|c| c.view.lock().unwrap().clone());
        let operation = &task.request.operation;
        let cwd = match operation {
            RuntimeOperation::Command { directory, .. }
            | RuntimeOperation::Script { directory, .. }
            | RuntimeOperation::Terminal { directory, .. } => normalize_path(
                "/",
                if directory.path.is_empty() {
                    "."
                } else {
                    &directory.path
                },
            )
            .unwrap_or_default(),
            _ => String::new(),
        };
        let command = match task
            .source_request
            .as_ref()
            .map(|r| &r.operation)
            .unwrap_or(operation)
        {
            RuntimeOperation::Command { command, .. } => command.clone(),
            RuntimeOperation::Script {
                source: ScriptSource::RemoteFile { target, .. },
                ..
            } => format!("script /{}", target.path),
            _ => operation.label().into(),
        };
        let (result, recovery_paths, file_effects) = if let Some(report) = &task.native_report {
            (
                report.json.clone(),
                report.recovery_paths.clone(),
                Some(report.effects),
            )
        } else if let Some(report) = &task.file_report {
            (
                report.json.clone(),
                report.recovery_paths.clone(),
                Some(report.commit),
            )
        } else {
            (
                None,
                if matches!(task.state.outcome, Some(ExecutionOutcome::Completed { .. })) {
                    vec![]
                } else {
                    task.script_staging_path.iter().cloned().collect()
                },
                None,
            )
        };
        AgentTask {
            policy_snapshot: Some(Box::new(task.policy.execution.clone())),
            revision: task.revision,
            execution_state: Some(Box::new(task.state.clone())),
            script_snapshot: task.script_snapshot.clone().map(Box::new),
            historical: None,
            result,
            recovery_paths,
            file_effects,
            entry_name: context
                .as_ref()
                .map_or_else(String::new, |c| c.entry_name.clone()),
            id: task.id.clone(),
            entry_id: task.entry_id.clone(),
            context_id: task.context_id.clone(),
            connection_id: task.connection_id.clone(),
            connection_name: context
                .as_ref()
                .map_or_else(String::new, |c| c.connection_name.clone()),
            connection_revision: task.request.identity.connection_revision,
            command,
            cwd,
            policy_revision: task.policy.revision,
            targets: operation.business_targets().unwrap_or_default(),
            phase: task.view_phase(),
            created_at: task.created_at,
            finished_at: task.finished_at,
            exit_code: match task.state.outcome {
                Some(ExecutionOutcome::Completed { exit_code }) => Some(exit_code),
                _ => None,
            },
            error: task.error.clone(),
            termination_confirmed: task
                .state
                .outcome
                .as_ref()
                .is_some_and(ExecutionOutcome::termination_confirmed),
            operation: Some(Box::new(operation.clone())),
            tool: operation.label().into(),
            terminal: task.state.terminal,
            parent_task_id: None,
            output_start: 0,
            output_end: 0,
        }
    }
    pub(super) fn project_history(&self, history: HistoricalTask) -> AgentTask {
        let display = &history.display;
        let phase = if history.active {
            TaskPhase::InterruptedUnknown
        } else {
            display
                .phase
                .as_ref()
                .and_then(|s| serde_json::from_value(serde_json::Value::String(s.clone())).ok())
                .unwrap_or(TaskPhase::InterruptedUnknown)
        };
        AgentTask {
            policy_snapshot: None,
            revision: 0,
            execution_state: None,
            script_snapshot: None,
            result: None,
            recovery_paths: vec![],
            file_effects: None,
            entry_name: display.entry_name.clone().unwrap_or_default(),
            id: history.id.clone(),
            entry_id: history.entry_id.clone(),
            context_id: display.context_id.clone().unwrap_or_default(),
            connection_id: history.connection_id.clone(),
            connection_name: display.connection_name.clone().unwrap_or_default(),
            connection_revision: 0,
            command: display.command.clone().unwrap_or_default(),
            cwd: display.cwd.clone().unwrap_or_default(),
            policy_revision: 0,
            targets: vec![],
            phase,
            created_at: history.created_at,
            finished_at: display.finished_at,
            exit_code: display.exit_code,
            error: display.error.clone(),
            termination_confirmed: !history.active,
            operation: None,
            tool: display.tool.clone().unwrap_or_default(),
            terminal: false,
            parent_task_id: None,
            output_start: display.output_start.unwrap_or(0),
            output_end: display.output_end.unwrap_or(0),
            historical: Some(Box::new(history)),
        }
    }
    pub(super) async fn execution_event(&self, event: local_execution::LocalExecutionEvent) {
        match event {
            local_execution::LocalExecutionEvent::Failed { id, error } => {
                if let Ok(task) = self.get_task(&id).await {
                    self.event(AgentEvent::Task { task }).await;
                }
                if matches!(
                    error,
                    "activity_recording_unavailable" | "agent_storage_unavailable"
                ) {
                    self.recording_failed().await;
                }
            }
            local_execution::LocalExecutionEvent::Task(task) => {
                // 步骤记录保留事件当时的状态，不能被随后读取的较新任务覆盖。
                if self.record_task(&self.project_task(&task)).await.is_err() {
                    return;
                }
                let view = self
                    .get_task(&task.id)
                    .await
                    .unwrap_or_else(|_| self.project_task(&task));
                {
                    let mut state = self.state.lock().unwrap();
                    if task.state.active() {
                        state.tasks.insert(task.id.clone(), view.clone());
                    } else {
                        state.tasks.remove(&task.id);
                        state.input_gates.remove(&task.id);
                    }
                    if matches!(
                        task.state.outcome,
                        Some(
                            ExecutionOutcome::Unknown { .. }
                                | ExecutionOutcome::Partial {
                                    termination_confirmed: false,
                                    ..
                                }
                        )
                    ) && let Some(context) = state.contexts.get(&task.context_id)
                    {
                        context
                            .unconfirmed
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                }
                if task.state.phase == ExecutionPhase::WaitingReview {
                    self.event(AgentEvent::Attention {
                        id: task.id.clone(),
                        connection_id: task.connection_id.clone(),
                        code: "script_review_required".into(),
                    })
                    .await;
                }
                self.event(AgentEvent::Task { task: view }).await;
            }
            local_execution::LocalExecutionEvent::Activity(record) => {
                self.event(AgentEvent::Activity { record: *record }).await;
            }
        }
    }
}
