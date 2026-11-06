use super::*;
use rusqlite::types::Value as SqlValue;
use serde_json::json;
use vigilshell_domain::agent::{AgentConfiguration, ExecutionPolicy};

fn fixture(c: &Connection) {
    c.execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/002_file_editor.sql"))
        .unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    c.execute_batch(include_str!("../migrations/003_monitor_display.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/004_monitor_presets.sql"))
        .unwrap();
    c.execute_batch(include_str!("../migrations/005_system_notifications.sql"))
        .unwrap();
    c.execute_batch(include_str!(
        "../migrations/006_remove_completion_notifications.sql"
    ))
    .unwrap();
    c.execute_batch(include_str!("../migrations/007_updates.sql"))
        .unwrap();
    c.pragma_update(None, "user_version", crate::SCHEMA_VERSION)
        .unwrap();
    let mut config = AgentConfiguration::default();
    config.settings.revision = 7;
    config.settings.endpoint_revision = 2;
    config.entries = vec![
        AccessSecret {
            entry: vigilshell_domain::agent::AccessEntry {
                client: Some(vigilshell_domain::agent::AgentClient::Dsh),
                id: "entry".into(),
                name: "synthetic access".into(),
                enabled: true,
                generation: 5,
            },
            digest: "a".repeat(64),
            sealed: "synthetic-sealed-token".into(),
        },
        AccessSecret {
            entry: vigilshell_domain::agent::AccessEntry {
                client: None,
                id: "revoked".into(),
                name: "retained revoked entry".into(),
                enabled: false,
                generation: 9,
            },
            digest: String::new(),
            sealed: String::new(),
        },
    ];
    config.policies.push(ConnectionPolicySettings {
        access_enabled: false,
        policy: ConnectionPolicy {
            connection_id: "00000000-0000-4000-8000-000000000001".into(),
            connection_revision: 3,
            revision: 4,
            verify_connect: true,
            execution: ExecutionPolicy::default(),
        },
    });
    let mut legacy = serde_json::to_value(&config).unwrap();
    legacy["policies"][0]
        .as_object_mut()
        .unwrap()
        .remove("accessEnabled");
    let old = legacy["policies"][0]["execution"].as_object_mut().unwrap();
    old.insert("version".into(), json!(1));
    old.remove("scriptReview");
    old.insert("commandNetwork".into(), json!(false));
    c.execute(
        "UPDATE agent_configuration SET revision=7,body=?1",
        [legacy.to_string()],
    )
    .unwrap();
    c.execute("INSERT INTO connection_profiles(id,group_sort_order,version,revision,config) VALUES('00000000-0000-4000-8000-000000000001',0,1,3,zeroblob(48))", []).unwrap();
    c.execute_batch(
        "UPDATE credential_vault SET initialized=1; UPDATE connection_configuration SET verifier=zeroblob(48);
         INSERT INTO encrypted_credentials VALUES('00000000-0000-4000-8000-000000000002','00000000-0000-4000-8000-000000000001','ssh_password',1,1,zeroblob(24),zeroblob(32));
         UPDATE connection_profiles SET credential_ref='00000000-0000-4000-8000-000000000002';
         INSERT INTO host_trusts VALUES('00000000-0000-4000-8000-000000000001','fixture.invalid',22,'synthetic-public-key','synthetic-fingerprint','ssh-ed25519','2026-01-01T00:00:00Z');"
    ).unwrap();
    let history = r#"{ "id":"history", "entryId":"entry", "connectionId":"00000000-0000-4000-8000-000000000001", "createdAt":1, "phase":"interruptedUnknown", "policySnapshot":{"model":"retired","payload":true}, "executionReceipt":{"executorPid":12} }"#;
    c.execute("INSERT INTO agent_tasks(id,entry_id,connection_id,created_at,active,request_key,fingerprint,body) VALUES('history','entry','00000000-0000-4000-8000-000000000001',1,0,'key','original-fingerprint',?1)", [history]).unwrap();
    c.execute(
        "INSERT INTO agent_receipts VALUES('entry','key','history','original-fingerprint',1)",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO agent_receipts VALUES('entry','expired','removed-task','retained-fingerprint',2)", []).unwrap();
    c.execute("INSERT INTO agent_access_observations VALUES('entry',5,'http://127.0.0.1:51602/mcp',2,1,3)", []).unwrap();
}

fn tables(c: &Connection) -> Vec<(String, Vec<Vec<SqlValue>>)> {
    let names = c
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'agent_configuration' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let mut query = c.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
            let columns = query.column_count();
            let rows = query
                .query_map([], |r| {
                    (0..columns).map(|i| r.get::<_, SqlValue>(i)).collect()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (name, rows)
        })
        .collect()
}

#[test]
fn conversion_preserves_every_other_table_tokens_and_historical_bytes() {
    let mut c = Connection::open_in_memory().unwrap();
    fixture(&c);
    let before = tables(&c);
    let old: Value = serde_json::from_str(&configuration(&c).unwrap().1).unwrap();
    let schema_before: String = c
        .query_row(
            "SELECT group_concat(sql,';') FROM sqlite_schema ORDER BY name",
            [],
            |r| r.get(0),
        )
        .unwrap();
    c.execute_batch("PRAGMA query_only=ON").unwrap();
    let changes = c.total_changes();
    let plan = prepare(&c).unwrap();
    assert_eq!(c.total_changes(), changes);
    assert_eq!(plan.preview().source_revision, 7);
    assert_eq!(plan.preview().target_revision, 8);
    assert_eq!(
        plan.preview().connection_ids,
        ["00000000-0000-4000-8000-000000000001"]
    );
    c.execute_batch("PRAGMA query_only=OFF").unwrap();
    assert_eq!(apply(&mut c, &plan), Ok(ConversionOutcome::Applied));
    let (revision, raw) = configuration(&c).unwrap();
    let new: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(revision, 8);
    assert_eq!(new["entries"], old["entries"]);
    let mut expected = old;
    expected["policies"][0]["accessEnabled"] = false.into();
    expected["policies"][0]["execution"]["level"] = "readOnly".into();
    expected["settings"]["revision"] = json!(8);
    expected["policies"][0]["revision"] = json!(5);
    expected["policies"][0]["execution"]
        .as_object_mut()
        .unwrap()
        .remove("commandNetwork");
    expected["policies"][0]["execution"]["version"] = json!(2);
    expected["policies"][0]["execution"]["scriptReview"] = json!(false);
    assert_eq!(new, expected);
    assert_eq!(tables(&c), before);
    let schema_after: String = c
        .query_row(
            "SELECT group_concat(sql,';') FROM sqlite_schema ORDER BY name",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema_after, schema_before);
    assert_eq!(
        apply(&mut c, &plan),
        Ok(ConversionOutcome::AlreadyConverted)
    );
    let again = prepare(&c).unwrap();
    assert_eq!(apply(&mut c, &again), Ok(ConversionOutcome::Unchanged));
    assert_eq!(configuration(&c).unwrap(), (revision, raw));
}

#[test]
fn commit_failure_rolls_back_and_same_plan_can_resume_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.sqlite3");
    let mut writer = Connection::open(&path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    fixture(&writer);
    let before = configuration(&writer).unwrap();
    let protected = tables(&writer);
    let plan = prepare(&writer).unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    configuration(&reader).unwrap();
    // 另一连接持有读锁，使更新后的 COMMIT 失败，不依赖产品代码的故障注入入口。
    assert_eq!(
        apply(&mut writer, &plan),
        Err(ConversionError::StorageUnavailable)
    );
    reader.execute_batch("ROLLBACK").unwrap();
    assert_eq!(configuration(&writer).unwrap(), before);
    assert_eq!(tables(&writer), protected);
    assert_eq!(apply(&mut writer, &plan), Ok(ConversionOutcome::Applied));
    assert_eq!(
        apply(&mut writer, &plan),
        Ok(ConversionOutcome::AlreadyConverted)
    );
}

#[test]
fn preparation_and_apply_reject_active_tasks_without_settling_them() {
    let mut c = Connection::open_in_memory().unwrap();
    fixture(&c);
    let plan = prepare(&c).unwrap();
    c.execute("UPDATE agent_tasks SET active=1 WHERE id='history'", [])
        .unwrap();
    let before = configuration(&c).unwrap();
    assert!(matches!(prepare(&c), Err(ConversionError::ActiveTasks)));
    assert_eq!(apply(&mut c, &plan), Err(ConversionError::ActiveTasks));
    assert_eq!(configuration(&c).unwrap(), before);
    assert_eq!(
        c.query_row(
            "SELECT active FROM agent_tasks WHERE id='history'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn stale_revision_or_changed_source_never_overwrites_current_configuration() {
    for changed_revision in [false, true] {
        let mut c = Connection::open_in_memory().unwrap();
        fixture(&c);
        let plan = prepare(&c).unwrap();
        if changed_revision {
            c.execute("UPDATE agent_configuration SET revision=8", [])
                .unwrap();
        } else {
            c.execute("UPDATE agent_configuration SET body=body||' '", [])
                .unwrap();
        }
        let original = configuration(&c).unwrap();
        assert_eq!(apply(&mut c, &plan), Err(ConversionError::Conflict));
        assert_eq!(configuration(&c).unwrap(), original);
    }
}

#[test]
fn unknown_missing_or_malformed_policy_is_not_guessed_or_partially_converted() {
    for change in [
        json!({"version":9}),
        json!({"version":1,"level":"operate"}),
        json!({"version":2,"level":"operate","commandNetwork":true}),
    ] {
        let c = Connection::open_in_memory().unwrap();
        fixture(&c);
        let mut value: Value = serde_json::from_str(&configuration(&c).unwrap().1).unwrap();
        value["policies"].as_array_mut().unwrap().push(json!({
            "connectionId":"other","connectionRevision":0,"revision":0,"verifyConnect":true,"execution":change
        }));
        c.execute(
            "UPDATE agent_configuration SET body=?1",
            [value.to_string()],
        )
        .unwrap();
        let before = configuration(&c).unwrap();
        assert!(matches!(
            prepare(&c),
            Err(ConversionError::InvalidConfiguration)
        ));
        assert_eq!(configuration(&c).unwrap(), before);
    }
}

#[test]
fn rejects_foreign_storage_without_rewriting_metadata() {
    let c = Connection::open_in_memory().unwrap();
    fixture(&c);
    let original = configuration(&c).unwrap();
    c.pragma_update(None, "application_id", 42).unwrap();
    assert!(matches!(
        prepare(&c),
        Err(ConversionError::UnsupportedStorage)
    ));
    assert_eq!(configuration(&c).unwrap(), original);
    assert_eq!(
        c.pragma_query_value(None, "application_id", |r| r.get::<_, u32>(0))
            .unwrap(),
        42
    );
}

#[test]
fn ambiguous_duplicate_fields_are_rejected_without_losing_tokens_or_granting_access() {
    for nested in [false, true] {
        let c = Connection::open_in_memory().unwrap();
        fixture(&c);
        let raw = configuration(&c).unwrap().1;
        let ambiguous = if nested {
            raw.replace(
                "\"level\":\"disabled\"",
                "\"level\":\"disabled\",\"level\":\"operate\"",
            )
        } else {
            format!("{},\"entries\":[]}}", raw.strip_suffix('}').unwrap())
        };
        assert_ne!(ambiguous, raw);
        c.execute("UPDATE agent_configuration SET body=?1", [&ambiguous])
            .unwrap();
        let protected = tables(&c);
        assert!(matches!(
            prepare(&c),
            Err(ConversionError::InvalidConfiguration)
        ));
        assert_eq!(configuration(&c).unwrap(), (7, ambiguous));
        assert_eq!(tables(&c), protected);
    }
}

#[test]
fn conversion_resumes_after_database_reopen_and_never_advances_twice() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.sqlite3");
    let mut writer = Connection::open(&path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    fixture(&writer);
    let before = configuration(&writer).unwrap();
    let protected = tables(&writer);
    let plan = prepare(&writer).unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    configuration(&reader).unwrap();
    assert_eq!(
        apply(&mut writer, &plan),
        Err(ConversionError::StorageUnavailable)
    );
    drop(reader);
    drop(writer);
    let mut reopened = Connection::open(&path).unwrap();
    assert_eq!(configuration(&reopened).unwrap(), before);
    assert_eq!(tables(&reopened), protected);
    let resumed = prepare(&reopened).unwrap();
    assert_eq!(
        apply(&mut reopened, &resumed),
        Ok(ConversionOutcome::Applied)
    );
    let converted = configuration(&reopened).unwrap();
    drop(reopened);
    let mut reopened = Connection::open(&path).unwrap();
    let unchanged = prepare(&reopened).unwrap();
    assert_eq!(
        apply(&mut reopened, &unchanged),
        Ok(ConversionOutcome::Unchanged)
    );
    assert_eq!(configuration(&reopened).unwrap(), converted);
    assert_eq!(converted.0, before.0 + 1);
    assert_eq!(tables(&reopened), protected);
}

#[test]
fn access_split_preserves_existing_enabled_states_and_other_settings() {
    for level in ["disabled", "readOnly", "operate"] {
        let mut c = Connection::open_in_memory().unwrap();
        fixture(&c);
        let (_, raw) = configuration(&c).unwrap();
        let mut old: Value = serde_json::from_str(&raw).unwrap();
        old["policies"][0]["execution"]["version"] = 2.into();
        old["policies"][0]["execution"]
            .as_object_mut()
            .unwrap()
            .remove("commandNetwork");
        old["policies"][0]["execution"]["scriptReview"] = false.into();
        old["policies"][0]["execution"]["level"] = level.into();
        c.execute("UPDATE agent_configuration SET body=?1", [old.to_string()])
            .unwrap();
        let before = tables(&c);
        let plan = prepare(&c).unwrap();
        apply(&mut c, &plan).unwrap();
        let (_, raw) = configuration(&c).unwrap();
        let current: AgentConfiguration = serde_json::from_str(&raw).unwrap();
        assert_eq!(current.policies[0].access_enabled, level != "disabled");
        assert_eq!(current.policies[0].connection_revision, 3);
        assert_eq!(current.policies[0].revision, 5);
        assert_eq!(tables(&c), before);
        assert!(prepare(&c).unwrap().preview.connection_ids.is_empty());
    }
}
