//! 新任务复用既有任务/回执事务；历史回执永不转换为新的执行许可。
use crate::workspace::db;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use vigilshell_application_contracts::ports::{ExecutionDbCommand as C, ExecutionDbReply as R};
use vigilshell_domain::{agent::*, workspace::WorkspaceError};
type Result<T> = std::result::Result<T, WorkspaceError>;
fn encode(task: &ExecutionTask) -> Result<String> {
    task.validate()?;
    let mut body = serde_json::to_value(task).map_err(|_| WorkspaceError::InvalidInput)?;
    body["phase"] =
        serde_json::to_value(task.view_phase()).map_err(|_| WorkspaceError::InvalidInput)?;
    let text = serde_json::to_string(&body).map_err(|_| WorkspaceError::InvalidInput)?;
    if text.len() > 2 * 1024 * 1024 {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(text)
}
fn get(c: &Connection, entry: &str, id: &str) -> Result<Option<Box<ExecutionTask>>> {
    let row: Option<(String,String,i64,bool)> = db(c.query_row(
        "SELECT body,connection_id,created_at,active FROM agent_tasks WHERE id=?1 AND entry_id=?2",
        params![id,entry],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional())?;
    row.map(|(body, connection, created, active)| {
        let task: ExecutionTask =
            serde_json::from_str(&body).map_err(|_| WorkspaceError::Conflict)?;
        task.validate()?;
        if task.id != id
            || task.entry_id != entry
            || task.connection_id != connection
            || i64::try_from(task.created_at).ok() != Some(created)
            || task.state.active() != active
        {
            return Err(WorkspaceError::StorageUnavailable);
        }
        Ok(Box::new(task))
    })
    .transpose()
}
pub(crate) fn execute(c: &mut Connection, command: C) -> Result<R> {
    match command {
        C::FindRequest(request) => {
            request
                .validate()
                .map_err(|_| WorkspaceError::InvalidInput)?;
            let receipt:Option<(String,String)>=db(c.query_row("SELECT task_id,fingerprint FROM agent_receipts WHERE entry_id=?1 AND request_key=?2",params![request.identity.entry_id,request.request_key],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
            let Some((id, previous)) = receipt else {
                return Ok(R::Task(None));
            };
            if request_fingerprint(&request)? != previous {
                return Err(WorkspaceError::Conflict);
            }
            Ok(R::Task(Some(
                get(c, &request.identity.entry_id, &id)?.ok_or(WorkspaceError::NotFound)?,
            )))
        }
        C::GetAny { id } => {
            let entry: Option<String> = db(c
                .query_row("SELECT entry_id FROM agent_tasks WHERE id=?1", [&id], |r| {
                    r.get(0)
                })
                .optional())?;
            Ok(R::Task(if let Some(entry) = entry {
                get(c, &entry, &id)?
            } else {
                None
            }))
        }
        C::Get { entry, id } => Ok(R::Task(get(c, &entry, &id)?)),
        C::Register(task) => {
            task.validate()?;
            if task.revision != 0
                || (task.state.phase == ExecutionPhase::Settled && !task.state.initial_denial())
                || !matches!(
                    task.state.phase,
                    ExecutionPhase::Queued
                        | ExecutionPhase::WaitingReview
                        | ExecutionPhase::Settled
                )
            {
                return Err(WorkspaceError::InvalidInput);
            }
            let fingerprint =
                request_fingerprint(task.source_request.as_ref().unwrap_or(&task.request))?;
            let tx = db(c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))?;
            let receipt:Option<(String,String)>=db(tx.query_row("SELECT task_id,fingerprint FROM agent_receipts WHERE entry_id=?1 AND request_key=?2",params![task.entry_id,task.request.request_key],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
            if let Some((id, previous)) = receipt {
                if previous != fingerprint {
                    return Err(WorkspaceError::Conflict);
                }
                let existing = get(&tx, &task.entry_id, &id)?.ok_or(WorkspaceError::NotFound)?;
                return Ok(R::Registered {
                    task: existing,
                    inserted: false,
                });
            }
            db(tx.execute(
                "INSERT INTO agent_tasks VALUES(?1,?2,?3,?4,?8,?5,?6,?7)",
                params![
                    task.id,
                    task.entry_id,
                    task.connection_id,
                    task.created_at as i64,
                    task.request.request_key,
                    fingerprint,
                    encode(&task)?,
                    task.state.active(),
                ],
            ))?;
            db(tx.execute(
                "INSERT INTO agent_receipts VALUES(?1,?2,?3,?4,?5)",
                params![
                    task.entry_id,
                    task.request.request_key,
                    task.id,
                    fingerprint,
                    task.created_at as i64
                ],
            ))?;
            db(tx.commit())?;
            Ok(R::Registered {
                task,
                inserted: true,
            })
        }
        C::Save { task, expected } => {
            let tx = db(c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))?;
            let old = get(&tx, &task.entry_id, &task.id)?.ok_or(WorkspaceError::NotFound)?;
            if old.revision != expected
                || task.revision != expected + 1
                || !old.state.active()
                || serde_json::to_string(&old.request).ok()
                    != serde_json::to_string(&task.request).ok()
                || serde_json::to_string(&old.source_request).ok()
                    != serde_json::to_string(&task.source_request).ok()
                || old.policy != task.policy
                || old.created_at != task.created_at
                || serde_json::to_string(&old.script_snapshot).ok()
                    != serde_json::to_string(&task.script_snapshot).ok()
                || old.script_staging_path != task.script_staging_path
            {
                return Err(WorkspaceError::Conflict);
            }
            let transition = matches!(
                (old.state.phase, task.state.phase),
                (
                    ExecutionPhase::WaitingReview,
                    ExecutionPhase::Queued | ExecutionPhase::Settled
                ) | (
                    ExecutionPhase::Queued,
                    ExecutionPhase::Dispatching | ExecutionPhase::Settled
                ) | (
                    ExecutionPhase::Dispatching,
                    ExecutionPhase::Running | ExecutionPhase::Settled
                ) | (
                    ExecutionPhase::Running,
                    ExecutionPhase::Stopping | ExecutionPhase::Settled
                ) | (ExecutionPhase::Stopping, ExecutionPhase::Settled)
            );
            if !transition
                || matches!(old.state.dispatch, DispatchState::Attempted { .. })
                    && old.state.dispatch != task.state.dispatch
            {
                return Err(WorkspaceError::Conflict);
            }
            db(tx.execute(
                "UPDATE agent_tasks SET active=?1,body=?2 WHERE id=?3",
                params![task.state.active(), encode(&task)?, task.id],
            ))?;
            db(tx.commit())?;
            Ok(R::Done)
        }
        C::ReserveTerminalInput {
            entry,
            key,
            task_id,
            fingerprint,
            created_at,
        } => {
            if key.is_empty()
                || key.len() > 128
                || key.chars().any(char::is_control)
                || fingerprint.len() != 64
                || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
                || created_at > vigilshell_domain::MAX_REVISION
            {
                return Err(WorkspaceError::InvalidInput);
            }
            let tx = db(c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))?;
            let old:Option<(String,String)>=db(tx.query_row("SELECT task_id,fingerprint FROM agent_receipts WHERE entry_id=?1 AND request_key=?2",params![entry,key],|row|Ok((row.get(0)?,row.get(1)?))).optional())?;
            if let Some((parent, hash)) = old {
                if parent != task_id || hash != fingerprint {
                    return Err(WorkspaceError::Conflict);
                }
                if get(&tx, &entry, &parent)?.is_none() {
                    return Err(WorkspaceError::NotFound);
                }
                return Ok(R::InputReserved { inserted: false });
            }
            let task = get(&tx, &entry, &task_id)?.ok_or(WorkspaceError::NotFound)?;
            if !task.state.terminal || task.state.phase != ExecutionPhase::Running {
                return Err(WorkspaceError::InvalidInput);
            }
            db(tx.execute(
                "INSERT INTO agent_receipts VALUES(?1,?2,?3,?4,?5)",
                params![entry, key, task_id, fingerprint, created_at as i64],
            ))?;
            db(tx.commit())?;
            Ok(R::InputReserved { inserted: true })
        }
        C::Recover => {
            let tx = db(c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))?;
            // 精确选择新格式，不解码/重写旧历史，不从重启入口重放命令。
            let rows=db(tx.prepare("SELECT body FROM agent_tasks WHERE active=1 AND json_type(body,'$.request.identity')='object' AND json_extract(body,'$.policy.execution.version')=2"))?
                .query_map([],|r|r.get::<_,String>(0)).map_err(|_|WorkspaceError::StorageUnavailable)?
                .collect::<rusqlite::Result<Vec<_>>>().map_err(|_|WorkspaceError::StorageUnavailable)?;
            for body in rows {
                let mut task: ExecutionTask =
                    serde_json::from_str(&body).map_err(|_| WorkspaceError::StorageUnavailable)?;
                task.state = task
                    .state
                    .recover_after_restart()
                    .map_err(|_| WorkspaceError::StorageUnavailable)?;
                if matches!(task.state.outcome, Some(ExecutionOutcome::Unknown { .. }))
                    && let Some(report) = task.file_report.as_mut()
                {
                    report.commit = FileCommitState::Unknown;
                    report.error = Some("application_restarted".into());
                }
                if matches!(task.state.outcome, Some(ExecutionOutcome::Unknown { .. }))
                    && let Some(report) = task.native_report.as_mut()
                {
                    report.effects = FileCommitState::Unknown;
                    report.outcome = task.state.outcome.clone();
                    report.error = Some("application_restarted".into());
                }
                task.revision += 1;
                db(tx.execute(
                    "UPDATE agent_tasks SET active=0,body=?1 WHERE id=?2",
                    params![encode(&task)?, task.id],
                ))?;
            }
            db(tx.commit())?;
            Ok(R::Done)
        }
    }
}

#[cfg(test)]
#[path = "execution_tasks_tests.rs"]
mod tests;

fn request_fingerprint(request: &ExecutionRequest) -> Result<String> {
    let mut request = request.clone();
    request.identity.task_id.clear();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&request).map_err(|_| WorkspaceError::InvalidInput)?)
    ))
}
