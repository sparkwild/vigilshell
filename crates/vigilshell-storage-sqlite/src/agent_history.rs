//! 独立的只读历史查询。原正文、回执和帧均不进入当前执行类型或恢复流程。
use crate::{recordings::RecordingStore, workspace::db};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Deserialize;
use std::path::Path;
use vigilshell_application_contracts::ports::{AgentHistoryCommand as C, AgentHistoryReply as R};
use vigilshell_domain::{agent::*, workspace::WorkspaceError};

type Result<T> = std::result::Result<T, WorkspaceError>;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = MAX_BODY_BYTES + 76;
const TASK_COLUMNS: &str =
    "id,entry_id,connection_id,created_at,active,request_key,fingerprint,body";
const RECORDING_COLUMNS: &str =
    "id,connection_id,entry_id,created_at,ended_at,result,incomplete,sequence,bytes,body";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskEnvelope {
    id: String,
    entry_id: String,
    connection_id: String,
    created_at: u64,
    state: Option<serde_json::Value>,
    request: Option<serde_json::Value>,
    #[serde(flatten)]
    display: HistoricalTaskDisplay,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordingEnvelope {
    id: String,
    entry_id: String,
    connection_id: String,
    created_at: u64,
    connection_name: Option<String>,
    entry_name: Option<String>,
}

fn decode<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_BODY_BYTES {
        return Err(WorkspaceError::StorageUnavailable);
    }
    serde_json::from_str(raw).map_err(|_| WorkspaceError::StorageUnavailable)
}

fn unsigned(row: &Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}

fn optional_unsigned(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(column)?
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
        })
        .transpose()
}

fn task_row(row: &Row<'_>) -> rusqlite::Result<HistoricalTask> {
    Ok(HistoricalTask {
        id: row.get(0)?,
        entry_id: row.get(1)?,
        connection_id: row.get(2)?,
        created_at: unsigned(row, 3)?,
        active: row.get(4)?,
        request_key: row.get(5)?,
        fingerprint: row.get(6)?,
        raw_body: row.get(7)?,
        display: HistoricalTaskDisplay::default(),
    })
}

fn project_task(mut task: HistoricalTask) -> Result<HistoricalTask> {
    let body: TaskEnvelope = decode(&task.raw_body)?;
    if body.id != task.id
        || body.entry_id != task.entry_id
        || body.connection_id != task.connection_id
        || body.created_at != task.created_at
    {
        return Err(WorkspaceError::StorageUnavailable);
    }
    task.display = body.display;
    // 只补充新格式的展示字段，不覆盖旧投影，也不解码为可执行请求。
    if let Some(state) = body.state {
        if task.display.phase.is_none() {
            task.display.phase = state
                .get("phase")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
        }
        if task.display.exit_code.is_none() {
            task.display.exit_code = state
                .pointer("/outcome/exitCode")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok());
        }
    }
    if let Some(request) = body.request {
        let kind = request
            .pointer("/operation/kind")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if kind == "command" {
            if task.display.command.is_none() {
                task.display.command = request
                    .pointer("/operation/command")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
            }
            if task.display.cwd.is_none() {
                task.display.cwd = request
                    .pointer("/operation/directory/path")
                    .and_then(|v| v.as_str())
                    .map(|v| format!("/{v}"));
            }
        }
        if task.display.tool.is_none() {
            task.display.tool = match kind {
                "command" => Some("command_run.command"),
                "files_list" => Some("files_read.list"),
                "files_stat" => Some("files_read.stat"),
                "files_read" => Some("files_read.read"),
                "files_write" => Some("files_write.write"),
                "files_edit" => Some("files_write.edit"),
                "files_mkdir" => Some("files_manage.mkdir"),
                "files_copy" => Some("files_manage.copy"),
                "files_move" => Some("files_manage.move"),
                "files_link" => Some("files_manage.link"),
                "files_set_attributes" => Some("files_manage.set_attributes"),
                "files_remove" => Some("files_remove"),
                "transfers_upload" => Some("files_transfer.upload"),
                "transfers_download" => Some("files_transfer.download"),
                _ => None,
            }
            .map(str::to_owned);
        }
    }
    Ok(task)
}

