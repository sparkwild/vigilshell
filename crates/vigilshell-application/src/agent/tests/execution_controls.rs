use super::*;

fn terminal() -> RuntimeOperation {
    RuntimeOperation::Terminal {
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv".into(),
        },
        columns: 80,
        rows: 24,
    }
}

async fn elapse(milliseconds: u64) {
    for _ in 0..milliseconds / 250 {
        tokio::time::advance(Duration::from_millis(250)).await;
        // 让执行循环、持久化事件和测试适配器处理本次时间推进。
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }
}

async fn wait_phase(f: &Fixture, id: &str, phase: TaskPhase) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if f.service.get_task(id).await.unwrap().phase == phase {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn fresh_terminal_input_is_rejected_while_queued_and_stopping() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .limits
        .concurrent_terminals = 1;
    let first = f.start(terminal(), "first-terminal").await;
    f.running(&first.id).await;
    let mut view = f.service.state.lock().unwrap().contexts["session"]
        .view
        .lock()
        .unwrap()
        .clone();
    view.id = "second-session".into();
    f.service.create_recording(&view, &f.entry).await.unwrap();
    f.service.state.lock().unwrap().contexts.insert(
        view.id.clone(),
        Arc::new(Context {
            target_tab: None,
            created_terminal: false,
            workspace_tab: Mutex::new(None),
            closing: tokio::sync::Mutex::new(()),
            recorded_policies: Mutex::new(Default::default()),
            unconfirmed: AtomicBool::new(false),
            entry_generation: 1,
            view: Mutex::new(view),
            cancel: CancellationToken::new(),
        }),
    );
    let AgentReply::Task { task: queued } = f
        .service
        .start_operation(
            f.entry.clone(),
            RuntimeCall {
                parent_task_id: None,
                session_id: "second-session".into(),
                request_key: "queued-terminal".into(),
                timeout_seconds: None,
                operation: terminal(),
            },
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        f.service.get_task(&queued.id).await.unwrap().phase,
        TaskPhase::Queued
    );
    let input = TerminalControl::Write {
        text: "must not send\n".into(),
    };
    assert_eq!(
        f.service
            .terminal_input(
                &f.entry.id,
                &queued.id,
                "queued-input".into(),
                input.clone()
            )
            .await
            .unwrap_err(),
        "terminal_not_running"
    );
    f.service
        .cancel_task(&first.id, ActivityActor::Agent)
        .await
        .unwrap();
    wait_phase(&f, &first.id, TaskPhase::Stopping).await;
    assert_eq!(
        f.service
            .terminal_input(&f.entry.id, &first.id, "stopping-input".into(), input)
            .await
            .unwrap_err(),
        "terminal_not_running"
    );
    assert!(f.connection.inputs.lock().unwrap().is_empty());
    f.finished(&first.id).await;
    f.running(&queued.id).await;
    assert_eq!(f.connection.requests.lock().unwrap().len(), 2);
    f.service
        .cancel_task(&queued.id, ActivityActor::Agent)
        .await
        .unwrap();
    f.finished(&queued.id).await;
    f.service.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn silent_command_is_not_subject_to_terminal_idle_timeout() {
    let f = Fixture::new().await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.policies[0]
            .policy
            .execution
            .limits
            .terminal_idle_seconds = Some(1);
        state.config.policies[0]
            .policy
            .execution
            .limits
            .max_task_seconds = Some(30);
    }
    f.connection.hold.store(true, Ordering::Release);
    f.connection.confirm_stop.store(true, Ordering::Release);
    let task = f
        .start(
            RuntimeOperation::Command {
                command: "sleep 30".into(),
                directory: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: "srv".into(),
                },
            },
            "silent-command",
        )
        .await;
    f.running(&task.id).await;
    elapse(2500).await;
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    f.service
        .cancel_task(&task.id, ActivityActor::Agent)
        .await
        .unwrap();
    let finished = f.finished(&task.id).await;
    assert!(matches!(
        finished.execution_state.unwrap().outcome,
        Some(ExecutionOutcome::Partial {
            reason: ExecutionStopReason::Cancelled,
            termination_confirmed: true,
        })
    ));
    f.service.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn terminal_input_and_output_refresh_idle_deadline_before_silence_times_out() {
    let f = Fixture::new().await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.policies[0]
            .policy
            .execution
            .limits
            .terminal_idle_seconds = Some(2);
        state.config.policies[0]
            .policy
            .execution
            .limits
            .terminal_seconds = Some(30);
    }
    f.connection.confirm_stop.store(true, Ordering::Release);
    let task = f.start(terminal(), "idle-terminal").await;
    f.running(&task.id).await;
    elapse(1000).await;
    let reply = f
        .service
        .terminal_input(
            &f.entry.id,
            &task.id,
            "keep-alive".into(),
            TerminalControl::Write {
                text: "activity\n".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(reply, AgentReply::Input { state, .. } if state == "sent"));
    elapse(1250).await;
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    f.connection
        .terminal_events
        .lock()
        .unwrap()
        .push_back(AgentProcessEvent::Stdout(b"more output".to_vec()));
    elapse(1250).await;
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    elapse(2250).await;
    let finished = f.finished(&task.id).await;
    assert_eq!(finished.error.as_deref(), Some("terminal_idle_timeout"));
    assert!(matches!(
        finished.execution_state.unwrap().outcome,
        Some(ExecutionOutcome::Partial {
            reason: ExecutionStopReason::TimedOut,
            termination_confirmed: true,
        })
    ));
    assert_eq!(f.connection.inputs.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}

async fn held_upload(f: &Fixture) -> (AgentTask, FileExecutionControl) {
    let root = std::fs::canonicalize(f._directory.path()).unwrap();
    std::fs::write(root.join("upload.txt"), b"fixture upload").unwrap();
    {
        let mut state = f.service.state.lock().unwrap();
        let policy = &mut state.config.policies[0].policy.execution;
        policy.upload_roots = vec![LocalDirectoryGrant {
            id: "upload".into(),
            path: root.to_str().unwrap().into(),
        }];
    }
    f.connection.file_hold.store(true, Ordering::Release);
    let task = f
        .start(
            RuntimeOperation::TransfersUpload {
                local: ScopedPath {
                    scope_id: "upload".into(),
                    path: "upload.txt".into(),
                },
                remote: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: "srv/upload.txt".into(),
                },
                overwrite: false,
            },
            "held-upload",
        )
        .await;
    f.running(&task.id).await;
    let control = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(control) = f.connection.file_control.lock().unwrap().clone() {
                break control;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    (task, control)
}

#[tokio::test(start_paused = true)]
async fn transfer_progress_refreshes_stall_deadline_and_stall_requests_cancellation() {
    let f = Fixture::new().await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.policies[0]
            .policy
            .execution
            .limits
            .transfer_seconds = None;
        state.config.policies[0]
            .policy
            .execution
            .limits
            .transfer_stall_seconds = Some(2);
    }
    let (task, control) = held_upload(&f).await;
    elapse(1000).await;
    control.bytes.store(4, Ordering::Release);
    elapse(1750).await;
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    assert!(!control.cancel.load(Ordering::Acquire));
    elapse(750).await;
    let finished = f.finished(&task.id).await;
    assert!(control.cancel.load(Ordering::Acquire));
    assert_eq!(finished.file_effects, Some(FileCommitState::Unchanged));
    assert!(matches!(
        finished.execution_state.unwrap().outcome,
        Some(ExecutionOutcome::Stopped {
            reason: ExecutionStopReason::TimedOut,
        })
    ));
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn transfer_progress_does_not_extend_total_deadline() {
    let f = Fixture::new().await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.policies[0]
            .policy
            .execution
            .limits
            .transfer_seconds = Some(2);
        state.config.policies[0]
            .policy
            .execution
            .limits
            .transfer_stall_seconds = Some(5);
    }
    let (task, control) = held_upload(&f).await;
    for bytes in 1..=6 {
        control.bytes.store(bytes, Ordering::Release);
        elapse(250).await;
        assert_eq!(
            f.service.get_task(&task.id).await.unwrap().phase,
            TaskPhase::Running
        );
    }
    elapse(750).await;
    let finished = f.finished(&task.id).await;
    assert!(control.cancel.load(Ordering::Acquire));
    assert!(matches!(
        finished.execution_state.unwrap().outcome,
        Some(ExecutionOutcome::Stopped {
            reason: ExecutionStopReason::TimedOut,
        })
    ));
    assert_eq!(finished.file_effects, Some(FileCommitState::Unchanged));
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}
