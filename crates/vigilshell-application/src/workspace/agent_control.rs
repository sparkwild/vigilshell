use super::*;
use sha2::{Digest, Sha256};
use vigilshell_domain::{
    agent::*,
    workspace_control::{AgentControl, ControlError},
};

fn control_error(error: ControlError) -> &'static str {
    match error {
        ControlError::Busy => "terminal_control_busy",
        ControlError::Released => "terminal_control_released",
        ControlError::RequestConflict => "request_conflict",
        ControlError::Capacity => "receipt_capacity",
        ControlError::InvalidRequest => "invalid_input",
    }
}
fn expired(session: &Session) -> bool {
    let now = std::time::Instant::now();
    session.control_deadline.is_some_and(|at| now >= at)
        || session.control_idle_deadline.is_some_and(|at| now >= at)
}
impl Session {
    pub(super) fn control_activity(&mut self) {
        self.control_idle_deadline = self
            .control_idle_seconds
            .map(|s| std::time::Instant::now() + Duration::from_secs(u64::from(s)));
    }
    pub(super) fn agent_input_allowed(&self, control: &AgentControl) -> bool {
        !expired(self) && self.control.check(control).is_ok()
    }
}
fn tab(session: &Session) -> WorkspaceTerminalTab {
    WorkspaceTerminalTab {
        id: session.view.id.clone(),
        name: session.view.connection.input.name.clone(),
        connection_id: session.view.connection.id.clone(),
        control: session.view.agent_control.clone(),
    }
}
fn owned(
    session: &Session,
    identity: &WorkspaceAgentIdentity,
    lease: &str,
) -> AgentResult<AgentControl> {
    let control = AgentControl {
        entry_id: identity.entry_id.clone(),
        context_id: identity.context_id.clone(),
        lease_id: lease.into(),
    };
    if session.view.connection.id != identity.connection_id
        || session.view.connection.revision != identity.connection_revision
        || session.view.phase != "running"
    {
        return Err("terminal_unavailable");
    }
    if expired(session) {
        return Err("terminal_control_released");
    }
    session.control.check(&control).map_err(control_error)?;
    Ok(control)
}
impl WorkspaceService {
    pub fn agent_tabs(&self, connection_id: &str, revision: u64) -> Vec<WorkspaceTerminalTab> {
        let state = self.state.lock().unwrap();
        let mut tabs: Vec<_> = state
            .sessions
            .values()
            .filter(|s| {
                s.view.phase == "running"
                    && s.view.connection.id == connection_id
                    && s.view.connection.revision == revision
            })
            .map(tab)
            .collect();
        tabs.sort_by(|a, b| a.id.cmp(&b.id));
        tabs
    }

