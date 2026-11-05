use super::*;

impl AgentService {
    pub(super) async fn open_context_terminal(
        self: &Arc<Self>,
        context: &Arc<Context>,
    ) -> AgentResult<()> {
        let view = context.view.lock().unwrap().clone();
        let policy = self.policy(&view.connection_id);
        if !policy.enabled() || policy.connection_revision != view.connection_revision {
            return Err("connection_changed");
        }
        if context.target_tab.is_some() && policy.execution.level != OperationLevel::Operate {
            return Err("connection_denied");
        }
        let count = self
            .state
            .lock()
            .unwrap()
            .contexts
            .values()
            .filter(|c| {
                c.workspace_tab.lock().unwrap().is_some()
                    && !c.cancel.is_cancelled()
                    && c.view.lock().unwrap().connection_id == view.connection_id
            })
            .count();
        if count >= usize::from(policy.execution.limits.concurrent_terminals) {
            return Err("terminal_control_busy");
        }
        let tab = self
            .connector
            .open_workspace(
                WorkspaceAgentIdentity {
                    terminal_seconds: policy.execution.limits.terminal_seconds,
                    idle_seconds: policy.execution.limits.terminal_idle_seconds,
                    entry_id: view.entry_id.clone(),
                    entry_name: view.entry_name.clone(),
                    context_id: view.id.clone(),
                    connection_id: view.connection_id.clone(),
                    connection_revision: view.connection_revision,
                },
                context.target_tab.clone(),
            )
            .await?;
        *context.workspace_tab.lock().unwrap() = Some(tab.id.clone());
        if context.cancel.is_cancelled()
            || self.stop.is_cancelled()
            || !self.valid_entry(&view.entry_id, context.entry_generation)
            || self.policy(&view.connection_id).revision != policy.revision
        {
            self.connector
                .release_workspace(view.id.clone(), true)
                .await?;
            return Err("access_revoked");
        }
        {
            let mut view = context.view.lock().unwrap();
            view.terminal = Some(tab);
            view.phase = ContextPhase::Ready;
            view.revision += 1;
        }
        self.watch_workspace(context.clone(), view.id);
        Ok(())
    }

    fn watch_workspace(self: &Arc<Self>, context: Arc<Context>, id: String) {
        let service = Arc::downgrade(self);
        self.jobs.spawn(async move {
            loop {
                tokio::select! { _ = context.cancel.cancelled() => {}, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
                let Some(service) = service.upgrade() else { break };
                if service.stop.is_cancelled() { break; }
                if context.cancel.is_cancelled() || !service.connector.workspace_control_live(id.clone()).await.unwrap_or(false) {
                    let _ = service.end_context(&id, false).await;
                    break;
                }
            }
        });
    }

    pub(super) async fn workspace_terminal(
        self: &Arc<Self>,
        entry: AccessEntry,
        session_id: String,
        command: WorkspaceTerminalAction,
    ) -> AgentResult<AgentReply> {
        let _admit = if matches!(command, WorkspaceTerminalAction::Takeover { .. }) {
            Some(self.admissions.lock().await)
        } else {
            None
        };
        let context = self.context(&entry.id, &session_id)?;
        let _closing = context.closing.lock().await;
        let view = context.view.lock().unwrap().clone();
        let policy = self.policy(&view.connection_id);
        if context.cancel.is_cancelled() || view.phase != ContextPhase::Ready {
            return Err("session_not_ready");
        }
        if !self.valid_entry(&entry.id, entry.generation)
            || (matches!(
                command,
                WorkspaceTerminalAction::Takeover { .. } | WorkspaceTerminalAction::WriteTab { .. }
            ) && policy.execution.level != OperationLevel::Operate)
            || policy.connection_revision != view.connection_revision
        {
            return Err("connection_denied");
        }
        let profile = self
            .database
            .execute(DbCommand::Get(view.connection_id.clone()))
            .await
            .map_err(ws)?;
        if !matches!(profile, DbReply::Connection(ref c) if !c.deleting && c.revision == view.connection_revision)
        {
            return Err("connection_changed");
        }
        if self.state.lock().unwrap().recording_blocked {
            return Err("activity_recording_unavailable");
        }
        let initial = context.workspace_tab.lock().unwrap().is_none();
        if initial && matches!(command, WorkspaceTerminalAction::Takeover { .. }) {
            let state = self.state.lock().unwrap();
            let attached = state
                .contexts
                .values()
                .filter(|c| {
                    c.workspace_tab.lock().unwrap().is_some()
                        && !c.cancel.is_cancelled()
                        && c.view.lock().unwrap().connection_id == view.connection_id
                })
                .count();
            let standalone = state
                .tasks
                .values()
                .filter(|t| t.connection_id == view.connection_id && t.terminal && t.phase.active())
                .count();
            if attached + standalone >= usize::from(policy.execution.limits.concurrent_terminals) {
                return Err("terminal_control_busy");
            }
        }
        match &command {
            WorkspaceTerminalAction::Takeover {
                request_key,
                tab_id,
            } => {
                contexts::validate_key(request_key)?;
                if self.executor.active_for_context(&session_id) > 0 {
                    return Err("terminal_control_busy");
                }
                if context
                    .workspace_tab
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|old| old != tab_id)
                {
                    return Err("terminal_already_attached");
                }
                self.record(
                    &session_id,
                    None,
                    ActivityActor::Agent,
                    policy.revision,
                    ActivityBody::Control {
                        action: "request_terminal_control".into(),
                    },
                )
                .await?;
            }
            WorkspaceTerminalAction::WriteTab {
                request_key, text, ..
            } => {
                contexts::validate_key(request_key)?;
                if text.is_empty() || text.len() > 32 * 1024 {
                    return Err("input_too_large");
                }
                self.record(
                    &session_id,
                    None,
                    ActivityActor::Agent,
                    policy.revision,
                    ActivityBody::Input {
                        request_key: request_key.clone(),
                        text: text.clone(),
                        delivered: false,
                    },
                )
                .await?;
            }
            _ => {}
        }
        let result = self
            .connector
            .workspace_terminal(
                WorkspaceAgentIdentity {
                    terminal_seconds: policy.execution.limits.terminal_seconds,
                    idle_seconds: policy.execution.limits.terminal_idle_seconds,
                    entry_id: entry.id,
                    entry_name: entry.name,
                    context_id: session_id.clone(),
                    connection_id: view.connection_id,
                    connection_revision: view.connection_revision,
                },
                command.clone(),
            )
            .await?;
        if let WorkspaceTerminalReply::Control { tab } = &result {
            *context.workspace_tab.lock().unwrap() = Some(tab.id.clone());
            {
                let mut view = context.view.lock().unwrap();
                view.terminal = Some(tab.clone());
                view.revision += 1;
            }
            if initial {
                self.watch_workspace(context.clone(), session_id.clone());
            }
            let updated = context.view.lock().unwrap().clone();
            self.publish_context(updated).await?;
        }
        if let WorkspaceTerminalAction::WriteTab {
            request_key, text, ..
        } = command
        {
            self.record(
                &session_id,
                None,
                ActivityActor::Agent,
                policy.revision,
                ActivityBody::Input {
                    request_key,
                    text,
                    delivered: true,
                },
            )
            .await?;
        }
        Ok(AgentReply::WorkspaceTerminal { result })
    }
}
