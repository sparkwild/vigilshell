use super::*;
pub(super) struct TerminalRequest {
    pub identity: ExecutionIdentity,
    pub control: TerminalControl,
    pub reply: tokio::sync::oneshot::Sender<TerminalDelivery>,
}
#[derive(Clone, Debug)]
pub enum TerminalDelivery {
    Sent,
    NotSent(&'static str),
    Unknown,
}
impl LocalExecutor {
    pub async fn terminal_control(
        &self,
        identity: ExecutionIdentity,
        control: TerminalControl,
    ) -> AgentResult<TerminalDelivery> {
        match &control {
            TerminalControl::Resize { columns, rows }
                if !(1..=500).contains(columns) || !(1..=300).contains(rows) =>
            {
                return Err("invalid_input");
            }
            TerminalControl::Write { text } if text.len() > 65536 => {
                return Err("invalid_input");
            }
            _ => {}
        }
        let task = self.get(&identity.entry_id, &identity.task_id).await?;
        if !task.state.terminal || task.state.phase != ExecutionPhase::Running {
            return Err("terminal_not_running");
        }
        if identity.session_id != task.context_id
            || identity.connection_id != task.connection_id
            || identity.connection_revision != task.request.identity.connection_revision
            || identity.entry_generation != task.request.identity.entry_generation
        {
            return Err("access_revoked");
        }
        let access = self
            .authority
            .current(identity.clone())
            .await
            .map_err(|_| "access_revoked")?;
        access.policy.execution.authorize(&task.request.operation)?;
        let sender = self
            .terminals
            .lock()
            .unwrap()
            .get(&task.id)
            .cloned()
            .ok_or("terminal_not_running")?;
        let (reply, wait) = tokio::sync::oneshot::channel();
        if sender
            .try_send(TerminalRequest {
                identity,
                control,
                reply,
            })
            .is_err()
        {
            return Ok(TerminalDelivery::NotSent("terminal_input_backpressure"));
        }
        Ok(tokio::time::timeout(Duration::from_secs(15), wait)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(TerminalDelivery::Unknown))
    }
    pub async fn cancel_scope(
        &self,
        entry: Option<&str>,
        connection: Option<&str>,
        context: Option<&str>,
    ) {
        for active in self.active.lock().unwrap().values() {
            if entry.is_none_or(|v| active.identity.entry_id == v)
                && connection.is_none_or(|v| active.identity.connection_id == v)
                && context.is_none_or(|v| active.identity.session_id == v)
            {
                active.cancel.cancel();
            }
        }
    }
}
