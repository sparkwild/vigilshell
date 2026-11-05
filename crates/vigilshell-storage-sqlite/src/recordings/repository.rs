use super::*;
use crate::workspace::db;
use rusqlite::{Connection, params};
use vigilshell_application_contracts::ports::{AgentDbReply as R, RecordingCommand as C};
use vigilshell_domain::{agent::*, workspace::WorkspaceError};
type DbResult<T> = std::result::Result<T, WorkspaceError>;

fn failure(_: RecordingError) -> WorkspaceError {
    WorkspaceError::StorageUnavailable
}
fn encode<T: serde::Serialize>(value: &T) -> DbResult<String> {
    serde_json::to_string(value).map_err(|_| WorkspaceError::InvalidInput)
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> DbResult<T> {
    serde_json::from_str(value).map_err(|_| WorkspaceError::StorageUnavailable)
}
fn session(c: &Connection, id: &str) -> DbResult<RecordedSession> {
    let (body, bytes, sequence, incomplete): (String, u64, u64, bool) = db(c.query_row(
        "SELECT body,bytes,sequence,incomplete FROM agent_recordings WHERE id=?1 AND deleting=0",
        [id],
        |r| {
            Ok((
                r.get(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
                r.get(3)?,
            ))
        },
    ))?;
    let mut value: RecordedSession = decode(&body)?;
    value.bytes = bytes;
    value.last_sequence = sequence;
    value.incomplete = incomplete;
    Ok(value)
}
fn save(c: &Connection, value: &RecordedSession) -> DbResult<()> {
    db(c.execute("UPDATE agent_recordings SET ended_at=?2,result=?3,incomplete=?4,sequence=?5,bytes=?6,body=?7 WHERE id=?1",
        params![value.id,value.ended_at.map(|n| n as i64),value.result,value.incomplete,value.last_sequence as i64,value.bytes as i64,encode(value)?]))?;
    Ok(())
}
fn incomplete(c: &Connection, id: &str) {
    let _ = c.execute("UPDATE agent_recordings SET incomplete=1 WHERE id=?1", [id]);
}
fn total(c: &Connection) -> DbResult<u64> {
    db(c.query_row(
        "SELECT COALESCE(SUM(bytes),0) FROM agent_recordings",
        [],
        |r| r.get::<_, i64>(0).map(|v| v as u64),
    ))
}
fn remove(c: &Connection, store: &mut RecordingStore, id: &str) -> DbResult<()> {
    let changed = db(c.execute(
        "UPDATE agent_recordings SET deleting=1 WHERE id=?1 AND ended_at IS NOT NULL",
        [id],
    ))?;
    if changed == 0 {
        return Err(WorkspaceError::InvalidInput);
    }
    store.remove_closed(id).map_err(failure)?;
    db(c.execute(
        "DELETE FROM agent_tasks WHERE active=0 AND json_extract(body,'$.contextId')=?1",
        [id],
    ))?;
    db(c.execute(
        "DELETE FROM agent_recordings WHERE id=?1 AND deleting=1",
        [id],
    ))?;
    Ok(())
}
fn prune(c: &Connection, store: &mut RecordingStore, before: u64, limit: u64) -> DbResult<()> {
    let ids: Vec<String> = db(c.prepare(
        "SELECT id FROM agent_recordings WHERE ended_at IS NOT NULL ORDER BY ended_at,id",
    ))?
    .query_map([], |r| r.get(0))
    .and_then(|rows| rows.collect())
    .map_err(|_| WorkspaceError::StorageUnavailable)?;
    for id in ids {
        let (ended, deleting): (u64, bool) = db(c.query_row(
            "SELECT ended_at,deleting FROM agent_recordings WHERE id=?1",
            [&id],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get(1)?)),
        ))?;
        if deleting || ended < before || total(c)? > limit {
            remove(c, store, &id)?;
        }
    }
    if total(c)? > limit {
        return Err(WorkspaceError::Busy);
    }
    Ok(())
}

