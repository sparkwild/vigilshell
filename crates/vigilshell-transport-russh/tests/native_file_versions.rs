//! Explicit loopback VM only: a maintenance version must reach the final SFTP delete boundary.
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use vigilshell_application_contracts::ports::*;
use vigilshell_domain::{agent::*, workspace::*};
use vigilshell_transport_russh::RusshTransport;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[tokio::test]
#[ignore = "requires explicitly selected disposable loopback SSH VM"]
async fn stale_maintenance_delete_cannot_remove_a_changed_definition() {
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        name: "native file version fixture".into(),
        auth: AuthKind::Password,
        environment: Environment::Test,
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
    };
    let key = RusshTransport.probe(input.clone()).await.unwrap();
    let connection = RusshTransport
        .agent(
            input,
            key,
            SecretMaterial {
                diagnostic: None,
                kind: AuthKind::Password,
                bytes: b"native-fixture-only".to_vec().into(),
                label: String::new(),
            },
            None,
        )
        .await
        .unwrap();
    let id = new_id();
    let name = format!("vigil-version-{id}.service");
    let target = ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: format!("etc/systemd/system/{name}"),
    };
    let current = b"[Service]\nExecStart=/usr/bin/true\nEnvironment=VERSION=new\n";
    let policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: true,
        execution: ExecutionPolicy {
            level: OperationLevel::Operate,
            ..Default::default()
        },
    };
    let request = |operation: RuntimeOperation| ExecutionRequest {
        identity: ExecutionIdentity {
            task_id: id.clone(),
            entry_id: "entry".into(),
            entry_generation: 1,
            connection_id: "connection".into(),
            connection_revision: 1,
            session_id: "session".into(),
            policy_revision: 1,
        },
        request_key: id.clone(),
        timeout_seconds: None,
        operation,
    };
    let definition:NativeServiceDefinition=serde_json::from_value(serde_json::json!({"target":{"manager":"system","name":name},"content":{"kind":"unit","text":std::str::from_utf8(current).unwrap()},"expectedHash":null})).unwrap();
    let permit = check_native_operation(
        &request(RuntimeOperation::ServicesRegister { definition }),
        &policy,
    )
    .unwrap();
    let root = format!("/root/.local/share/vigilshell/native-staging/{id}");
    let write = native_file_operation(
        &permit,
        RuntimeOperation::FilesWrite {
            target: target.clone(),
            data: STANDARD.encode(current),
            mode: FileWriteMode::Create,
            expected_hash: None,
        },
        &root,
    )
    .unwrap();
    let result = connection
        .execute_files(
            write,
            FileTransferHandles::default(),
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert!(result.error.is_none(), "{result:?}");
    for (expected, success) in [(hash(b"old version"), false), (hash(current), true)] {
        let permit = check_native_operation(
            &request(RuntimeOperation::ServicesUnregister {
                name: name.clone(),
                drop_in: None,
                expected_hash: expected.clone(),
            }),
            &policy,
        )
        .unwrap();
        let remove = native_file_operation(
            &permit,
            RuntimeOperation::FilesRemove {
                target: target.clone(),
                recursive: false,
            },
            &root,
        )
        .unwrap();
        assert_eq!(remove.removal_hash(), Some(expected.as_str()));
        let result = connection
            .execute_files(
                remove,
                FileTransferHandles::default(),
                FileExecutionControl::default(),
            )
            .await
            .unwrap();
        if success {
            assert!(result.error.is_none(), "{result:?}");
        } else {
            assert_eq!(result.error.as_deref(), Some("file_changed"));
            assert_eq!(result.commit, FileCommitState::Unchanged);
            let read = native_file_operation(
                &permit,
                RuntimeOperation::FilesRead {
                    target: target.clone(),
                    offset: 0,
                    length: 65536,
                },
                &root,
            )
            .unwrap();
            let result = connection
                .execute_files(
                    read,
                    FileTransferHandles::default(),
                    FileExecutionControl::default(),
                )
                .await
                .unwrap();
            let json: serde_json::Value =
                serde_json::from_str(result.json.as_deref().unwrap()).unwrap();
            assert_eq!(
                STANDARD.decode(json["data"].as_str().unwrap()).unwrap(),
                current
            );
        }
    }
    connection.close().await.unwrap();
}
