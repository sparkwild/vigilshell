use super::*;
async fn commit(f: &Fixture, change: AgentMutation) -> AgentResult<AgentReply> {
    let AgentReply::Challenge { id, .. } = f.service.prepare(change).await? else {
        panic!()
    };
    f.service.commit(&id).await
}
fn current(f: &Fixture) -> ConnectionPolicySettings {
    f.service.state.lock().unwrap().config.policies[0].clone()
}
fn gate(f: &Fixture, enabled: bool) -> AgentMutation {
    let p = current(f);
    AgentMutation::SetPolicyAccess {
        connection_id: p.connection_id.clone(),
        expected_revision: p.revision,
        connection_revision: p.connection_revision,
        enabled,
    }
}
fn patch(f: &Fixture, patch: PolicySettingsPatch) -> AgentMutation {
    let p = current(f);
    AgentMutation::UpdatePolicySettings {
        connection_id: p.connection_id.clone(),
        expected_revision: p.revision,
        connection_revision: p.connection_revision,
        patch,
    }
}
#[tokio::test]
async fn policy_settings_require_closed_access_and_do_not_reenable_or_change_other_fields() {
    let f = Fixture::new().await;
    assert!(matches!(
        f.service
            .prepare(patch(&f, PolicySettingsPatch::VerifyConnect(false)))
            .await,
        Err("policy_access_enabled")
    ));
    commit(&f, gate(&f, false)).await.unwrap();
    let before = current(&f);
    commit(&f, patch(&f, PolicySettingsPatch::VerifyConnect(false)))
        .await
        .unwrap();
    let after = current(&f);
    assert!(!after.access_enabled);
    assert!(!after.verify_connect);
    assert_eq!(after.execution, before.execution);
    assert!(matches!(
        f.service
            .open_context(f.entry.clone(), &f.connection_id, "closed", None)
            .await,
        Err("connection_denied")
    ));
    let revision = after.revision;
    commit(&f, patch(&f, PolicySettingsPatch::VerifyConnect(false)))
        .await
        .unwrap();
    assert_eq!(current(&f).revision, revision);
    commit(&f, gate(&f, true)).await.unwrap();
    assert!(current(&f).access_enabled);
    f.service.shutdown().await;
}
#[tokio::test]
async fn policy_settings_reject_stale_authorization_after_another_field_is_saved() {
    let f = Fixture::new().await;
    commit(&f, gate(&f, false)).await.unwrap();
    let AgentReply::Challenge { id, .. } = f.service.prepare(gate(&f, true)).await.unwrap() else {
        panic!()
    };
    commit(&f, patch(&f, PolicySettingsPatch::VerifyConnect(false)))
        .await
        .unwrap();
    assert!(matches!(
        f.service.commit(&id).await,
        Err("authorization_expired")
    ));
    assert!(!current(&f).access_enabled);
    f.service.shutdown().await;
}
#[tokio::test]
async fn policy_settings_reject_invalid_limits_and_preserve_saved_configuration() {
    let f = Fixture::new().await;
    commit(&f, gate(&f, false)).await.unwrap();
    let before = current(&f);
    assert!(matches!(
        f.service
            .prepare(patch(
                &f,
                PolicySettingsPatch::Limit(PolicyLimitPatch {
                    key: PolicyLimitKey::ConcurrentTasks,
                    value: Some(0)
                })
            ))
            .await,
        Err("invalid_limits")
    ));
    assert_eq!(current(&f), before);
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_settings_close_isolates_admission_without_closing_other_contexts() {
    let f = Fixture::new().await;
    let other = {
        let mut state = f.service.state.lock().unwrap();
        let mut view = state.contexts["session"].view.lock().unwrap().clone();
        view.id = "unrelated-session".into();
        view.connection_id = "unrelated-connection".into();
        let context = Arc::new(Context {
            target_tab: None,
            created_terminal: false,
            workspace_tab: Mutex::new(None),
            closing: tokio::sync::Mutex::new(()),
            recorded_policies: Mutex::new(Default::default()),
            unconfirmed: std::sync::atomic::AtomicBool::new(false),
            entry_generation: 1,
            view: Mutex::new(view),
            cancel: CancellationToken::new(),
        });
        state
            .contexts
            .insert("unrelated-session".into(), context.clone());
        context
    };
    let close = gate(&f, false);
    let (_, closed) = tokio::join!(
        f.service
            .open_context(f.entry.clone(), &f.connection_id, "racing-open", None),
        commit(&f, close)
    );
    closed.unwrap();
    assert!(!other.cancel.is_cancelled());
    assert!(
        f.service
            .state
            .lock()
            .unwrap()
            .contexts
            .values()
            .filter(|c| c.view.lock().unwrap().connection_id == f.connection_id)
            .all(|c| !c.cancel.is_cancelled())
    );
    assert!(matches!(
        f.service
            .open_context(f.entry.clone(), &f.connection_id, "after-close", None)
            .await,
        Err("connection_denied")
    ));
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_settings_edit_and_reenable_leave_running_work_unchanged() {
    let f = Fixture::new().await;
    f.connection.hold.store(true, Ordering::Release);
    let task = f.start(server_query(), "running-policy").await;
    f.running(&task.id).await;
    commit(&f, gate(&f, false)).await.unwrap();
    commit(
        &f,
        patch(&f, PolicySettingsPatch::Level(OperationLevel::ReadOnly)),
    )
    .await
    .unwrap();
    assert!(!current(&f).access_enabled);
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    assert!(
        !f.service.state.lock().unwrap().contexts["session"]
            .cancel
            .is_cancelled()
    );
    commit(&f, gate(&f, true)).await.unwrap();
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    f.connection.hold.store(false, Ordering::Release);
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Succeeded);
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_settings_closed_access_denies_queued_work_even_after_reenable() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .limits
        .concurrent_tasks = 1;
    f.connection.hold.store(true, Ordering::Release);
    let running = f.start(server_query(), "running").await;
    f.running(&running.id).await;
    let queued = f.start(server_query(), "queued").await;
    assert_eq!(
        f.service.get_task(&queued.id).await.unwrap().phase,
        TaskPhase::Queued
    );
    commit(&f, gate(&f, false)).await.unwrap();
    assert!(matches!(
        f.service
            .start_operation(
                f.entry.clone(),
                RuntimeCall {
                    parent_task_id: None,
                    session_id: "session".into(),
                    request_key: "denied".into(),
                    timeout_seconds: None,
                    operation: server_query(),
                }
            )
            .await,
        Err("connection_changed")
    ));
    commit(&f, gate(&f, true)).await.unwrap();
    f.connection.hold.store(false, Ordering::Release);
    assert_eq!(f.finished(&running.id).await.phase, TaskPhase::Succeeded);
    let denied = f.finished(&queued.id).await;
    assert_ne!(denied.phase, TaskPhase::Succeeded);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_settings_closed_access_denies_input_without_stopping_terminal() {
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
    commit(&f, gate(&f, false)).await.unwrap();
    let reply = f
        .service
        .terminal_input(
            &f.entry.id,
            &task.id,
            "closed-input".into(),
            TerminalControl::Write {
                text: "whoami\n".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(reply, AgentReply::Input { state, .. } if state == "notSent"));
    assert!(f.connection.inputs.lock().unwrap().is_empty());
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_settings_closed_access_denies_script_approval() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .script_review = true;
    let task = f
        .start(
            RuntimeOperation::Script {
                source: ScriptSource::Inline {
                    text: "printf test".into(),
                },
                interpreter: ScriptInterpreter::Sh,
                arguments: vec![],
                directory: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: "srv/app".into(),
                },
            },
            "review",
        )
        .await;
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
    commit(&f, gate(&f, false)).await.unwrap();
    assert!(matches!(
        f.service.commit(&id).await,
        Err("authorization_expired")
    ));
    assert!(
        commit(
            &f,
            AgentMutation::ApproveScript {
                task_id: task.id.clone()
            }
        )
        .await
        .is_err()
    );
    assert!(f.connection.requests.lock().unwrap().is_empty());
    f.service.shutdown().await;
}