fn task(c: &Connection, id: &str) -> Result<Option<HistoricalTask>> {
    db(c.query_row(
        &format!("SELECT {TASK_COLUMNS} FROM agent_tasks WHERE id=?1"),
        [id],
        task_row,
    )
    .optional())?
    .map(project_task)
    .transpose()
}

fn tasks(c: &Connection, filter: TaskHistoryFilter) -> Result<TaskHistoryPage> {
    let mut query = db(c.prepare(&format!(
        "SELECT {TASK_COLUMNS} FROM agent_tasks WHERE (?1 IS NULL OR entry_id=?1) AND (?2 IS NULL OR connection_id=?2) AND (?3 IS NULL OR json_extract(body,'$.contextId')=?3) AND (?4 IS NULL OR COALESCE(json_extract(body,'$.phase'),json_extract(body,'$.state.phase'))=?4) ORDER BY created_at DESC,id DESC LIMIT 101 OFFSET ?5"
    )))?;
    let rows = db(query.query_map(
        params![
            filter.entry_id,
            filter.connection_id,
            filter.session_id,
            filter.phase,
            filter.offset
        ],
        task_row,
    ))?;
    let mut page = TaskHistoryPage::default();
    let mut bytes = 0;
    for row in rows {
        let task = db(row)?;
        if page.tasks.len() == 100
            || (!page.tasks.is_empty() && bytes + task.raw_body.len() > MAX_PAGE_BYTES)
        {
            page.next_offset = Some(
                filter
                    .offset
                    .checked_add(page.tasks.len() as u32)
                    .ok_or(WorkspaceError::InvalidInput)?,
            );
            break;
        }
        bytes += task.raw_body.len();
        page.tasks.push(project_task(task)?);
    }
    Ok(page)
}

fn receipt(c: &Connection, entry: &str, key: &str) -> Result<Option<HistoricalRequestReceipt>> {
    let receipt = db(c.query_row(
        "SELECT task_id,fingerprint,created_at FROM agent_receipts WHERE entry_id=?1 AND request_key=?2",
        params![entry,key],
        |row| Ok(HistoricalRequestReceipt {
            entry_id: entry.into(), request_key: key.into(), task_id: row.get(0)?,
            fingerprint: row.get(1)?, created_at: unsigned(row, 2)?, task_present: false,
        }),
    ).optional())?;
    let Some(mut receipt) = receipt else {
        return Ok(None);
    };
    let owner: Option<String> = db(c
        .query_row(
            "SELECT entry_id FROM agent_tasks WHERE id=?1",
            [&receipt.task_id],
            |row| row.get(0),
        )
        .optional())?;
    if owner.as_deref().is_some_and(|owner| owner != entry) {
        return Err(WorkspaceError::StorageUnavailable);
    }
    receipt.task_present = owner.is_some();
    Ok(Some(receipt))
}

fn recording_row(row: &Row<'_>) -> rusqlite::Result<HistoricalRecording> {
    Ok(HistoricalRecording {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        entry_id: row.get(2)?,
        created_at: unsigned(row, 3)?,
        ended_at: optional_unsigned(row, 4)?,
        result: row.get(5)?,
        incomplete: row.get(6)?,
        last_sequence: unsigned(row, 7)?,
        bytes: unsigned(row, 8)?,
        raw_body: row.get(9)?,
        connection_name: None,
        entry_name: None,
    })
}