    pub async fn open_agent_terminal(
        self: &Arc<Self>,
        identity: WorkspaceAgentIdentity,
        target: Option<String>,
    ) -> AgentResult<WorkspaceTerminalTab> {
        if let Some(tab_id) = target {
            return match self
                .agent_terminal(
                    identity.clone(),
                    WorkspaceTerminalAction::Takeover {
                        tab_id,
                        request_key: identity.context_id.clone(),
                    },
                )
                .await?
            {
                WorkspaceTerminalReply::Control { tab } => Ok(tab),
                _ => Err("workspace_unavailable"),
            };
        }
        let owner = self
            .state
            .lock()
            .unwrap()
            .display_owner
            .clone()
            .ok_or("workspace_unavailable")?;
        let operation = match self
            .start_open_owned(
                &owner,
                &identity.connection_id.clone(),
                100,
                30,
                Some(identity),
            )
            .await
            .map_err(|e| e.code())?
        {
            WorkspaceResult::Operation(op) => op,
            _ => return Err("workspace_unavailable"),
        };
        let id = operation.session_id.ok_or("workspace_unavailable")?;
        let started = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                {
                    let state = self.state.lock().unwrap();
                    let s = state.sessions.get(&id).ok_or("terminal_unavailable")?;
                    match s.view.phase.as_str() {
                        "running" => return Ok(tab(s)),
                        "failed" | "closed" | "cancelled" => {
                            return Err(s.view.error.map_or("terminal_unavailable", |e| e.code()));
                        }
                        _ => {}
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or(Err("connection_timeout"));
        if started.is_err() {
            let context_id = {
                let state = self.state.lock().unwrap();
                state.sessions.get(&id).and_then(|s| {
                    s.cancel.cancel();
                    s.control.active().map(|c| c.context_id.clone())
                })
            };
            if let Some(context_id) = context_id {
                self.release_agent_terminal(&context_id, true).await?;
            }
        }
        started
    }

    pub fn agent_control_live(&self, context_id: &str) -> bool {
        self.state.lock().unwrap().sessions.values().any(|s| {
            s.view.phase == "running"
                && s.control
                    .active()
                    .is_some_and(|c| c.context_id == context_id && s.agent_input_allowed(c))
        })
    }

    pub async fn agent_terminal(
        &self,
        identity: WorkspaceAgentIdentity,
        command: WorkspaceTerminalAction,
    ) -> AgentResult<WorkspaceTerminalReply> {
        if matches!(command, WorkspaceTerminalAction::ListTabs {}) {
            let state = self.state.lock().unwrap();
            let mut tabs: Vec<_> = state
                .sessions
                .values()
                .filter(|s| {
                    s.view.phase == "running"
                        && s.view.connection.id == identity.connection_id
                        && s.view.connection.revision == identity.connection_revision
                })
                .map(tab)
                .collect();
            tabs.sort_by(|a, b| a.id.cmp(&b.id));
            return Ok(WorkspaceTerminalReply::Tabs { tabs });
        }
        let id = match &command {
            WorkspaceTerminalAction::Takeover { tab_id, .. }
            | WorkspaceTerminalAction::ReadTab { tab_id, .. }
            | WorkspaceTerminalAction::WriteTab { tab_id, .. } => tab_id.clone(),
            _ => unreachable!(),
        };
        let gate = {
            let state = self.state.lock().unwrap();
            let session = state.sessions.get(&id).ok_or("terminal_unavailable")?;
            if session.view.connection.id != identity.connection_id
                || session.view.connection.revision != identity.connection_revision
                || session.view.phase != "running"
            {
                return Err("terminal_unavailable");
            }
            session.write_gate.clone()
        };
        match command {
            WorkspaceTerminalAction::Takeover { request_key, .. } => {
                let _gate = gate.lock().await;
                let (owner, view, result) = {
                    let mut state = self.state.lock().unwrap();
                    if state.sessions.values().any(|s| {
                        s.view.id != id
                            && s.control
                                .active()
                                .is_some_and(|c| c.context_id == identity.context_id)
                    }) {
                        return Err("terminal_already_attached");
                    }
                    // 已启动的文件写入或外部编辑持有独立租约，不能在其运行期间改变操作归属。
                    let group = state
                        .sessions
                        .get(&id)
                        .ok_or("terminal_unavailable")?
                        .view
                        .group_id
                        .clone();
                    if state.file_edits.values().any(|e| e.group.tools.id == group)
                        || state
                            .file_jobs
                            .values()
                            .any(|j| j.group.tools.id == group && j.phase == "working")
                        || state.transfers.values().any(|t| {
                            t.group.tools.id == group && tools::transfer_active(&t.view.phase)
                        })
                    {
                        return Err("terminal_control_busy");
                    }
                    let session = state.sessions.get_mut(&id).ok_or("terminal_unavailable")?;
                    if session.view.phase != "running" {
                        return Err("terminal_unavailable");
                    }
                    let initial = session.control.active().is_none();
                    let control = session
                        .control
                        .acquire(&identity.entry_id, &identity.context_id, &request_key)
                        .map_err(control_error)?;
                    if initial {
                        session.control_deadline = identity
                            .terminal_seconds
                            .map(|s| std::time::Instant::now() + Duration::from_secs(u64::from(s)));
                        session.control_idle_seconds = identity.idle_seconds;
                        session.control_activity();
                    }
                    session.view.agent_control = Some(WorkspaceAgentControl {
                        entry_id: identity.entry_id,
                        entry_name: identity.entry_name,
                        context_id: identity.context_id,
                        lease_id: control.lease_id,
                        releasing: false,
                    });
                    (session.owner.clone(), session.view.clone(), tab(session))
                };
                self.sink
                    .event(owner, WorkspaceNotice::Session(Box::new(view)))
                    .await
                    .map_err(|_| "workspace_unavailable")?;
                Ok(WorkspaceTerminalReply::Control { tab: result })
            }
            WorkspaceTerminalAction::ReadTab {
                lease_id, after, ..
            } => {
                let state = self.state.lock().unwrap();
                let session = state.sessions.get(&id).ok_or("terminal_unavailable")?;
                owned(session, &identity, &lease_id)?;
                if after > session.output_cursor {
                    return Err("invalid_cursor");
                }
                let incomplete = session
                    .output_history
                    .front()
                    .is_some_and(|(cursor, _)| after.saturating_add(1) < *cursor);
                // JSON 转义最多放大六倍；分页须始终小于 1 MiB 的 IPC 帧上限。
                let mut bytes = Vec::new();
                let mut cursor = after;
                for (sequence, chunk) in session
                    .output_history
                    .iter()
                    .filter(|(sequence, _)| *sequence > after)
                {
                    if bytes.len() + chunk.len() > 128 * 1024 {
                        break;
                    }
                    bytes.extend_from_slice(chunk);
                    cursor = *sequence;
                }
                Ok(WorkspaceTerminalReply::Output {
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                    cursor,
                    incomplete,
                })
            }
            WorkspaceTerminalAction::WriteTab {
                lease_id,
                request_key,
                text,
                ..
            } => {
                if text.is_empty()
                    || text.len() > 32 * 1024
                    || request_key.is_empty()
                    || request_key.len() > 256
                {
                    return Err("invalid_input");
                }
                let (reply, receive) = oneshot::channel();
                {
                    let mut state = self.state.lock().unwrap();
                    let session = state.sessions.get_mut(&id).ok_or("terminal_unavailable")?;
                    let control = owned(session, &identity, &lease_id)?;
                    let key = format!("{lease_id}:{request_key}");
                    let fingerprint = format!("{:x}", Sha256::digest(text.as_bytes()));
                    if let Some((previous, sent)) = session.agent_inputs.get(&key) {
                        if previous != &fingerprint {
                            return Err("request_conflict");
                        }
                        return match sent {
                            Some(true) => Ok(WorkspaceTerminalReply::Input { request_key }),
                            _ => Err("input_delivery_unknown"),
                        };
                    }
                    if session.agent_inputs.len() >= 4096 {
                        return Err("receipt_capacity");
                    }
                    session
                        .controls
                        .as_ref()
                        .ok_or("terminal_unavailable")?
                        .try_send(SessionCommand::AgentInput {
                            request_key: key.clone(),
                            control,
                            bytes: text.as_bytes().to_vec(),
                            reply,
                        })
                        .map_err(|_| "terminal_control_busy")?;
                    session.agent_inputs.insert(key, (fingerprint, None));
                }
                receive
                    .await
                    .map_err(|_| "input_delivery_unknown")?
                    .map_err(|error| error.code())?;
                Ok(WorkspaceTerminalReply::Input { request_key })
            }
            _ => unreachable!(),
        }
    }

    pub async fn release_agent_terminal(&self, context_id: &str, finish: bool) -> AgentResult<()> {
        let ids: Vec<_> = self
            .state
            .lock()
            .unwrap()
            .sessions
            .values()
            .filter(|s| {
                s.control
                    .active()
                    .is_some_and(|c| c.context_id == context_id)
            })
            .map(|s| (s.view.id.clone(), s.write_gate.clone()))
            .collect();
        for (id, gate) in ids {
            let _gate = gate.lock().await;
            let notice = {
                let mut state = self.state.lock().unwrap();
                let Some(session) = state.sessions.get_mut(&id) else {
                    continue;
                };
                let Some(control) = session
                    .control
                    .active()
                    .filter(|c| c.context_id == context_id)
                    .cloned()
                else {
                    continue;
                };
                session
                    .control
                    .begin_release(&control.lease_id)
                    .map_err(control_error)?;
                if finish {
                    session
                        .control
                        .finish_release(&control.lease_id)
                        .map_err(control_error)?;
                    session.view.agent_control = None;
                    session.agent_inputs.clear();
                } else if let Some(view) = session.view.agent_control.as_mut() {
                    view.releasing = true;
                }
                (session.owner.clone(), session.view.clone())
            };
            self.sink
                .event(notice.0, WorkspaceNotice::Session(Box::new(notice.1)))
                .await
                .map_err(|_| "workspace_unavailable")?;
        }
        Ok(())
    }
}
