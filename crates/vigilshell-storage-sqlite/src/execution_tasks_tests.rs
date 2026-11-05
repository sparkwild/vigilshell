use super::*;
fn task(id: &str) -> ExecutionTask {
    let request:ExecutionRequest=serde_json::from_value(serde_json::json!({"identity":{"taskId":id,"entryId":"entry","entryGeneration":1,"connectionId":"connection","connectionRevision":1,"sessionId":"session","policyRevision":1},"requestKey":id,"timeoutSeconds":null,"operation":{"kind":"command","command":"pwd","directory":{"scopeId":"__account__","path":"srv"}}})).unwrap();
    ExecutionTask {
        id: id.into(),
        entry_id: "entry".into(),
        connection_id: "connection".into(),
        context_id: "session".into(),
        created_at: 1,
        finished_at: None,
        revision: 0,
        request,
        source_request: None,
        policy: ConnectionPolicy {
            connection_id: "connection".into(),
            connection_revision: 1,
            revision: 1,
            verify_connect: true,
            execution: ExecutionPolicy {
                level: OperationLevel::ReadOnly,
                ..Default::default()
            },
        },
        state: ExecutionState {
            phase: ExecutionPhase::Queued,
            review: ScriptReviewState::NotRequired {},
            dispatch: DispatchState::NotDispatched {},
            outcome: None,
            terminal: false,
        },
        phase: None,
        error: None,
        output_incomplete: false,
        file_report: None,
        native_report: None,
        script_snapshot: None,
        script_staging_path: None,
    }
}
fn db() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!("migrations/001_initial.sql"))
        .unwrap();
    c
}
#[test]
fn receipts_atomic_cas_and_history_tombstones_cannot_replay() {
    let mut c = db();
    let t = task("first");
    execute(&mut c, C::Register(Box::new(t.clone()))).unwrap();
    let mut duplicate = t.clone();
    duplicate.id = "second".into();
    duplicate.request.identity.task_id = duplicate.id.clone();
    assert!(matches!(
        execute(&mut c, C::Register(Box::new(duplicate.clone()))),
        Ok(R::Registered {
            inserted: false,
            ..
        })
    ));
    duplicate.request.timeout_seconds = Some(1);
    assert!(matches!(
        execute(&mut c, C::Register(Box::new(duplicate))),
        Err(WorkspaceError::Conflict)
    ));
    assert!(get(&c, "entry", "second").unwrap().is_none());
    let mut changed = t.clone();
    changed.state.begin_dispatch("attempt".into(), 2).unwrap();
    changed.revision = 1;
    execute(
        &mut c,
        C::Save {
            task: Box::new(changed.clone()),
            expected: 0,
        },
    )
    .unwrap();
    assert!(
        execute(
            &mut c,
            C::Save {
                task: Box::new(changed.clone()),
                expected: 0
            }
        )
        .is_err()
    );
    changed.state = t.state.clone();
    changed.revision = 2;
    assert!(
        execute(
            &mut c,
            C::Save {
                task: Box::new(changed),
                expected: 1
            }
        )
        .is_err()
    );
    c.execute("DELETE FROM agent_tasks WHERE id='first'", [])
        .unwrap();
    assert!(matches!(
        execute(&mut c, C::Register(Box::new(t))),
        Err(WorkspaceError::NotFound)
    ));
    c.execute(
        "INSERT INTO agent_receipts VALUES('entry','old','historical','original-fingerprint',1)",
        [],
    )
    .unwrap();
    assert!(matches!(
        execute(&mut c, C::Register(Box::new(task("old")))),
        Err(WorkspaceError::Conflict)
    ));
}
#[test]
fn restart_recovery_preserves_old_raw_and_new_history_remains_queryable() {
    let mut c = db();
    execute(&mut c, C::Register(Box::new(task("queued")))).unwrap();
    let mut attempted = task("sent");
    execute(&mut c, C::Register(Box::new(attempted.clone()))).unwrap();
    attempted.state.begin_dispatch("attempt".into(), 2).unwrap();
    attempted.revision = 1;
    execute(
        &mut c,
        C::Save {
            task: Box::new(attempted),
            expected: 0,
        },
    )
    .unwrap();
    let old = "{\"id\":\"old\",\"entryId\":\"entry\",\"connectionId\":\"connection\",\"createdAt\":1,\"phase\":\"oldUnknown\"}";
    c.execute(
        "INSERT INTO agent_tasks VALUES('old','entry','connection',1,1,NULL,NULL,?1)",
        [old],
    )
    .unwrap();
    execute(&mut c, C::Recover).unwrap();
    assert!(matches!(
        get(&c, "entry", "queued").unwrap().unwrap().state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Restarted
        })
    ));
    assert!(matches!(
        get(&c, "entry", "sent").unwrap().unwrap().state.outcome,
        Some(ExecutionOutcome::Unknown {
            reason: ExecutionStopReason::Restarted
        })
    ));
    let raw: String = c
        .query_row("SELECT body FROM agent_tasks WHERE id='old'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(raw, old);
    let before = get(&c, "entry", "sent").unwrap().unwrap().revision;
    execute(&mut c, C::Recover).unwrap();
    assert_eq!(get(&c, "entry", "sent").unwrap().unwrap().revision, before);
    use vigilshell_application_contracts::ports::{AgentHistoryCommand, AgentHistoryReply};
    let AgentHistoryReply::Tasks(page) = crate::agent_history::execute(
        &c,
        None,
        AgentHistoryCommand::Tasks {
            filter: TaskHistoryFilter {
                phase: Some("failed".into()),
                ..Default::default()
            },
        },
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(page.tasks.len(), 1);
    assert!(
        page.tasks
            .iter()
            .all(|t| t.display.command.as_deref() == Some("pwd"))
    );
}
#[test]
fn registration_constraint_failure_rolls_back_task_and_receipt() {
    let mut c = db();
    c.execute_batch("CREATE TRIGGER reject_receipt BEFORE INSERT ON agent_receipts BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(execute(&mut c, C::Register(Box::new(task("new")))).is_err());
    assert_eq!(
        c.query_row("SELECT count(*) FROM agent_tasks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn current_task_reads_reject_index_body_mismatch() {
    let mut c = db();
    execute(&mut c, C::Register(Box::new(task("task")))).unwrap();
    c.execute(
        "UPDATE agent_tasks SET connection_id='another' WHERE id='task'",
        [],
    )
    .unwrap();
    assert!(matches!(
        get(&c, "entry", "task"),
        Err(WorkspaceError::StorageUnavailable)
    ));
}

#[test]
fn file_task_recovery_keeps_recovery_paths_and_history_label() {
    let mut c = db();
    let mut file = task("file");
    file.policy.execution.level = OperationLevel::Operate;
    file.request.operation = RuntimeOperation::FilesWrite {
        target: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/app/file".into(),
        },
        data: "YQ==".into(),
        mode: FileWriteMode::Create,
        expected_hash: None,
    };
    execute(&mut c, C::Register(Box::new(file.clone()))).unwrap();
    file.state.begin_dispatch("attempt".into(), 2).unwrap();
    file.revision = 1;
    file.file_report = Some(FileExecutionReport {
        commit: FileCommitState::Unchanged,
        error: None,
        json: None,
        recovery_paths: vec!["/srv/app/.vigilshell-file.part".into()],
        bytes: 0,
        hash: None,
    });
    execute(
        &mut c,
        C::Save {
            task: Box::new(file),
            expected: 0,
        },
    )
    .unwrap();
    execute(&mut c, C::Recover).unwrap();
    let file = get(&c, "entry", "file").unwrap().unwrap();
    let report = file.file_report.unwrap();
    assert_eq!(report.commit, FileCommitState::Unknown);
    assert_eq!(
        report.recovery_paths,
        vec!["/srv/app/.vigilshell-file.part"]
    );
    use vigilshell_application_contracts::ports::{AgentHistoryCommand, AgentHistoryReply};
    let AgentHistoryReply::Task(Some(history)) =
        crate::agent_history::execute(&c, None, AgentHistoryCommand::Task { id: "file".into() })
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(history.display.tool.as_deref(), Some("files_write.write"));
}

#[test]
fn native_recovery_keeps_backup_and_never_replays_attempt() {
    let mut c = db();
    let mut t = task("native");
    t.policy.execution.level = OperationLevel::Operate;
    t.request.operation = RuntimeOperation::PackagesInstall {
        packages: vec![PackageTarget::Repository {
            name: "example".into(),
            version: None,
        }],
    };
    execute(&mut c, C::Register(Box::new(t.clone()))).unwrap();
    t.state.begin_dispatch("attempt".into(), 2).unwrap();
    t.revision = 1;
    t.native_report = Some(NativeExecutionReport {
        outcome: None,
        effects: FileCommitState::Partial,
        error: None,
        json: None,
        recovery_paths: vec!["/root/.local/share/vigilshell/source-backups/native".into()],
    });
    execute(
        &mut c,
        C::Save {
            task: Box::new(t.clone()),
            expected: 0,
        },
    )
    .unwrap();
    execute(&mut c, C::Recover).unwrap();
    let recovered = get(&c, "entry", "native").unwrap().unwrap();
    let report = recovered.native_report.unwrap();
    assert_eq!(report.effects, FileCommitState::Unknown);
    assert!(matches!(
        report.outcome,
        Some(ExecutionOutcome::Unknown {
            reason: ExecutionStopReason::Restarted
        })
    ));
    assert_eq!(
        report.recovery_paths,
        t.native_report.unwrap().recovery_paths
    );
    assert!(matches!(
        execute(&mut c, C::Register(Box::new(task("native")))),
        Err(WorkspaceError::Conflict)
    ));
}
#[test]
fn accepted_power_request_stays_accepted_after_history_recovery() {
    let mut c = db();
    let mut power = task("power");
    power.policy.execution.level = OperationLevel::Operate;
    power.request.operation = RuntimeOperation::PowerControl {
        action: PowerAction::Reboot,
    };
    execute(&mut c, C::Register(Box::new(power.clone()))).unwrap();
    power.state.begin_dispatch("attempt".into(), 2).unwrap();
    power.revision = 1;
    execute(
        &mut c,
        C::Save {
            task: Box::new(power.clone()),
            expected: 0,
        },
    )
    .unwrap();
    power
        .state
        .settle(ExecutionOutcome::RequestAccepted {})
        .unwrap();
    power.revision = 2;
    power.native_report = Some(NativeExecutionReport {
        outcome: Some(ExecutionOutcome::RequestAccepted {}),
        effects: FileCommitState::Unknown,
        error: None,
        json: Some("{\"effectConfirmed\":false}".into()),
        recovery_paths: vec![],
    });
    execute(
        &mut c,
        C::Save {
            task: Box::new(power),
            expected: 1,
        },
    )
    .unwrap();
    execute(&mut c, C::Recover).unwrap();
    let power = get(&c, "entry", "power").unwrap().unwrap();
    assert!(!power.state.active());
    assert!(matches!(
        power.state.outcome,
        Some(ExecutionOutcome::RequestAccepted {})
    ));
    assert_eq!(
        power.native_report.unwrap().effects,
        FileCommitState::Unknown
    );
}

#[test]
fn script_source_version_and_review_record_are_immutable_and_restart_never_dispatches() {
    use sha2::{Digest, Sha256};
    let mut c = db();
    let mut t = task("review-file");
    t.policy.execution.level = OperationLevel::Operate;
    t.policy.execution.script_review = true;
    t.request.operation = RuntimeOperation::Script {
        source: ScriptSource::RemoteFile {
            target: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/deploy.sh".into(),
            },
            expected_hash: None,
        },
        interpreter: ScriptInterpreter::Bash,
        arguments: vec![],
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv".into(),
        },
    };
    let snapshot = ScriptSnapshot {
        source_version: Some("f".repeat(64)),
        text: "printf reviewed".into(),
        hash: format!("{:x}", Sha256::digest(b"printf reviewed")),
        path: "/srv/deploy.sh".into(),
        directory: "/srv".into(),
        interpreter: ScriptInterpreter::Bash,
        arguments: vec![],
    };
    let binding = ScriptReviewBinding {
        identity: t.request.identity.clone(),
        snapshot: snapshot.clone(),
    };
    t.script_snapshot = Some(snapshot);
    t.state.review = ScriptReviewState::WaitingReview {
        binding: binding.clone(),
    };
    t.state.phase = ExecutionPhase::WaitingReview;
    execute(&mut c, C::Register(Box::new(t.clone()))).unwrap();
    let mut changed = t.clone();
    changed.script_snapshot.as_mut().unwrap().source_version = Some("e".repeat(64));
    assert!(
        execute(
            &mut c,
            C::Save {
                task: Box::new(changed),
                expected: 0
            }
        )
        .is_err()
    );
    assert_eq!(
        get(&c, "entry", &t.id).unwrap().unwrap().script_snapshot,
        t.script_snapshot
    );
    t.state.approve_verified(&binding).unwrap();
    t.revision += 1;
    execute(
        &mut c,
        C::Save {
            task: Box::new(t.clone()),
            expected: 0,
        },
    )
    .unwrap();
    execute(&mut c, C::Recover).unwrap();
    let recovered = get(&c, "entry", &t.id).unwrap().unwrap();
    assert!(matches!(
        recovered.state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Restarted
        })
    ));
    assert!(matches!(
        recovered.state.dispatch,
        DispatchState::NotDispatched {}
    ));
    assert_eq!(recovered.script_snapshot, t.script_snapshot);
    assert_eq!(recovered.view_phase(), TaskPhase::Failed);
}