fn project_recording(mut recording: HistoricalRecording) -> Result<HistoricalRecording> {
    let body: RecordingEnvelope = decode(&recording.raw_body)?;
    if body.id != recording.id
        || body.entry_id != recording.entry_id
        || body.connection_id != recording.connection_id
        || body.created_at != recording.created_at
    {
        return Err(WorkspaceError::StorageUnavailable);
    }
    recording.connection_name = body.connection_name;
    recording.entry_name = body.entry_name;
    Ok(recording)
}

fn recording(c: &Connection, id: &str) -> Result<HistoricalRecording> {
    let value = db(c
        .query_row(
            &format!("SELECT {RECORDING_COLUMNS} FROM agent_recordings WHERE id=?1 AND deleting=0"),
            [id],
            recording_row,
        )
        .optional())?
    .ok_or(WorkspaceError::NotFound)?;
    project_recording(value)
}

fn recordings(c: &Connection, filter: RecordingFilter) -> Result<RecordingHistoryPage> {
    let mut query = db(c.prepare(&format!("SELECT {RECORDING_COLUMNS} FROM agent_recordings WHERE deleting=0 AND (?1 IS NULL OR connection_id=?1) AND (?2 IS NULL OR entry_id=?2) AND (?3 IS NULL OR id=?3) AND (?4 IS NULL OR result=?4) AND (?6 IS NULL OR created_at<?6 OR (created_at=?6 AND id<?7)) AND (?8 IS NULL OR (ended_at IS NULL)=?8) AND (?9 IS NULL OR instr(lower(COALESCE(json_extract(body,'$.connectionName'),'')),lower(?9))>0 OR instr(lower(COALESCE(json_extract(body,'$.entryName'),'')),lower(?9))>0) AND (?10 IS NULL OR created_at>=?10) ORDER BY created_at DESC,id DESC LIMIT 101 OFFSET ?5")))?;
    let rows = db(query.query_map(
        params![
            filter.connection_id,
            filter.entry_id,
            filter.session_id,
            filter.result,
            filter.offset,
            filter
                .before_created_at
                .map(i64::try_from)
                .transpose()
                .map_err(|_| WorkspaceError::InvalidInput)?,
            filter.before_id,
            filter.active,
            filter.query,
            filter
                .created_after
                .map(i64::try_from)
                .transpose()
                .map_err(|_| WorkspaceError::InvalidInput)?
        ],
        recording_row,
    ))?;
    let mut page = RecordingHistoryPage::default();
    let mut bytes = 0;
    for row in rows {
        let value = db(row)?;
        if page.sessions.len() == 100
            || (!page.sessions.is_empty() && bytes + value.raw_body.len() > MAX_PAGE_BYTES)
        {
            page.next_offset = Some(
                filter
                    .offset
                    .checked_add(page.sessions.len() as u32)
                    .ok_or(WorkspaceError::InvalidInput)?,
            );
            break;
        }
        bytes += value.raw_body.len();
        page.sessions.push(project_recording(value)?);
    }
    Ok(page)
}

