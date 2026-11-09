//! 输入回执在写入 SSH 前登记；重复键不会再次发送，发送成功不代表远端程序已应用。
use super::*;
use sha2::{Digest, Sha256};
impl AgentService {
    pub(super) async fn terminal_input(
        &self,
        entry: &str,
        id: &str,
        key: String,
        control: TerminalControl,
    ) -> AgentResult<AgentReply> {
        contexts::validate_key(&key)?;
        match &control {
            TerminalControl::Write { text } if text.len() > 65536 => return Err("input_too_large"),
            TerminalControl::Resize { columns, rows }
                if !(1..=500).contains(columns) || !(1..=300).contains(rows) =>
            {
                return Err("invalid_terminal_size");
            }
            _ => {}
        }
        let task = self.owned_task(entry, id).await?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(id, &control)).map_err(|_| "invalid_request")?)
        );
        let gate = self
            .state
            .lock()
            .unwrap()
            .input_gates
            .entry(id.into())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _gate = gate.lock().await;
        let reserved = self
            .database
            .execute(DbCommand::Execution(
                ExecutionDbCommand::ReserveTerminalInput {
                    entry: entry.into(),
                    key: key.clone(),
                    task_id: id.into(),
                    fingerprint,
                    created_at: now(),
                },
            ))
            .await;
        match reserved {
            Ok(DbReply::Execution(ExecutionDbReply::InputReserved { inserted: false })) => {
                let cache = self.state.lock().unwrap();
                let state = cache
                    .input_cache
                    .iter()
                    .rev()
                    .find(|(e, k, _)| e == entry && k == &key)
                    .map_or("registered", |(_, _, s)| s.as_str())
                    .to_owned();
                return Ok(AgentReply::Input {
                    request_key: key,
                    state,
                });
            }
            Ok(DbReply::Execution(ExecutionDbReply::InputReserved { inserted: true })) => {}
            Err(WorkspaceError::Conflict) => return Err("request_conflict"),
            Err(WorkspaceError::NotFound) => return Err("result_expired_no_replay"),
            Err(WorkspaceError::InvalidInput) => return Err("terminal_not_running"),
            _ => {
                self.recording_failed().await;
                return Err("agent_storage_unavailable");
            }
        }
        let context = self.context(entry, &task.context_id)?;
        let policy = self.policy(&task.connection_id);
        let generation = context.entry_generation;
        let identity = ExecutionIdentity {
            task_id: id.into(),
            entry_id: entry.into(),
            entry_generation: generation,
            connection_id: task.connection_id.clone(),
            connection_revision: task.connection_revision,
            session_id: task.context_id.clone(),
            policy_revision: policy.revision,
        };
        let sent = self
            .executor
            .terminal_control(identity, control.clone())
            .await;
        let state = match sent {
            Ok(local_execution::TerminalDelivery::Sent) => "sent",
            Ok(local_execution::TerminalDelivery::NotSent(_)) | Err(_) => "notSent",
            Ok(local_execution::TerminalDelivery::Unknown) => "unknown",
        };
        match control {
            TerminalControl::Write { text } => {
                self.record(
                    &task.context_id,
                    Some(id.into()),
                    ActivityActor::Agent,
                    policy.revision,
                    ActivityBody::Input {
                        request_key: key.clone(),
                        text,
                        delivered: state == "sent",
                    },
                )
                .await?;
            }
            TerminalControl::Resize { columns, rows } if state == "sent" => {
                self.record(
                    &task.context_id,
                    Some(id.into()),
                    ActivityActor::Agent,
                    policy.revision,
                    ActivityBody::Resize { columns, rows },
                )
                .await?;
            }
            _ => {}
        }
        {
            let mut state_cache = self.state.lock().unwrap();
            state_cache
                .input_cache
                .push_back((entry.into(), key.clone(), state.into()));
            while state_cache.input_cache.len() > 256 {
                state_cache.input_cache.pop_front();
            }
        }
        Ok(AgentReply::Input {
            request_key: key,
            state: state.into(),
        })
    }
}
