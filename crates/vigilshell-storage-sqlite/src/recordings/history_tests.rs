use super::*;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use vigilshell_application_contracts::ports::{AgentHistoryCommand as C, AgentHistoryReply as R};
use vigilshell_domain::agent::RecordingFilter;

// 固定历史文件格式夹具，不经当前 ActivityBody/RuntimePolicy 编码。
fn frame(raw: &str, previous: [u8; 32]) -> (Vec<u8>, [u8; 32]) {
    let mut hash = Sha256::new();
    hash.update(b"VSRLOG01");
    hash.update(previous);
    hash.update(raw.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut bytes = b"VSRLOG01".to_vec();
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&previous);
    bytes.extend_from_slice(&digest);
    bytes.extend_from_slice(raw.as_bytes());
    (bytes, digest)
}

fn raw(sequence: u64, kind: &str) -> String {
    format!(
        r#"{{ "sessionId":"session", "sequence":{sequence}, "taskId":"task", "timestampMs":42, "actor":"retiredActor", "policyRevision":8, "body":{{"kind":"{kind}","legacyPolicy":{{"network":false,"serviceMode":"retired"}}}}, "unknownEnvelope": true }}"#
    )
}

struct Fixture {
    root: tempfile::TempDir,
    database: Connection,
    positions: Vec<RecordingPosition>,
    originals: Vec<String>,
    path: PathBuf,
}

impl Fixture {
    fn new(count: u64) -> Self {
        let root = tempfile::tempdir().unwrap();
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        let directory = root.path().join("agent-recordings/session");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("00000000.vsr");
        let mut bytes = Vec::new();
        let mut previous = [0; 32];
        let mut positions = Vec::new();
        let mut originals = Vec::new();
        for sequence in 1..=count {
            let original = raw(
                sequence,
                if sequence % 2 == 0 {
                    "output"
                } else {
                    "policy"
                },
            );
            let (next, digest) = frame(&original, previous);
            let position = RecordingPosition {
                session_id: "session".into(),
                segment: 0,
                offset: bytes.len() as u64,
                length: next.len() as u32,
                sequence,
            };
            bytes.extend_from_slice(&next);
            previous = digest;
            positions.push(position);
            originals.push(original);
        }
        fs::write(&path, &bytes).unwrap();
        let body = r#"{ "id":"session", "connectionId":"connection", "entryId":"entry", "createdAt":42, "connectionName":"old server", "entryName":"old client", "unknown":{"keep":true} }"#;
        database.execute("INSERT INTO agent_recordings VALUES('session','connection','entry',42,NULL,NULL,0,?1,?2,0,?3)", params![count as i64,bytes.len() as i64,body]).unwrap();
        for position in &positions {
            database
                .execute(
                    "INSERT INTO agent_recording_events VALUES('session',?1,'task',?2,42,?3)",
                    params![
                        position.sequence as i64,
                        if position.sequence % 2 == 0 {
                            "output"
                        } else {
                            "policy"
                        },
                        serde_json::to_string(position).unwrap()
                    ],
                )
                .unwrap();
        }
        Self {
            root,
            database,
            positions,
            originals,
            path,
        }
    }

    fn history(
        &self,
        cursor: u64,
        include_output: bool,
    ) -> std::result::Result<R, vigilshell_domain::workspace::WorkspaceError> {
        crate::agent_history::execute(
            &self.database,
            Some(self.root.path()),
            C::Activity {
                session_id: "session".into(),
                task_id: Some("task".into()),
                cursor,
                include_output,
            },
        )
    }
}

#[test]
fn legacy_frames_and_sessions_are_readable_without_changing_original_evidence() {
    let fixture = Fixture::new(2);
    fixture
        .database
        .execute(
            "UPDATE agent_configuration SET body='unreadable optional configuration'",
            [],
        )
        .unwrap();
    fixture
        .database
        .pragma_update(None, "query_only", true)
        .unwrap();
    let changes = fixture.database.total_changes();
    let original_file = fs::read(&fixture.path).unwrap();
    let store = RecordingStore::open_read_only(fixture.root.path()).unwrap();
    assert!(store.read(&fixture.positions[0]).is_err());
    let recovered = store.recover("session").unwrap();
    assert!(!recovered.incomplete);
    assert_eq!(recovered.positions.len(), 2);
    let R::Activity(page) = fixture.history(0, true).unwrap() else {
        panic!()
    };
    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[0].raw_json, fixture.originals[0]);
    assert_eq!(page.records[1].raw_json, fixture.originals[1]);
    assert_eq!(page.records[0].previous_digest, "0".repeat(64));
    assert_eq!(page.records[1].previous_digest, page.records[0].digest);
    assert_eq!(page.next_cursor, 2);
    assert!(!page.has_more);
    assert!(!page.incomplete);
    let R::Recordings(recordings) = crate::agent_history::execute(
        &fixture.database,
        Some(fixture.root.path()),
        C::Recordings {
            filter: RecordingFilter::default(),
        },
    )
    .unwrap() else {
        panic!()
    };
    assert!(
        recordings.sessions[0]
            .raw_body
            .contains(r#""unknown":{"keep":true}"#)
    );
    assert_eq!(recordings.sessions[0].ended_at, None);
    assert_eq!(recordings.sessions[0].result, None);
    assert_eq!(fixture.database.total_changes(), changes);
    assert_eq!(fs::read(&fixture.path).unwrap(), original_file);
}