fn activity(
    c: &Connection,
    root: Option<&Path>,
    session_id: &str,
    task_id: Option<&str>,
    cursor: u64,
    include_output: bool,
) -> Result<ActivityHistoryPage> {
    let cursor_sql = i64::try_from(cursor).map_err(|_| WorkspaceError::InvalidInput)?;
    let session = recording(c, session_id)?;
    let mut query = db(c.prepare("SELECT sequence,task_id,kind,timestamp_ms,position FROM agent_recording_events WHERE session_id=?1 AND sequence>?2 AND (?3 IS NULL OR task_id=?3) AND (?4 OR (kind<>'output' AND kind<>'screen')) ORDER BY sequence LIMIT 257"))?;
    let rows = db(query.query_map(
        params![session_id, cursor_sql, task_id, include_output],
        |row| {
            Ok((
                unsigned(row, 0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                unsigned(row, 3)?,
                row.get::<_, String>(4)?,
            ))
        },
    ))?;
    let mut page = ActivityHistoryPage {
        next_cursor: cursor,
        incomplete: session.incomplete,
        ..Default::default()
    };
    let mut store = None;
    let mut bytes = 0;
    for row in rows {
        let (sequence, task, kind, timestamp, raw_position) = db(row)?;
        let position: RecordingPosition = decode(&raw_position)?;
        if position.session_id != session_id || position.sequence != sequence {
            return Err(WorkspaceError::StorageUnavailable);
        }
        if page.records.len() == 256
            || (!page.records.is_empty() && bytes + position.length as usize > MAX_PAGE_BYTES)
        {
            page.has_more = true;
            break;
        }
        if store.is_none() {
            store = Some(
                RecordingStore::open_read_only(root.ok_or(WorkspaceError::StorageUnavailable)?)
                    .map_err(|_| WorkspaceError::StorageUnavailable)?,
            );
        }
        let record = store
            .as_ref()
            .unwrap()
            .read_history(&position)
            .map_err(|_| WorkspaceError::StorageUnavailable)?;
        if record.task_id != task || record.kind != kind || record.timestamp_ms != timestamp {
            return Err(WorkspaceError::StorageUnavailable);
        }
        bytes += position.length as usize;
        page.next_cursor = sequence;
        page.records.push(record);
    }
    Ok(page)
}

fn summaries(
    c: &Connection,
    root: Option<&Path>,
    filter: ActivitySummaryFilter,
) -> Result<Vec<ActivitySummary>> {
    let before = filter
        .before_time
        .map(i64::try_from)
        .transpose()
        .map_err(|_| WorkspaceError::InvalidInput)?;
    // Task transitions collapse into one row; independent events retain their recorded identity.
    let mut statement = db(c.prepare("SELECT identity,stamp,session,task,sequence FROM (
        SELECT 't:'||id AS identity,created_at AS stamp,json_extract(body,'$.contextId') AS session,id AS task,NULL AS sequence,active,body || COALESCE((SELECT r.body FROM agent_recordings r WHERE r.id=json_extract(agent_tasks.body,'$.contextId')), '') AS searchable FROM agent_tasks
        UNION ALL
        SELECT 'e:'||e.session_id||':'||e.sequence,e.timestamp_ms,e.session_id,NULL,e.sequence,0,r.body FROM agent_recording_events e JOIN agent_recordings r ON r.id=e.session_id AND r.deleting=0 WHERE e.kind IN ('session','policy','decision','control','fileChanged','gap','historical') AND (e.task_id IS NULL OR e.kind<>'fileChanged')
        ) WHERE (?1 IS NULL OR stamp<?1 OR (stamp=?1 AND identity<?2)) AND (?3 IS NULL OR instr(lower(searchable),lower(?3))>0) AND (?4 IS NULL OR active=?4) AND (?5 IS NULL OR session=?5) ORDER BY stamp DESC,identity DESC LIMIT 101"))?;
    let rows = db(statement.query_map(
        params![
            before,
            filter.before,
            filter.query,
            filter.active,
            filter.session_id
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                unsigned(r, 1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                optional_unsigned(r, 4)?,
            ))
        },
    ))?;
    let mut result = Vec::new();
    for row in rows {
        let (id, created_at, session, task_id, sequence) = db(row)?;
        let session_id = session.unwrap_or_default();
        let recording = match recording(c, &session_id) {
            Ok(r) => Some(r),
            Err(WorkspaceError::NotFound) => None,
            Err(e) => return Err(e),
        };
        let mut item = ActivitySummary {
            entry_id: recording
                .as_ref()
                .map(|r| r.entry_id.clone())
                .unwrap_or_default(),
            id,
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            sequence,
            created_at,
            connection_name: recording
                .as_ref()
                .and_then(|r| r.connection_name.clone())
                .unwrap_or_default(),
            entry_name: recording
                .as_ref()
                .and_then(|r| r.entry_name.clone())
                .unwrap_or_default(),
            kind: String::new(),
            summary: String::new(),
            phase: None,
            started_at: None,
            finished_at: None,
            uncertain: false,
        };
        if let Some(id) = task_id {
            let history = task(c, &id)?.ok_or(WorkspaceError::NotFound)?;
            item.entry_id = history.entry_id.clone();
            item.connection_name = history
                .display
                .connection_name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or(item.connection_name);
            item.entry_name = history
                .display
                .entry_name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or(item.entry_name);
            if let Ok(task) = serde_json::from_str::<ExecutionTask>(&history.raw_body) {
                item.kind = task.request.operation.label().into();
                item.summary = match &task.request.operation {
                    RuntimeOperation::Command { command, .. } => command.clone(),
                    RuntimeOperation::Script {
                        source: ScriptSource::RemoteFile { target, .. },
                        ..
                    } => target.path.clone(),
                    op => op
                        .business_targets()
                        .unwrap_or_default()
                        .iter()
                        .map(|t| t.path.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                item.phase = Some(task.view_phase());
                item.started_at = match task.state.dispatch {
                    DispatchState::Attempted { attempted_at, .. } => Some(attempted_at),
                    _ => None,
                };
                item.finished_at = task.finished_at;
                item.uncertain =
                    matches!(task.state.outcome, Some(ExecutionOutcome::Unknown { .. }));
            } else if let Ok(task) = serde_json::from_str::<AgentTask>(&history.raw_body) {
                item.kind = task.tool;
                item.summary = task.command;
                item.phase = Some(task.phase);
                item.finished_at = task.finished_at;
                item.started_at = task
                    .execution_state
                    .as_ref()
                    .and_then(|s| match s.dispatch {
                        DispatchState::Attempted { attempted_at, .. } => Some(attempted_at),
                        _ => None,
                    });
                item.uncertain = !task.termination_confirmed;
            } else {
                item.kind = history.display.tool.unwrap_or_else(|| "historical".into());
                item.summary = history.display.command.unwrap_or_default();
                item.uncertain = true;
            }
        } else if let Some(sequence) = sequence {
            let position: String = db(c.query_row(
                "SELECT position FROM agent_recording_events WHERE session_id=?1 AND sequence=?2",
                params![
                    session_id,
                    i64::try_from(sequence).map_err(|_| WorkspaceError::InvalidInput)?
                ],
                |r| r.get(0),
            ))?;
            let store =
                RecordingStore::open_read_only(root.ok_or(WorkspaceError::StorageUnavailable)?)
                    .map_err(|_| WorkspaceError::StorageUnavailable)?;
            let record = store
                .read_history(&decode(&position)?)
                .map_err(|_| WorkspaceError::StorageUnavailable)?;
            item.kind = record.kind;
            if let Ok(record) = serde_json::from_str::<ActivityRecord>(&record.raw_json) {
                item.summary = match record.body {
                    ActivityBody::Session { phase, .. } => format!("session:{phase:?}"),
                    ActivityBody::Control { action } => action,
                    ActivityBody::Decision { allowed, .. } => {
                        if allowed { "allowed" } else { "denied" }.into()
                    }
                    ActivityBody::FileChanged { paths } => paths.join(", "),
                    _ => String::new(),
                };
            }
        }
        // A restarted backend cannot attest that an old dispatched task is still executing.
        if item.phase.as_ref().is_some_and(TaskPhase::active)
            && recording.as_ref().is_none_or(|r| r.ended_at.is_some())
        {
            item.uncertain = true;
        }
        result.push(item);
    }
    Ok(result)
}

pub(crate) fn execute(c: &Connection, root: Option<&Path>, command: C) -> Result<R> {
    match command {
        C::ActivitySources => {
            let mut query=db(c.prepare("SELECT json_extract(e.value,'$.entry.id'),json_extract(e.value,'$.entry.client') FROM agent_configuration a,json_each(a.body,'$.entries') e WHERE a.id=1"))?;
            let rows = db(query.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            }))?;
            let mut sources = Vec::new();
            for row in rows {
                let (id, client) = db(row)?;
                sources.push(ActivitySource {
                    id,
                    client: client
                        .and_then(|c| serde_json::from_value(serde_json::Value::String(c)).ok()),
                });
            }
            Ok(R::ActivitySources(sources))
        }

        C::Screen {
            session_id,
            task_id,
        } => {
            let raw:Option<String>=db(c.query_row("SELECT position FROM agent_recording_events WHERE session_id=?1 AND task_id=?2 AND kind='screen' ORDER BY sequence DESC LIMIT 1",params![session_id,task_id],|r|r.get(0)).optional())?;
            let Some(raw) = raw else {
                return Ok(R::Screen(None));
            };
            let position: RecordingPosition = decode(&raw)?;
            if position.session_id != session_id {
                return Err(WorkspaceError::StorageUnavailable);
            }
            let store =
                RecordingStore::open_read_only(root.ok_or(WorkspaceError::StorageUnavailable)?)
                    .map_err(|_| WorkspaceError::StorageUnavailable)?;
            let record = store
                .read_history(&position)
                .map_err(|_| WorkspaceError::StorageUnavailable)?;
            if record.task_id.as_deref() != Some(&task_id) {
                return Err(WorkspaceError::StorageUnavailable);
            }
            match decode::<ActivityRecord>(&record.raw_json)?.body {
                ActivityBody::Screen { checkpoint } => Ok(R::Screen(Some(checkpoint))),
                _ => Err(WorkspaceError::StorageUnavailable),
            }
        }

        C::ActivitySummaries { filter } => summaries(c, root, filter).map(R::ActivitySummaries),
        C::Task { id } => task(c, &id).map(|value| R::Task(value.map(Box::new))),
        C::Tasks { filter } => tasks(c, filter).map(R::Tasks),
        C::RequestReceipt { entry, key } => receipt(c, &entry, &key).map(R::RequestReceipt),
        C::Recordings { filter } => recordings(c, filter).map(R::Recordings),
        C::Activity {
            session_id,
            task_id,
            cursor,
            include_output,
        } => activity(
            c,
            root,
            &session_id,
            task_id.as_deref(),
            cursor,
            include_output,
        )
        .map(R::Activity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(include_str!("migrations/001_initial.sql"))
            .unwrap();
        connection
    }

    fn insert(c: &Connection, id: &str, extra: &str) -> String {
        let body = format!(
            r#"{{ "id": "{id}", "entryId": "entry", "connectionId": "connection", "createdAt": 42, "contextId": "session", "phase": "interruptedUnknown", "command": "old request", "outputStart": 7, "outputEnd": 19, {extra} }}"#
        );
        c.execute(
            "INSERT INTO agent_tasks VALUES(?1,'entry','connection',42,1,NULL,NULL,?2)",
            params![id, body],
        )
        .unwrap();
        body
    }

    #[test]
    fn activity_sources_expose_only_identity_and_client_without_configuration_initialization() {
        let c = database();
        c.execute("UPDATE agent_configuration SET body=?1",[r#"{"entries":[{"entry":{"id":"one","client":"codex"},"sealed":"never-return","digest":"private"}]}"#]).unwrap();
        c.pragma_update(None, "query_only", true).unwrap();
        let R::ActivitySources(sources) = execute(&c, None, C::ActivitySources).unwrap() else {
            panic!()
        };
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "one");
        assert!(matches!(sources[0].client, Some(AgentClient::Codex)));
        let json = serde_json::to_string(&sources).unwrap();
        assert!(!json.contains("sealed") && !json.contains("digest"));
    }

    #[test]
    fn notification_session_filter_applies_before_pagination() {
        let c = database();
        insert(&c, "a", r#""extra":true"#);
        insert(&c, "b", r#""extra":true"#);
        c.execute(
            "UPDATE agent_tasks SET body=json_set(body,'$.contextId','other') WHERE id='b'",
            [],
        )
        .unwrap();
        let rows = summaries(
            &c,
            None,
            ActivitySummaryFilter {
                session_id: Some("session".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "t:a");
    }

    #[test]
    fn summaries_are_global_stable_and_read_only() {
        let c = database();
        insert(&c, "a", r#""extra":true"#);
        insert(&c, "b", r#""extra":true"#);
        c.pragma_update(None, "query_only", true).unwrap();
        let changes = c.total_changes();
        let rows = summaries(&c, None, ActivitySummaryFilter::default()).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["t:b", "t:a"]
        );
        assert!(rows.iter().all(|r| r.uncertain));
        let next = summaries(
            &c,
            None,
            ActivitySummaryFilter {
                before: Some("t:b".into()),
                before_time: Some(42),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, "t:a");
        assert!(
            summaries(
                &c,
                None,
                ActivitySummaryFilter {
                    query: Some("missing".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(changes, c.total_changes());
    }

    #[test]
    fn legacy_task_queries_preserve_raw_facts_without_configuration_or_recovery() {
        let c = database();
        let original = insert(
            &c,
            "task",
            r#""policySnapshot":{"model":"retired","policy":{"network":false}},"executionReceipt":{"protocol":3,"unknown":"receipt"},"operation":{"kind":"retiredOperation"},"futureField":{"keep":[1,2]}"#,
        );
        assert!(serde_json::from_str::<AgentTask>(&original).is_err());
        c.execute(
            "UPDATE agent_configuration SET body='broken optional configuration'",
            [],
        )
        .unwrap();
        c.pragma_update(None, "query_only", true).unwrap();
        let before = c.total_changes();
        let R::Task(Some(value)) = execute(&c, None, C::Task { id: "task".into() }).unwrap() else {
            panic!()
        };
        assert_eq!(value.raw_body, original);
        assert!(value.active);
        assert_eq!(value.display.phase.as_deref(), Some("interruptedUnknown"));
        assert_eq!(value.display.output_start, Some(7));
        assert_eq!(value.display.output_end, Some(19));
        let R::Tasks(page) = execute(
            &c,
            None,
            C::Tasks {
                filter: TaskHistoryFilter {
                    session_id: Some("session".into()),
                    entry_id: Some("entry".into()),
                    connection_id: Some("connection".into()),
                    phase: Some("interruptedUnknown".into()),
                    ..Default::default()
                },
            },
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(page.tasks.len(), 1);
        assert_eq!(page.tasks[0].raw_body, original);
        assert_eq!(page.next_offset, None);
        assert_eq!(c.total_changes(), before);
        assert_eq!(
            c.query_row("SELECT body FROM agent_tasks", [], |row| row
                .get::<_, String>(0))
                .unwrap(),
            original
        );
    }

    #[test]
    fn corrupt_task_json_and_identity_conflicts_are_rejected() {
        for body in [
            "{broken",
            r#"{"id":"other","entryId":"entry","connectionId":"connection","createdAt":42}"#,
            r#"{"id":"task","entryId":"other","connectionId":"connection","createdAt":42}"#,
            r#"{"id":"task","entryId":"entry","connectionId":"other","createdAt":42}"#,
            r#"{"id":"task","entryId":"entry","connectionId":"connection","createdAt":43}"#,
        ] {
            let c = database();
            c.execute(
                "INSERT INTO agent_tasks VALUES('task','entry','connection',42,0,NULL,NULL,?1)",
                [body],
            )
            .unwrap();
            c.pragma_update(None, "query_only", true).unwrap();
            assert!(task(&c, "task").is_err());
            assert!(tasks(&c, TaskHistoryFilter::default()).is_err());
            assert_eq!(
                c.query_row("SELECT body FROM agent_tasks", [], |row| row
                    .get::<_, String>(0))
                    .unwrap(),
                body
            );
        }
    }

    #[test]
    fn historical_receipts_keep_original_fingerprints_and_expired_tombstones() {
        let c = database();
        insert(&c, "task", r#""legacy":true"#);
        let fingerprint = "original fingerprint, not a new hash";
        c.execute(
            "INSERT INTO agent_receipts VALUES('entry','same-key','task',?1,42)",
            [fingerprint],
        )
        .unwrap();
        let original = receipt(&c, "entry", "same-key").unwrap().unwrap();
        assert!(original.task_present);
        assert_eq!(original.fingerprint, fingerprint);
        c.execute("DELETE FROM agent_tasks", []).unwrap();
        c.pragma_update(None, "query_only", true).unwrap();
        let tombstone = receipt(&c, "entry", "same-key").unwrap().unwrap();
        assert!(!tombstone.task_present);
        assert_eq!(tombstone.fingerprint, fingerprint);
        assert_eq!(tombstone.task_id, original.task_id);
        assert!(receipt(&c, "other-entry", "same-key").unwrap().is_none());
    }

    #[test]
    fn receipt_cannot_present_a_task_owned_by_another_entry() {
        let c = database();
        insert(&c, "task", r#""legacy":true"#);
        c.execute(
            "INSERT INTO agent_receipts VALUES('different-entry','key','task','fingerprint',42)",
            [],
        )
        .unwrap();
        c.pragma_update(None, "query_only", true).unwrap();
        assert!(receipt(&c, "different-entry", "key").is_err());
    }

    #[test]
    fn task_pages_are_bounded_and_keep_deterministic_continuation() {
        let c = database();
        for index in 0..101 {
            insert(&c, &format!("task-{index:03}"), r#""legacy":true"#);
        }
        let first = tasks(&c, TaskHistoryFilter::default()).unwrap();
        assert_eq!(first.tasks.len(), 100);
        assert_eq!(first.next_offset, Some(100));
        let second = tasks(
            &c,
            TaskHistoryFilter {
                offset: first.next_offset.unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second.tasks.len(), 1);
        assert_eq!(second.tasks[0].id, "task-000");
        assert_eq!(second.next_offset, None);
        let large = database();
        for index in 0..3 {
            insert(
                &large,
                &format!("large-{index}"),
                &format!(r#""legacy":"{}""#, "x".repeat(900_000)),
            );
        }
        let page = tasks(&large, TaskHistoryFilter::default()).unwrap();
        assert_eq!(page.tasks.len(), 2);
        assert_eq!(page.next_offset, Some(2));
        let oversized = database();
        insert(
            &oversized,
            "too-large",
            &format!(r#""legacy":"{}""#, "x".repeat(MAX_BODY_BYTES)),
        );
        assert!(tasks(&oversized, TaskHistoryFilter::default()).is_err());
    }

    #[test]
    fn workspace_history_route_does_not_create_recording_storage() {
        use vigilshell_application_contracts::ports::{DbCommand, DbReply, WorkspaceRepository};
        let root = tempfile::tempdir().unwrap();
        let mut repository = crate::SqlitePreferences::open(root.path()).unwrap();
        insert(&repository.connection, "task", r#""legacy":true"#);
        repository
            .connection
            .pragma_update(None, "query_only", true)
            .unwrap();
        assert!(matches!(
            repository
                .execute(DbCommand::AgentHistory(C::Task { id: "task".into() }))
                .unwrap(),
            DbReply::AgentHistory(R::Task(Some(_)))
        ));
        assert!(
            matches!(repository.execute(DbCommand::AgentHistory(C::Recordings { filter: RecordingFilter::default() })).unwrap(), DbReply::AgentHistory(R::Recordings(page)) if page.sessions.is_empty())
        );
        assert!(!root.path().join("agent-recordings").exists());
    }
}
