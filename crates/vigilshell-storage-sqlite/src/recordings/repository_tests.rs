use super::*;
use rusqlite::Connection;
use vigilshell_application_contracts::ports::{
    AgentDbReply as R, AgentHistoryCommand as H, AgentHistoryReply as HR, RecordingCommand as C,
};
use vigilshell_domain::agent::*;

fn history_sessions(c: &Connection, filter: RecordingFilter) -> Vec<HistoricalRecording> {
    let HR::Recordings(page) =
        crate::agent_history::execute(c, None, H::Recordings { filter }).unwrap()
    else {
        panic!()
    };
    page.sessions
}

fn session(id: &str) -> RecordedSession {
    RecordedSession {
        id: id.into(),
        connection_id: "connection".into(),
        connection_name: "server".into(),
        entry_id: "entry".into(),
        entry_name: "Trusted configured name".into(),
        created_at: 10,
        ended_at: None,
        result: None,
        incomplete: false,
        last_sequence: 0,
        bytes: 0,
    }
}
fn append(id: &str, text: &str, limit_bytes: u64) -> C {
    C::Append {
        record: ActivityRecord {
            session_id: id.into(),
            task_id: Some("task".into()),
            parent_task_id: None,
            sequence: 0,
            timestamp_ms: 10,
            actor: ActivityActor::Agent,
            policy_revision: 1,
            body: ActivityBody::Output {
                format: vigilshell_domain::agent::ActivityOutputFormat::Text,
                data: None,
                stream: "terminal".into(),
                text: text.into(),
            },
        },
        limit_bytes,
    }
}
fn database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    connection
}
fn read(c: &mut Connection, store: &mut RecordingStore, id: &str) -> ActivityPage {
    let R::Activity(page) = execute(
        c,
        store,
        C::Read {
            include_output: true,
            session_id: id.into(),
            task_id: None,
            cursor: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    page
}

#[test]
fn restart_keeps_complete_history_and_marks_active_execution_unknown() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    for id in ["complete", "active"] {
        execute(
            &mut c,
            &mut store,
            C::Create {
                session: session(id),
            },
        )
        .unwrap();
        execute(&mut c, &mut store, append(id, "中文\u{1b}[2J", 10000)).unwrap();
    }
    execute(
        &mut c,
        &mut store,
        C::End {
            session_id: "complete".into(),
            ended_at: 20,
            result: "succeeded".into(),
            incomplete: false,
        },
    )
    .unwrap();
    drop(store);
    let mut store = RecordingStore::open(root.path()).unwrap();
    execute(&mut c, &mut store, C::Recover { now: 30 }).unwrap();
    assert!(!read(&mut c, &mut store, "complete").incomplete);
    assert!(read(&mut c, &mut store, "active").incomplete);
    let sessions = history_sessions(
        &c,
        RecordingFilter {
            session_id: Some("active".into()),
            ..Default::default()
        },
    );
    assert_eq!(sessions[0].result.as_deref(), Some("interruptedUnknown"));
    assert!(execute(&mut c, &mut store, append("active", "never resume", 10000)).is_err());
}

#[test]
fn failed_index_commit_preserves_evidence_and_marks_gap() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    execute(
        &mut c,
        &mut store,
        C::Create {
            session: session("active"),
        },
    )
    .unwrap();
    c.execute_batch("CREATE TRIGGER fail_index BEFORE INSERT ON agent_recording_events BEGIN SELECT RAISE(ABORT,'disk fault'); END").unwrap();
    assert!(
        execute(
            &mut c,
            &mut store,
            append("active", "persisted before failed index", 10000)
        )
        .is_err()
    );
    assert_eq!(store.recover("active").unwrap().positions.len(), 1);
    assert!(read(&mut c, &mut store, "active").incomplete);
    execute(&mut c, &mut store, C::Recover { now: 30 }).unwrap();
    assert!(read(&mut c, &mut store, "active").incomplete);
}

#[test]
fn quota_removes_oldest_ended_session_only_and_never_truncates_active() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    for id in ["old", "active"] {
        execute(
            &mut c,
            &mut store,
            C::Create {
                session: session(id),
            },
        )
        .unwrap();
        execute(&mut c, &mut store, append(id, &"x".repeat(1000), 10000)).unwrap();
    }
    execute(
        &mut c,
        &mut store,
        C::End {
            session_id: "old".into(),
            ended_at: 20,
            result: "succeeded".into(),
            incomplete: false,
        },
    )
    .unwrap();
    execute(
        &mut c,
        &mut store,
        append("active", &"x".repeat(1000), 3000),
    )
    .unwrap();
    assert!(!root.path().join("agent-recordings/old").exists());
    assert_eq!(read(&mut c, &mut store, "active").records.len(), 2);
    assert!(
        execute(
            &mut c,
            &mut store,
            append("active", &"x".repeat(1000), 3000)
        )
        .is_err()
    );
    let page = read(&mut c, &mut store, "active");
    assert!(page.incomplete);
    assert_eq!(page.records.len(), 2);
}