pub(crate) fn execute(c: &mut Connection, store: &mut RecordingStore, command: C) -> DbResult<R> {
    let optional = matches!(&command, C::AppendScreen { .. });
    match command {
        C::Probe => {
            store.probe().map_err(failure)?;
            Ok(R::Done)
        }
        C::Create { session: value } => {
            if value.ended_at.is_some()
                || value.result.is_some()
                || value.last_sequence != 0
                || value.bytes != 0
                || value.incomplete
            {
                return Err(WorkspaceError::InvalidInput);
            }
            let tx = db(c.transaction())?;
            db(tx.execute("INSERT INTO agent_recordings(id,connection_id,entry_id,created_at,ended_at,result,incomplete,sequence,bytes,deleting,body) VALUES(?1,?2,?3,?4,NULL,NULL,0,0,0,0,?5)",
                params![value.id,value.connection_id,value.entry_id,value.created_at as i64,encode(&value)?]))?;
            store.create(&value.id).map_err(failure)?;
            if let Err(error) = tx.commit() {
                store.close(&value.id);
                // 未发布的空会话没有业务记录，失败后回收，不覆盖已有会话。
                let _ = store.remove_closed(&value.id);
                return Err(db::<()>(Err(error)).unwrap_err());
            }
            Ok(R::RecordedSession(value))
        }
        C::Append {
            mut record,
            limit_bytes,
        }
        | C::AppendScreen {
            mut record,
            limit_bytes,
        } => {
            if matches!(record.body, ActivityBody::Historical { .. }) {
                return Err(WorkspaceError::InvalidInput);
            }
            let critical = matches!(
                &record.body,
                ActivityBody::Control { .. } | ActivityBody::Gap { .. }
            ) || matches!(&record.body,ActivityBody::Operation {phase,..} if !phase.active())
                || matches!(&record.body,ActivityBody::Session {phase,..} if matches!(phase,ContextPhase::Closing|ContextPhase::Closed|ContextPhase::Disconnected|ContextPhase::Failed));
            // 正常输出不能耗尽用于停止和结算的空间；已运行的输出仍不被截断或淘汰。
            let limit_bytes = if critical {
                limit_bytes
            } else {
                limit_bytes.saturating_sub((limit_bytes / 64).min(4 * 1024 * 1024))
            };
            let mut value = session(c, &record.session_id)?;
            if optional && !matches!(record.body, ActivityBody::Screen { .. }) {
                return Err(WorkspaceError::InvalidInput);
            }
            if value.ended_at.is_some() || value.incomplete {
                if optional {
                    return Ok(R::Done);
                }
                return Err(WorkspaceError::InvalidInput);
            }
            record.sequence = value.last_sequence + 1;
            let bytes = RecordingStore::frame_bytes(&record).map_err(failure)?;
            if optional && total(c)?.saturating_add(bytes) > limit_bytes {
                return Ok(R::Done);
            }
            if bytes > limit_bytes || prune(c, store, 0, limit_bytes - bytes).is_err() {
                incomplete(c, &value.id);
                store.close(&value.id);
                return Err(WorkspaceError::Busy);
            }
            let position = match store.append(&mut record, limit_bytes - total(c)?) {
                Ok(position) => position,
                Err(error) => {
                    incomplete(c, &value.id);
                    store.close(&value.id);
                    return Err(failure(error));
                }
            };
            let indexed = (|| -> DbResult<()> {
                let tx = db(c.transaction())?;
                let kind = match &record.body {
                    ActivityBody::Script { .. } => "script",
                    ActivityBody::Policy { .. } => "policy",
                    ActivityBody::Screen { .. } => "screen",
                    ActivityBody::Transfer { .. } => "transfer",
                    ActivityBody::Session { .. } => "session",
                    ActivityBody::Operation { .. } => "operation",
                    ActivityBody::Input { .. } => "input",
                    ActivityBody::Output { .. } => "output",
                    ActivityBody::Resize { .. } => "resize",
                    ActivityBody::FileChanged { .. } => "files",
                    ActivityBody::Decision { .. } => "decision",
                    ActivityBody::Control { .. } => "control",
                    ActivityBody::Gap { .. } => "gap",
                    ActivityBody::Historical { .. } => return Err(WorkspaceError::InvalidInput),
                };
                db(tx.execute("INSERT INTO agent_recording_events(session_id,sequence,task_id,kind,timestamp_ms,position) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![record.session_id,record.sequence as i64,record.task_id,kind,record.timestamp_ms as i64,encode(&position)?]))?;
                value.bytes += u64::from(position.length);
                value.last_sequence = record.sequence;
                save(&tx, &value)?;
                db(tx.commit())
            })();
            if let Err(error) = indexed {
                incomplete(c, &value.id);
                store.close(&value.id);
                return Err(error);
            }
            Ok(R::Recorded(record))
        }
        C::Read {
            include_output,
            session_id,
            task_id,
            cursor,
        } => {
            let value = session(c, &session_id)?;
            let sql = if include_output {
                "SELECT position FROM agent_recording_events WHERE session_id=?1 AND sequence>?2 AND (?3 IS NULL OR task_id=?3) ORDER BY sequence LIMIT 256"
            } else {
                "SELECT position FROM agent_recording_events WHERE session_id=?1 AND sequence>?2 AND (?3 IS NULL OR task_id=?3) AND kind<>'output' AND kind<>'screen' ORDER BY sequence LIMIT 256"
            };
            let mut query = db(c.prepare(sql))?;
            let rows = db(
                query.query_map(params![session_id, cursor as i64, task_id], |r| {
                    r.get::<_, String>(0)
                }),
            )?;
            let mut page = ActivityPage {
                next_cursor: cursor,
                incomplete: value.incomplete,
                ..Default::default()
            };
            let mut bytes = 0;
            for row in rows {
                let position: RecordingPosition = decode(&db(row)?)?;
                if bytes > 0 && bytes + position.length > 256 * 1024 {
                    break;
                }
                match store.read(&position) {
                    Ok(record) => {
                        bytes += position.length;
                        page.next_cursor = record.sequence;
                        page.records.push(record);
                    }
                    Err(_) => {
                        page.incomplete = true;
                        incomplete(c, &session_id);
                        break;
                    }
                }
            }
            Ok(R::Activity(page))
        }
        C::End {
            session_id,
            ended_at,
            result,
            incomplete: gap,
        } => {
            let mut value = session(c, &session_id)?;
            if value.ended_at.is_some() {
                return Ok(R::RecordedSession(value));
            }
            value.ended_at = Some(ended_at);
            value.result = Some(result);
            value.incomplete |= gap;
            save(c, &value)?;
            store.close(&session_id);
            Ok(R::RecordedSession(value))
        }
        C::Recover { now } => {
            recover(c, store, now)?;
            Ok(R::Done)
        }
        C::Prune {
            before,
            limit_bytes,
        } => {
            prune(c, store, before, limit_bytes)?;
            Ok(R::Done)
        }
    }
}

fn recover(c: &Connection, store: &mut RecordingStore, now: u64) -> DbResult<()> {
    let rows: Vec<(String, bool)> = db(c.prepare("SELECT id,deleting FROM agent_recordings"))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .and_then(|rows| rows.collect())
        .map_err(|_| WorkspaceError::StorageUnavailable)?;
    for (id, deleting) in rows {
        if deleting {
            remove(c, store, &id)?;
            continue;
        }
        store.close(&id);
        let mut value = session(c, &id)?;
        let before = (value.incomplete, value.ended_at, value.result.clone());
        match store.recover(&id) {
            Ok(recovered) => {
                let indexed: Vec<String> = db(c.prepare("SELECT position FROM agent_recording_events WHERE session_id=?1 ORDER BY sequence"))?
                    .query_map([&id], |r| r.get(0)).and_then(|rows| rows.collect()).map_err(|_| WorkspaceError::StorageUnavailable)?;
                let exact = indexed.len() == recovered.positions.len()
                    && indexed
                        .iter()
                        .zip(&recovered.positions)
                        .all(|(saved, position)| {
                            encode(position).is_ok_and(|actual| saved == &actual)
                        });
                value.incomplete |=
                    recovered.incomplete || !exact || recovered.bytes != value.bytes;
            }
            Err(_) => value.incomplete = true,
        }
        if value.ended_at.is_none() {
            value.ended_at = Some(now);
            value.result = Some("interruptedUnknown".into());
            value.incomplete = true;
        }
        if before != (value.incomplete, value.ended_at, value.result.clone()) {
            save(c, &value)?;
        }
    }
    Ok(())
}