#[test]
fn unknown_event_kind_does_not_break_frame_integrity_or_recovery() {
    let fixture = Fixture::new(1);
    let original = raw(1, "retiredMaintenanceEvent");
    let (bytes, _) = frame(&original, [0; 32]);
    fs::write(&fixture.path, &bytes).unwrap();
    let store = RecordingStore::open_read_only(fixture.root.path()).unwrap();
    let recovered = store.recover("session").unwrap();
    assert!(!recovered.incomplete);
    let record = store.read_history(&recovered.positions[0]).unwrap();
    assert_eq!(record.kind, "retiredMaintenanceEvent");
    assert_eq!(record.raw_json, original);
    assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
}

#[test]
fn malformed_json_and_bad_digest_remain_corrupt_without_repair() {
    for malformed_json in [false, true] {
        let fixture = Fixture::new(1);
        let mut bytes = if malformed_json {
            frame("{ invalid JSON", [0; 32]).0
        } else {
            fs::read(&fixture.path).unwrap()
        };
        if !malformed_json {
            bytes[44] ^= 1;
        }
        fs::write(&fixture.path, &bytes).unwrap();
        let store = RecordingStore::open_read_only(fixture.root.path()).unwrap();
        assert!(store.recover("session").unwrap().incomplete);
        assert!(store.read_history(&fixture.positions[0]).is_err());
        fixture
            .database
            .pragma_update(None, "query_only", true)
            .unwrap();
        assert!(fixture.history(0, true).is_err());
        assert_eq!(fs::read(&fixture.path).unwrap(), bytes);
        assert!(
            !fixture
                .database
                .query_row("SELECT incomplete FROM agent_recordings", [], |row| row
                    .get::<_, bool>(0))
                .unwrap()
        );
    }
}

#[test]
fn index_identity_and_position_mismatches_are_rejected_without_writes() {
    for change in [
        "UPDATE agent_recording_events SET position=json_set(position,'$.sequence',2)",
        "UPDATE agent_recording_events SET position=json_set(position,'$.sessionId','other')",
        "UPDATE agent_recording_events SET position=json_set(position,'$.length',1)",
        "UPDATE agent_recording_events SET position=json_set(position,'$.offset',1)",
        "UPDATE agent_recording_events SET task_id='other'",
        "UPDATE agent_recording_events SET kind='operation'",
        "UPDATE agent_recording_events SET timestamp_ms=43",
        "UPDATE agent_recordings SET body=json_set(body,'$.id','other')",
    ] {
        let fixture = Fixture::new(1);
        fixture.database.execute_batch(change).unwrap();
        fixture
            .database
            .pragma_update(None, "query_only", true)
            .unwrap();
        let changes = fixture.database.total_changes();
        let result = crate::agent_history::execute(
            &fixture.database,
            Some(fixture.root.path()),
            C::Activity {
                session_id: "session".into(),
                task_id: None,
                cursor: 0,
                include_output: true,
            },
        );
        assert!(result.is_err(), "{change}");
        assert_eq!(fixture.database.total_changes(), changes);
    }
}

#[test]
fn historical_pages_keep_original_cursors_and_bound_the_number_of_frames() {
    let fixture = Fixture::new(257);
    fixture
        .database
        .pragma_update(None, "query_only", true)
        .unwrap();
    let R::Activity(first) = fixture.history(0, true).unwrap() else {
        panic!()
    };
    assert_eq!(first.records.len(), 256);
    assert_eq!(first.next_cursor, 256);
    assert!(first.has_more);
    let R::Activity(last) = fixture.history(first.next_cursor, true).unwrap() else {
        panic!()
    };
    assert_eq!(last.records.len(), 1);
    assert_eq!(last.next_cursor, 257);
    assert!(!last.has_more);
    let R::Activity(steps) = fixture.history(0, false).unwrap() else {
        panic!()
    };
    assert_eq!(steps.records.len(), 129);
    assert_eq!(steps.next_cursor, 257);
    assert!(steps.records.iter().all(|record| record.kind == "policy"));
    assert!(fixture.history(u64::MAX, true).is_err());
}

#[test]
fn read_only_open_never_creates_a_directory_and_rejects_symlink_sessions() {
    let root = tempfile::tempdir().unwrap();
    assert!(RecordingStore::open_read_only(root.path()).is_err());
    assert!(!root.path().join("agent-recordings").exists());
    #[cfg(unix)]
    {
        let fixture = Fixture::new(1);
        let outside = tempfile::tempdir().unwrap();
        fs::rename(
            fixture.path.parent().unwrap(),
            outside.path().join("session"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("session"),
            fixture.path.parent().unwrap(),
        )
        .unwrap();
        let store = RecordingStore::open_read_only(fixture.root.path()).unwrap();
        assert!(store.read_history(&fixture.positions[0]).is_err());
        assert!(store.recover("session").is_err());
    }
}

#[test]
fn summary_feed_reads_independent_events_and_excludes_output() {
    let fixture = Fixture::new(4);
    fixture
        .database
        .pragma_update(None, "query_only", true)
        .unwrap();
    let R::ActivitySummaries(items) = crate::agent_history::execute(
        &fixture.database,
        Some(fixture.root.path()),
        C::ActivitySummaries {
            filter: Default::default(),
        },
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|item| item.kind == "policy" && item.task_id.is_none())
    );
    assert!(items.iter().all(|item| item.started_at.is_none()));
}
