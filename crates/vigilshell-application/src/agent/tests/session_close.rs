use super::*;

#[tokio::test]
async fn closing_a_failed_connection_preserves_its_recording_and_new_admission() {
    let f = Fixture::new().await;
    let context = f.service.context(&f.entry.id, "session").unwrap();
    f.connection.closed.store(true, Ordering::Release);
    f.service.begin_connect(context.clone());
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if context.view.lock().unwrap().phase == ContextPhase::Failed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    for key in ["close", "close", "another-close"] {
        f.service
            .invoke(
                f.entry.clone(),
                AgentAction::SessionClose {
                    session_id: "session".into(),
                    request_key: key.into(),
                },
            )
            .await
            .unwrap();
    }
    assert!(!f.service.state.lock().unwrap().recording_blocked);
    let AgentReply::Activity { page } = f
        .service
        .activity("session".into(), None, 0, true)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(!page.incomplete);
    assert!(
        !page
            .records
            .iter()
            .any(|r| matches!(&r.body, ActivityBody::Control {action} if action == "end_session"))
    );
    f.service
        .open_context(f.entry.clone(), &f.connection_id, "next-session", None)
        .await
        .unwrap();
    f.service.shutdown().await;
}

#[tokio::test]
async fn concurrent_closes_record_one_end_request_and_leave_history_complete() {
    let f = Fixture::new().await;
    let close = |key: &str| {
        f.service.invoke(
            f.entry.clone(),
            AgentAction::SessionClose {
                session_id: "session".into(),
                request_key: key.into(),
            },
        )
    };
    let (a, b) = tokio::join!(close("first"), close("second"));
    a.unwrap();
    b.unwrap();
    assert!(!f.service.state.lock().unwrap().recording_blocked);
    let AgentReply::Activity { page } = f
        .service
        .activity("session".into(), None, 0, true)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(!page.incomplete);
    assert_eq!(
        page.records
            .iter()
            .filter(
                |r| matches!(&r.body, ActivityBody::Control {action} if action == "end_session")
            )
            .count(),
        1
    );
    f.service.shutdown().await;
}
