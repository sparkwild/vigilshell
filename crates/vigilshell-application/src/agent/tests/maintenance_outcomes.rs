use super::*;
#[tokio::test]
async fn native_partial_unknown_and_nonzero_results_do_not_become_success() {
    for (effects, outcome, error, phase) in [
        (
            FileCommitState::Unknown,
            ExecutionOutcome::Unknown {
                reason: ExecutionStopReason::Disconnected,
            },
            "native_result_unknown",
            TaskPhase::InterruptedUnknown,
        ),
        (
            FileCommitState::Partial,
            ExecutionOutcome::Completed { exit_code: 1 },
            "service_reload_failed",
            TaskPhase::Failed,
        ),
        (
            FileCommitState::Unchanged,
            ExecutionOutcome::Completed { exit_code: 1 },
            "native_service_failed",
            TaskPhase::Failed,
        ),
    ] {
        let f = Fixture::new().await;
        *f.connection.native_result.lock().unwrap() = Some(NativeExecutionReport {
            outcome: Some(outcome),
            effects,
            error: Some(error.into()),
            json: None,
            recovery_paths: vec![],
        });
        let task = f.start(server_query(), "result").await;
        let result = f.finished(&task.id).await;
        assert_eq!(result.phase, phase);
        assert_eq!(result.error.as_deref(), Some(error));
        assert_eq!(f.start(server_query(), "result").await.id, task.id);
        assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
        f.service.shutdown().await;
    }
}
#[tokio::test]
async fn native_client_cancellation_does_not_report_business_stopped() {
    let f = Fixture::new().await;
    f.connection.hold.store(true, Ordering::Release);
    let task = f
        .start(
            RuntimeOperation::ServicesControl {
                name: "demo.service".into(),
                action: ServiceAction::Start,
            },
            "service",
        )
        .await;
    f.running(&task.id).await;
    f.service
        .cancel_task(&task.id, ActivityActor::User)
        .await
        .unwrap();
    assert_eq!(
        f.finished(&task.id).await.phase,
        TaskPhase::InterruptedUnknown
    );
    f.service.shutdown().await;
}
