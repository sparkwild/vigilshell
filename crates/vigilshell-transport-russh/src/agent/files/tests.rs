use super::*;
use vigilshell_application_contracts::ports::{SecretMaterial, SshTransport};
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn missing_atomic_rename_capability_keeps_original() {
    let port = std::env::var("VIGILSHELL_TEST_SFTP_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
        environment: Environment::Test,
        name: "capability fixture".into(),
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        auth: AuthKind::Password,
    };
    let key = crate::RusshTransport.probe(input.clone()).await.unwrap();
    let (handle, guard) = authenticated(
        input,
        key,
        SecretMaterial {
            diagnostic: None,
            kind: AuthKind::Password,
            bytes: b"runtime-fixture-only".to_vec().into(),
            label: String::new(),
        },
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let mut session = crate::sftp::open(&handle).await.unwrap();
    session.1 = false;
    let connection = Connection {
        encoding: encoding_rs::GBK,
        handle,
        _guard: guard,
        previous_cpu: Mutex::default(),
        sftp: tokio::sync::OnceCell::new(),
        listings: Mutex::default(),
    };
    assert!(connection.sftp.set(session).is_ok());
    let policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: false,
        execution: ExecutionPolicy {
            level: OperationLevel::Operate,
            ..Default::default()
        },
    };
    let path = ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: format!("srv/fixture/中文-atomic-{}", new_id()),
    };
    let request = |operation| ExecutionRequest {
        identity: ExecutionIdentity {
            task_id: new_id(),
            entry_id: "entry".into(),
            entry_generation: 1,
            connection_id: "connection".into(),
            connection_revision: 1,
            session_id: "session".into(),
            policy_revision: 1,
        },
        request_key: new_id(),
        timeout_seconds: Some(10),
        operation,
    };
    let create = check_file_operation(
        &request(RuntimeOperation::FilesWrite {
            target: path.clone(),
            data: STANDARD.encode("before"),
            mode: FileWriteMode::Create,
            expected_hash: None,
        }),
        &policy,
    )
    .unwrap();
    assert!(
        execute(
            &connection,
            create,
            FileTransferHandles::default(),
            FileExecutionControl::default()
        )
        .await
        .unwrap()
        .error
        .is_none()
    );
    let replace = check_file_operation(
        &request(RuntimeOperation::FilesWrite {
            target: path.clone(),
            data: STANDARD.encode("after"),
            mode: FileWriteMode::Replace,
            expected_hash: None,
        }),
        &policy,
    )
    .unwrap();
    let report = execute(
        &connection,
        replace,
        FileTransferHandles::default(),
        FileExecutionControl::default(),
    )
    .await
    .unwrap();
    assert_eq!(report.commit, FileCommitState::Unchanged);
    assert_eq!(report.error.as_deref(), Some("file_capability_unavailable"));
    assert!(report.recovery_paths.is_empty());
    let read = check_file_operation(
        &request(RuntimeOperation::FilesRead {
            target: path.clone(),
            offset: 0,
            length: 64,
        }),
        &policy,
    )
    .unwrap();
    let value = execute(
        &connection,
        read,
        FileTransferHandles::default(),
        FileExecutionControl::default(),
    )
    .await
    .unwrap();
    let value: Value = serde_json::from_str(value.json.as_ref().unwrap()).unwrap();
    assert_eq!(
        STANDARD.decode(value["data"].as_str().unwrap()).unwrap(),
        b"before"
    );
    let remove = check_file_operation(
        &request(RuntimeOperation::FilesRemove {
            target: path,
            recursive: false,
        }),
        &policy,
    )
    .unwrap();
    assert!(
        execute(
            &connection,
            remove,
            FileTransferHandles::default(),
            FileExecutionControl::default()
        )
        .await
        .unwrap()
        .error
        .is_none()
    );
    connection.close().await.unwrap();
}
