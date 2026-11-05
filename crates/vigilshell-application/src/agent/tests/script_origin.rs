use super::*;
#[tokio::test]
async fn script_record_precedes_execution_and_retry_reuses_the_snapshot() {
    let f = Fixture::new().await;
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
    let task = f.start(operation.clone(), "script").await;
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Succeeded);
    let AgentReply::Activity { page } = f
        .service
        .activity("session".into(), Some(task.id.clone()), 0, true)
        .await
        .unwrap()
    else {
        panic!()
    };
    let snapshot = page
        .records
        .iter()
        .find(|r| matches!(r.body, ActivityBody::Script { .. }))
        .unwrap();
    let output = page
        .records
        .iter()
        .find(|r| matches!(r.body, ActivityBody::Output { .. }))
        .unwrap();
    assert!(snapshot.sequence < output.sequence);
    assert_eq!(f.start(operation, "script").await.id, task.id);
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service.shutdown().await;
}
#[tokio::test]
async fn source_badge_requires_owned_path_and_matching_bytes() {
    for changed in [false, true] {
        let f = Fixture::new().await;
        f.service.state.lock().unwrap().config.policies[0]
            .policy
            .execution
            .script_review = true;
        let source = f
            .start(
                RuntimeOperation::FilesWrite {
                    target: ScopedPath {
                        scope_id: ACCOUNT_SCOPE_ID.into(),
                        path: "srv/app/deploy.sh".into(),
                    },
                    data: STANDARD.encode(b"printf source"),
                    mode: FileWriteMode::Create,
                    expected_hash: None,
                },
                "source",
            )
            .await;
        f.finished(&source.id).await;
        if changed {
            *f.connection.snapshot_text.lock().unwrap() = Some("printf changed".into());
        }
        let task = f
            .start(
                RuntimeOperation::Script {
                    source: ScriptSource::RemoteFile {
                        target: ScopedPath {
                            scope_id: ACCOUNT_SCOPE_ID.into(),
                            path: "srv/app/deploy.sh".into(),
                        },
                        expected_hash: None,
                    },
                    interpreter: ScriptInterpreter::Sh,
                    arguments: vec![],
                    directory: ScopedPath {
                        scope_id: ACCOUNT_SCOPE_ID.into(),
                        path: "srv/app".into(),
                    },
                },
                "script",
            )
            .await;
        assert_eq!(task.phase, TaskPhase::WaitingReview);
        let AgentReply::Activity { page } = f
            .service
            .activity("session".into(), Some(task.id.clone()), 0, true)
            .await
            .unwrap()
        else {
            panic!()
        };
        let origin = page
            .records
            .iter()
            .find_map(|r| {
                if let ActivityBody::Script { origin, .. } = &r.body {
                    origin.as_ref()
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(origin.task_id, source.id);
        assert_eq!(origin.content_matches, !changed);
        f.service.shutdown().await;
    }
}
