use super::*;
use std::io::Write;
use vigilshell_domain::agent::{ActivityActor, ActivityBody};

fn record(session: &str, text: &str) -> ActivityRecord {
    ActivityRecord {
        session_id: session.into(),
        task_id: Some("task".into()),
        parent_task_id: None,
        sequence: 999,
        timestamp_ms: 1,
        actor: ActivityActor::Agent,
        policy_revision: 1,
        body: ActivityBody::Output {
            format: vigilshell_domain::agent::ActivityOutputFormat::Text,
            data: None,
            stream: "terminal".into(),
            text: text.into(),
        },
    }
}

#[test]
fn output_larger_than_old_limit_survives_segments_and_reopen() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    store.segment_bytes = 512 * 1024;
    store.create("session").unwrap();
    let text = "中文\u{1b}[2J\r\n".repeat(8192);
    let mut positions = Vec::new();
    for index in 0..80 {
        let position = store
            .append(&mut record("session", &text), 64 * 1024 * 1024)
            .unwrap();
        assert_eq!(position.sequence, index + 1);
        positions.push(position);
    }
    assert!(positions.last().unwrap().segment > 0);
    assert!(store.remove_closed("session").is_err());
    drop(store);
    let store = RecordingStore::open(root.path()).unwrap();
    let recovered = store.recover("session").unwrap();
    assert!(!recovered.incomplete);
    assert!(recovered.bytes > 4 * 1024 * 1024);
    assert_eq!(recovered.positions.len(), 80);
    for position in positions {
        let saved = store.read(&position).unwrap();
        let ActivityBody::Output { text: actual, .. } = saved.body else {
            panic!()
        };
        assert_eq!(actual, text);
    }
}

#[test]
fn corrupt_and_partial_tail_are_preserved_and_reported() {
    for truncated in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = RecordingStore::open(root.path()).unwrap();
        store.create("session").unwrap();
        let first = store
            .append(&mut record("session", "first"), 10000)
            .unwrap();
        let second = store
            .append(&mut record("session", "second"), 10000)
            .unwrap();
        store.close("session");
        let path = store.root.join("session/00000000.vsr");
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        if truncated {
            file.set_len(second.offset + 20).unwrap();
        } else {
            file.seek(SeekFrom::End(-1)).unwrap();
            file.write_all(b"!").unwrap();
        }
        let original = fs::read(&path).unwrap();
        let result = store.recover("session").unwrap();
        assert!(result.incomplete);
        assert_eq!(result.positions.len(), 1);
        assert!(store.read(&first).is_ok());
        assert!(store.read(&second).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}

#[test]
fn quota_failure_never_discards_active_output_or_resumes_after_gap() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    store.create("session").unwrap();
    let position = store.append(&mut record("session", "kept"), 10000).unwrap();
    assert_eq!(
        store.append(&mut record("session", "lost"), 1).unwrap_err(),
        RecordingError::Quota
    );
    assert_eq!(
        store
            .append(&mut record("session", "later"), 10000)
            .unwrap_err(),
        RecordingError::Gap
    );
    assert_eq!(store.read(&position).unwrap().sequence, 1);
    store.close("session");
    assert!(store.create("session").is_err());
}

#[test]
fn removed_reordered_or_injected_segments_are_incomplete() {
    for remove in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = RecordingStore::open(root.path()).unwrap();
        store.segment_bytes = 1;
        store.create("session").unwrap();
        for _ in 0..3 {
            store
                .append(&mut record("session", "value"), 10000)
                .unwrap();
        }
        store.close("session");
        let directory = store.root.join("session");
        if remove {
            fs::remove_file(directory.join("00000001.vsr")).unwrap();
        } else {
            fs::copy(
                directory.join("00000000.vsr"),
                directory.join("00000001.vsr"),
            )
            .unwrap();
        }
        assert!(store.recover("session").unwrap().incomplete);
    }
}

#[test]
fn unsafe_session_paths_and_symlink_roots_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut store = RecordingStore::open(root.path()).unwrap();
    for id in ["", "../other", "/tmp/other", "a/b"] {
        assert!(store.create(id).is_err());
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), store.root.join("symlink")).unwrap();
        assert!(store.recover("symlink").is_err());
        assert!(store.create("symlink").is_err());
        assert!(store.remove_closed("symlink").is_err());
    }
}
