use super::*;
impl AgentService {
    pub(super) async fn record_rejection(
        &self,
        entry: &AccessEntry,
        label: &str,
        code: &str,
    ) -> AgentResult<()> {
        let id = new_id();
        self.db(AgentDbCommand::recording(RecordingCommand::Create {
            session: RecordedSession {
                id: id.clone(),
                connection_id: String::new(),
                connection_name: String::new(),
                entry_id: entry.id.clone(),
                entry_name: entry.name.clone(),
                created_at: now(),
                ended_at: None,
                result: None,
                incomplete: false,
                last_sequence: 0,
                bytes: 0,
            },
        }))
        .await?;
        let task = AgentTask {
            revision: 0,
            execution_state: None,
            script_snapshot: None,
            historical: None,
            result: None,
            recovery_paths: vec![],
            file_effects: None,
            policy_snapshot: None,
            targets: vec![],
            id: new_id(),
            entry_id: entry.id.clone(),
            entry_name: entry.name.clone(),
            context_id: id,
            connection_id: String::new(),
            connection_name: String::new(),
            connection_revision: 0,
            command: label.chars().take(128).collect(),
            cwd: String::new(),
            policy_revision: 0,
            phase: TaskPhase::Denied,
            created_at: now(),
            finished_at: Some(now()),
            exit_code: None,
            error: Some(code.into()),
            termination_confirmed: true,
            operation: None,
            tool: label.chars().take(128).collect(),
            terminal: false,
            parent_task_id: None,
            output_start: 0,
            output_end: 0,
        };
        self.db(AgentDbCommand::SaveAuditTask {
            task: Box::new(task.clone()),
        })
        .await?;
        self.record_task(&task).await?;
        self.db(AgentDbCommand::recording(RecordingCommand::End {
            session_id: task.context_id.clone(),
            ended_at: now(),
            result: "denied".into(),
            incomplete: false,
        }))
        .await?;
        self.event(AgentEvent::Task { task }).await;
        Ok(())
    }
}
