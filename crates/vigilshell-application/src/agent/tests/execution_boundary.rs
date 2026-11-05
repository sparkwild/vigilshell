use super::*;

#[tokio::test]
async fn execution_steps_persist_each_phase_and_repeat_does_not_add_steps() {
    let f = Fixture::new().await;
    let operation = RuntimeOperation::Command {
        command: "printf workbench".into(),
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/app".into(),
        },
    };
    let task = f.start(operation.clone(), "workbench").await;
    f.finished(&task.id).await;
    let records = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let AgentReply::Activity { page } = f
                .service
                .activity("session".into(), Some(task.id.clone()), 0, true)
                .await
                .unwrap()
            else {
                panic!()
            };
            if page.records.iter().any(|r| {
                matches!(
                    r.body,
                    ActivityBody::Operation {
                        phase: TaskPhase::Succeeded,
                        ..
                    }
                )
            }) {
                break page.records;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("completed execution must be discoverable in persisted workbench steps");
    let phases: Vec<_> = records
        .iter()
        .filter_map(|r| match &r.body {
            ActivityBody::Operation { phase, .. } => Some(phase.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(phases.first(), Some(&TaskPhase::Queued));
    assert!(phases.contains(&TaskPhase::Running));
    assert_eq!(phases.last(), Some(&TaskPhase::Succeeded));
    assert!(records.iter().any(|r| matches!(&r.body, ActivityBody::Operation { command: Some(command), .. } if command == "printf workbench")));
    assert!(
        records.iter().any(
            |r| matches!(&r.body, ActivityBody::Output { text, .. } if text == "command output")
        )
    );
    let persisted_output = records
        .iter()
        .filter_map(|record| {
            if let ActivityBody::Output { stream, text, .. } = &record.body {
                Some((record.sequence, stream.clone(), text.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let output_events = f
                .sink
                .0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|event| {
                    if let AgentEvent::Activity { record } = event
                        && record.task_id.as_deref() == Some(task.id.as_str())
                        && let ActivityBody::Output { stream, text, .. } = &record.body
                    {
                        Some((record.sequence, stream.clone(), text.clone()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            if output_events.len() >= persisted_output.len() {
                assert_eq!(output_events, persisted_output);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("every persisted output frame must reach the activity stream exactly once");
    let AgentReply::Output { output } = f.service.output(&task.id, 0).await.unwrap() else {
        panic!()
    };
    let queried = output
        .chunks
        .into_iter()
        .map(|chunk| (chunk.sequence, chunk.stream, chunk.text))
        .collect::<Vec<_>>();
    assert_eq!(queried, persisted_output);
    assert_eq!(f.start(operation, "workbench").await.id, task.id);
    let AgentReply::Activity { page } = f
        .service
        .activity("session".into(), Some(task.id), 0, true)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page.records.len(), records.len());
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}
#[tokio::test]
async fn one_policy_uses_durable_receipts_and_denies_writes_before_dispatch() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .level = OperationLevel::ReadOnly;
    let task = f.start(server_query(), "once").await;
    let result = f.finished(&task.id).await;
    assert_eq!(result.phase, TaskPhase::Succeeded);
    assert_eq!(result.policy_snapshot.unwrap().version, 2);
    assert_eq!(f.start(server_query(), "once").await.id, task.id);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    let denied = f
        .start(
            RuntimeOperation::PackagesInstall {
                packages: vec![PackageTarget::Repository {
                    name: "example".into(),
                    version: None,
                }],
            },
            "denied",
        )
        .await;
    assert_eq!(denied.phase, TaskPhase::Denied);
    assert_eq!(
        f.start(
            RuntimeOperation::PackagesInstall {
                packages: vec![PackageTarget::Repository {
                    name: "example".into(),
                    version: None
                }]
            },
            "denied"
        )
        .await
        .id,
        denied.id
    );
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}
#[tokio::test]
async fn failed_registration_and_unavailable_adapter_execute_zero_operations() {
    for recording in [true, false] {
        let f = Fixture::new().await;
        if recording {
            f.database.1.store(true, Ordering::Release);
        } else {
            f.connection.unavailable.store(true, Ordering::Release);
        }
        let result = f
            .service
            .start_operation(
                f.entry.clone(),
                RuntimeCall {
                    parent_task_id: None,
                    session_id: "session".into(),
                    request_key: "unavailable".into(),
                    timeout_seconds: None,
                    operation: server_query(),
                },
            )
            .await;
        if recording {
            assert!(result.is_err());
            f.database.1.store(false, Ordering::Release);
        } else {
            let AgentReply::Task { task } = result.unwrap() else {
                panic!()
            };
            let task = f.finished(&task.id).await;
            assert_eq!(task.phase, TaskPhase::Denied);
            assert_eq!(task.error.as_deref(), Some("native_preparation_failed"));
        }
        assert!(f.connection.requests.lock().unwrap().is_empty());
        f.service.shutdown().await;
    }
}
#[tokio::test]
async fn queued_revocation_prevents_start_and_keeps_original_result() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .limits
        .concurrent_tasks = 1;
    f.connection.hold.store(true, Ordering::Release);
    let first = f.start(server_query(), "first").await;
    f.running(&first.id).await;
    let second = f.start(server_query(), "second").await;
    assert_eq!(second.phase, TaskPhase::Queued);
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .level = OperationLevel::Disabled;
    f.service
        .executor
        .cancel(&f.entry.id, &first.id)
        .await
        .unwrap();
    f.finished(&first.id).await;
    assert_eq!(f.finished(&second.id).await.phase, TaskPhase::Denied);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}
#[tokio::test]
async fn pending_script_approval_is_bound_and_does_not_occupy_a_slot() {
    let f = Fixture::new().await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.policies[0].policy.execution.script_review = true;
        state.config.policies[0]
            .policy
            .execution
            .limits
            .concurrent_tasks = 1;
    }
    let operation = RuntimeOperation::Script {
        source: ScriptSource::Inline {
            text: "printf SCRIPT_EXECUTED".into(),
        },
        interpreter: ScriptInterpreter::Sh,
        arguments: vec![],
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/app".into(),
        },
    };
    let task = f.start(operation.clone(), "review").await;
    assert_eq!(task.phase, TaskPhase::WaitingReview);
    assert!(f.connection.requests.lock().unwrap().is_empty());
    let other = f.start(server_query(), "query").await;
    assert_eq!(f.finished(&other.id).await.phase, TaskPhase::Succeeded);
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::ApproveScript {
            task_id: task.id.clone(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.commit(&id).await.unwrap();
    let finished = f.finished(&task.id).await;
    assert_eq!(finished.phase, TaskPhase::Succeeded);
    assert!(finished.output_end > 0);
    let repeated = f.start(operation, "review").await;
    assert_eq!(repeated.id, task.id);
    assert_eq!(repeated.output_start, finished.output_start);
    assert_eq!(repeated.output_end, finished.output_end);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 2);
    assert!(f.service.commit(&id).await.is_err());
    f.service.shutdown().await;
}
#[tokio::test]
async fn cancellation_during_identity_wait_makes_late_approval_invalid() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .script_review = true;
    let task = f
        .start(
            RuntimeOperation::Script {
                source: ScriptSource::Inline {
                    text: "printf SCRIPT_EXECUTED".into(),
                },
                interpreter: ScriptInterpreter::Sh,
                arguments: vec![],
                directory: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: "srv/app".into(),
                },
            },
            "cancel-review",
        )
        .await;
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::ApproveScript {
            task_id: task.id.clone(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    f.service
        .cancel_task(&task.id, ActivityActor::User)
        .await
        .unwrap();
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Cancelled);
    assert!(f.service.commit(&id).await.is_err());
    assert!(f.connection.requests.lock().unwrap().is_empty());
    f.service.shutdown().await;
}

#[tokio::test]
async fn terminal_input_receipt_deduplicates_and_never_claims_remote_application() {
    let f = Fixture::new().await;
    let task = f
        .start(
            RuntimeOperation::Terminal {
                directory: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: "srv".into(),
                },
                columns: 80,
                rows: 24,
            },
            "terminal",
        )
        .await;
    f.running(&task.id).await;
    let control = TerminalControl::Write {
        text: "printf once\n".into(),
    };
    for _ in 0..2 {
        let reply = f
            .service
            .terminal_input(&f.entry.id, &task.id, "input-once".into(), control.clone())
            .await
            .unwrap();
        assert!(matches!(reply,AgentReply::Input{state,..} if state=="sent"));
    }
    assert_eq!(
        f.connection
            .inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|v| matches!(v, TerminalControl::Write { .. }))
            .count(),
        1
    );
    assert_eq!(
        f.service
            .terminal_input(
                &f.entry.id,
                &task.id,
                "input-once".into(),
                TerminalControl::Write {
                    text: "other".into()
                }
            )
            .await
            .unwrap_err(),
        "request_conflict"
    );
    f.service
        .cancel_task(&task.id, ActivityActor::Agent)
        .await
        .unwrap();
    f.finished(&task.id).await;
    assert_eq!(
        f.service
            .terminal_input(&f.entry.id, &task.id, "after-end".into(), control.clone())
            .await
            .unwrap_err(),
        "terminal_not_running"
    );
    let repeated = f
        .service
        .terminal_input(&f.entry.id, &task.id, "input-once".into(), control)
        .await
        .unwrap();
    assert!(matches!(repeated, AgentReply::Input { state, .. } if state == "sent"));
    assert_eq!(f.connection.inputs.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}

#[tokio::test]
async fn source_change_after_review_denies_before_dispatch_without_reapproval_or_replay() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .script_review = true;
    let operation = RuntimeOperation::Script {
        source: ScriptSource::RemoteFile {
            target: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/app/deploy.sh".into(),
            },
            expected_hash: None,
        },
        interpreter: ScriptInterpreter::Bash,
        arguments: vec![],
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/app".into(),
        },
    };
    let task = f.start(operation.clone(), "review-file-version").await;
    assert_eq!(task.phase, TaskPhase::WaitingReview);
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::ApproveScript {
            task_id: task.id.clone(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    *f.connection.snapshot_text.lock().unwrap() = Some("printf changed".into());
    f.service.commit(&id).await.unwrap();
    let finished = f.finished(&task.id).await;
    assert_eq!(finished.phase, TaskPhase::Denied);
    assert_eq!(finished.error.as_deref(), Some("script_source_changed"));
    assert!(matches!(
        finished.execution_state.unwrap().dispatch,
        DispatchState::NotDispatched {}
    ));
    assert!(f.connection.requests.lock().unwrap().is_empty());
    assert_eq!(f.start(operation, "review-file-version").await.id, task.id);
    assert!(f.connection.requests.lock().unwrap().is_empty());
    f.service.shutdown().await;
}

#[tokio::test]
async fn review_disabled_remote_script_does_not_prepare_or_record_a_content_snapshot() {
    let f = Fixture::new().await;
    let operation = RuntimeOperation::Script {
        source: ScriptSource::RemoteFile {
            target: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/direct.sh".into(),
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
    let task = f.start(operation, "direct-script").await;
    let result = f.finished(&task.id).await;
    assert_eq!(result.phase, TaskPhase::Succeeded);
    assert!(result.script_snapshot.is_none());
    assert!(matches!(
        result.execution_state.unwrap().review,
        ScriptReviewState::NotRequired {}
    ));
    assert_eq!(f.connection.snapshot_reads.load(Ordering::Relaxed), 0);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}

#[tokio::test]
async fn structured_result_markers_preserve_chunked_recordings_and_mcp_output_bytes() {
    let f = Fixture::new().await;
    let json = serde_json::json!({"hostname":"fixture", "detail":"中".repeat(30000)}).to_string();
    *f.connection.native_result.lock().unwrap() = Some(NativeExecutionReport {
        outcome: Some(ExecutionOutcome::Completed { exit_code: 0 }),
        effects: FileCommitState::Unchanged,
        error: None,
        json: Some(json.clone()),
        recovery_paths: vec![],
    });
    let task = f.start(server_query(), "structured-output").await;
    assert_eq!(
        f.finished(&task.id).await.result.as_deref(),
        Some(json.as_str())
    );
    let AgentReply::Activity { page } = f
        .service
        .activity("session".into(), Some(task.id.clone()), 0, true)
        .await
        .unwrap()
    else {
        panic!()
    };
    let structured: Vec<_> = page
        .records
        .iter()
        .filter_map(|r| match &r.body {
            ActivityBody::Output {
                format: ActivityOutputFormat::Structured,
                text,
                ..
            } => Some((r.sequence, text.clone())),
            _ => None,
        })
        .collect();
    assert!(structured.len() > 1);
    assert_eq!(
        structured
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<String>(),
        json
    );
    assert!(page.records.iter().any(|r|matches!(&r.body,ActivityBody::Output {format: ActivityOutputFormat::Text, text,..} if text=="中文 output\n")));
    let AgentReply::Output { output } = f.service.output(&task.id, 0).await.unwrap() else {
        panic!()
    };
    for (sequence, text) in structured {
        assert!(output.chunks.iter().any(|chunk| chunk.sequence == sequence
            && chunk.text == text
            && chunk.stream == "stdout"));
    }
    f.service.shutdown().await;
}