#[test]
fn screen_checkpoints_survive_reopen_without_becoming_business_steps() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    execute(
        &mut c,
        &mut store,
        C::Create {
            session: session("checkpoint"),
        },
    )
    .unwrap();
    execute(
        &mut c,
        &mut store,
        append("checkpoint", "original output", 1024 * 1024),
    )
    .unwrap();
    let record = ActivityRecord {
        session_id: "checkpoint".into(),
        task_id: Some("task".into()),
        parent_task_id: None,
        sequence: 0,
        timestamp_ms: 12,
        actor: ActivityActor::System,
        policy_revision: 1,
        body: ActivityBody::Screen {
            checkpoint: ScreenCheckpoint {
                version: 1,
                sequence: 1,
                columns: 80,
                rows: 24,
                gzip: "derived-data".into(),
            },
        },
    };
    execute(
        &mut c,
        &mut store,
        C::Append {
            record,
            limit_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    drop(store);
    let mut store = RecordingStore::open(root.path()).unwrap();
    let HR::Screen(Some(checkpoint)) = crate::agent_history::execute(
        &c,
        Some(root.path()),
        H::Screen {
            session_id: "checkpoint".into(),
            task_id: "task".into(),
        },
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(checkpoint.sequence, 1);
    assert_eq!(checkpoint.gzip, "derived-data");
    let R::Activity(page) = execute(
        &mut c,
        &mut store,
        C::Read {
            include_output: false,
            session_id: "checkpoint".into(),
            task_id: None,
            cursor: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    assert!(page.records.is_empty());
    let page = read(&mut c, &mut store, "checkpoint");
    assert_eq!(page.records.len(), 2);
    assert!(
        matches!(&page.records[0].body,ActivityBody::Output {text,..} if text=="original output")
    );
}

#[test]
fn optional_checkpoint_after_session_end_does_not_mark_history_incomplete() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    execute(
        &mut c,
        &mut store,
        C::Create {
            session: session("ended"),
        },
    )
    .unwrap();
    execute(&mut c, &mut store, append("ended", "kept", 1024 * 1024)).unwrap();
    execute(
        &mut c,
        &mut store,
        C::End {
            session_id: "ended".into(),
            ended_at: 11,
            result: "closed".into(),
            incomplete: false,
        },
    )
    .unwrap();
    let record = ActivityRecord {
        session_id: "ended".into(),
        task_id: Some("task".into()),
        parent_task_id: None,
        sequence: 0,
        timestamp_ms: 12,
        actor: ActivityActor::System,
        policy_revision: 1,
        body: ActivityBody::Screen {
            checkpoint: ScreenCheckpoint {
                version: 1,
                sequence: 1,
                columns: 80,
                rows: 24,
                gzip: "cache".into(),
            },
        },
    };
    assert!(matches!(
        execute(
            &mut c,
            &mut store,
            C::AppendScreen {
                record,
                limit_bytes: 1024 * 1024
            }
        )
        .unwrap(),
        R::Done
    ));
    let page = read(&mut c, &mut store, "ended");
    assert!(!page.incomplete);
    assert_eq!(page.records.len(), 1);
}

#[test]
fn recording_list_filters_state_name_and_stable_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    let mut c = database();
    for id in ["a", "b", "c"] {
        execute(
            &mut c,
            &mut store,
            C::Create {
                session: session(id),
            },
        )
        .unwrap();
    }
    execute(
        &mut c,
        &mut store,
        C::End {
            session_id: "b".into(),
            ended_at: 20,
            result: "closed".into(),
            incomplete: false,
        },
    )
    .unwrap();
    let active = history_sessions(
        &c,
        RecordingFilter {
            active: Some(true),
            query: Some("SERVER".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        active
            .iter()
            .map(|value| value.id.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a"]
    );
    let ended = history_sessions(
        &c,
        RecordingFilter {
            active: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(ended[0].id, "b");
    let next = history_sessions(
        &c,
        RecordingFilter {
            before_created_at: Some(10),
            before_id: Some("b".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        next.iter()
            .map(|value| value.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
}
