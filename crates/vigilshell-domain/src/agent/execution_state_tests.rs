use super::*;
use serde_json::json;

fn binding() -> ScriptReviewBinding {
    let text = "echo fixture\n".to_owned();
    ScriptReviewBinding {
        identity: ExecutionIdentity {
            task_id: "task".into(),
            entry_id: "entry".into(),
            entry_generation: 1,
            connection_id: "connection".into(),
            connection_revision: 2,
            session_id: "session".into(),
            policy_revision: 3,
        },
        snapshot: ScriptSnapshot {
            source_version: None,
            hash: format!("{:x}", Sha256::digest(text.as_bytes())),
            text,
            path: "/tmp/script-entry".into(),
            directory: "/srv/app".into(),
            interpreter: ScriptInterpreter::Sh,
            arguments: vec!["argument with spaces".into()],
        },
    }
}

fn waiting() -> ExecutionState {
    ExecutionState {
        phase: ExecutionPhase::WaitingReview,
        review: ScriptReviewState::WaitingReview { binding: binding() },
        dispatch: DispatchState::NotDispatched {},
        outcome: None,
        terminal: false,
    }
}

fn queued(terminal: bool) -> ExecutionState {
    ExecutionState {
        phase: ExecutionPhase::Queued,
        review: ScriptReviewState::NotRequired {},
        dispatch: DispatchState::NotDispatched {},
        outcome: None,
        terminal,
    }
}

#[test]
fn snapshot_decode_checks_full_content_and_request_binding() {
    let original = binding();
    assert_eq!(original.validate(), Ok(()));
    let mut tampered = serde_json::to_value(&original).unwrap();
    tampered["snapshot"]["text"] = json!("echo changed\n");
    assert!(serde_json::from_value::<ScriptReviewBinding>(tampered).is_err());
    let request = ExecutionRequest {
        identity: original.identity.clone(),
        request_key: "key".into(),
        timeout_seconds: None,
        operation: RuntimeOperation::Script {
            source: ScriptSource::Inline {
                text: original.snapshot.text.clone(),
            },
            interpreter: original.snapshot.interpreter,
            arguments: original.snapshot.arguments.clone(),
            directory: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/app".into(),
            },
        },
    };
    assert_eq!(original.matches_request(&request), Ok(()));
    let mut policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 2,
        revision: 3,
        verify_connect: true,
        execution: ExecutionPolicy {
            level: OperationLevel::Operate,
            script_review: true,
            ..ExecutionPolicy::default()
        },
    };
    assert_eq!(waiting().validate_for(&request, &policy), Ok(()));
    assert_eq!(
        queued(false).validate_for(&request, &policy),
        Err("execution_state_request_mismatch")
    );
    policy.execution.script_review = false;
    let mut still_waiting = waiting();
    assert_eq!(still_waiting.validate_for(&request, &policy), Ok(()));
    assert_eq!(
        still_waiting.begin_dispatch("attempt".into(), 1),
        Err("execution_not_queued")
    );
    let mut changed = request.clone();
    changed.identity.entry_generation += 1;
    assert_eq!(
        original.matches_request(&changed),
        Err("script_review_request_mismatch")
    );
    let RuntimeOperation::Script { directory, .. } = &mut changed.operation else {
        unreachable!()
    };
    directory.path = "srv/other".into();
    changed.identity = original.identity.clone();
    assert_eq!(
        original.matches_request(&changed),
        Err("script_review_request_mismatch")
    );
}

#[test]
fn waiting_review_is_active_and_roundtrips() {
    let state = waiting();
    assert!(state.active());
    assert!(
        serde_json::from_value::<ExecutionState>(serde_json::to_value(&state).unwrap()).is_ok()
    );
}

#[test]
fn stale_or_late_approval_cannot_release_a_task() {
    let mut state = waiting();
    let mut stale = binding();
    stale.identity.policy_revision += 1;
    assert_eq!(
        state.approve_verified(&stale),
        Err("script_review_binding_changed")
    );
    assert_eq!(state.phase, ExecutionPhase::WaitingReview);
    state.reject_review().unwrap();
    assert_eq!(
        state.approve_verified(&binding()),
        Err("review_not_waiting")
    );
    assert_eq!(
        state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Denied
        })
    );
}

#[test]
fn dispatch_attempt_is_separate_from_approval_and_execution_confirmation() {
    let mut state = waiting();
    assert_eq!(
        state.begin_dispatch("attempt".into(), 1),
        Err("execution_not_queued")
    );
    state.approve_verified(&binding()).unwrap();
    assert_eq!(state.phase, ExecutionPhase::Queued);
    state.begin_dispatch("attempt".into(), 1).unwrap();
    assert_eq!(
        state.begin_dispatch("attempt-again".into(), 2),
        Err("execution_not_queued")
    );
    state.settle(ExecutionOutcome::RequestAccepted {}).unwrap();
    assert!(!state.active());
    assert!(!state.outcome.as_ref().unwrap().termination_confirmed());
}

#[test]
fn terminal_partial_outcome_keeps_unconfirmed_termination() {
    let mut state = queued(true);
    state.begin_dispatch("attempt".into(), 1).unwrap();
    state.phase = ExecutionPhase::Running;
    state.phase = ExecutionPhase::Stopping;
    state
        .settle(ExecutionOutcome::Partial {
            reason: ExecutionStopReason::Cancelled,
            termination_confirmed: false,
        })
        .unwrap();
    assert!(!state.active());
    assert!(!state.outcome.as_ref().unwrap().termination_confirmed());
}

#[test]
fn restart_distinguishes_never_dispatched_and_unknown_execution() {
    let never = waiting().recover_after_restart().unwrap();
    assert_eq!(
        never.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Restarted
        })
    );
    let mut approved = waiting();
    approved.approve_verified(&binding()).unwrap();
    let recovered = approved.recover_after_restart().unwrap();
    assert_eq!(recovered.phase, ExecutionPhase::Settled);
    assert!(matches!(
        recovered.review,
        ScriptReviewState::WaitingReview { .. }
    ));
    assert!(!recovered.active());
    approved.begin_dispatch("attempt".into(), 1).unwrap();
    let mut unknown = approved.recover_after_restart().unwrap();
    assert_eq!(
        unknown.outcome,
        Some(ExecutionOutcome::Unknown {
            reason: ExecutionStopReason::Restarted
        })
    );
    assert_eq!(
        unknown.begin_dispatch("again".into(), 2),
        Err("execution_not_queued")
    );
}

#[test]
fn malformed_cross_states_fail_deserialization_and_validation() {
    let mut state = queued(false);
    state.outcome = Some(ExecutionOutcome::Completed { exit_code: 0 });
    state.phase = ExecutionPhase::Settled;
    assert_eq!(state.validate(), Err("invalid_execution_state"));
    assert!(
        serde_json::from_value::<ExecutionState>(serde_json::to_value(state).unwrap()).is_err()
    );
    let mut attempted = queued(false);
    attempted.begin_dispatch("attempt".into(), 1).unwrap();
    assert_eq!(
        attempted.settle(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Cancelled
        }),
        Err("invalid_execution_state")
    );
    assert_eq!(attempted.phase, ExecutionPhase::Dispatching);
    let mut review_in_terminal = waiting();
    review_in_terminal.terminal = true;
    assert_eq!(
        review_in_terminal.validate(),
        Err("invalid_execution_state")
    );
    let mut unknown = serde_json::to_value(queued(false)).unwrap();
    unknown["dispatch"]["executorPid"] = json!(1);
    assert!(serde_json::from_value::<ExecutionState>(unknown).is_err());
}
