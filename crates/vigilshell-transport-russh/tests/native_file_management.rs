//! Runs only against an explicitly selected disposable loopback SSH fixture.
use vigilshell_application_contracts::ports::*;
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;

#[tokio::test]
#[ignore = "requires disposable loopback SSH fixture"]
async fn native_stat_exclusive_create_permissions_and_nonempty_delete() {
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        name: "file management fixture".into(),
        auth: AuthKind::Password,
        environment: Environment::Test,
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
    };
    let key = RusshTransport.probe(input.clone()).await.unwrap();
    let tools = RusshTransport
        .tools(
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
    let root = format!("/tmp/vigil-file-management-{}", new_id());
    tools.mkdir(root.clone()).await.unwrap();
    let path = format!("{root}/空 格'\nfile");
    tools.create_file(path.clone()).await.unwrap();
    let before = tools.stat(path.clone()).await.unwrap();
    assert_eq!(before.kind, "file");
    assert_eq!(before.size, Some(0));
    assert!(matches!(
        tools.create_file(path.clone()).await,
        Err(WorkspaceError::FilesExists)
    ));
    tools
        .permissions(path.clone(), 0o600, 0o177, before.clone())
        .await
        .unwrap();
    let after = tools.stat(path.clone()).await.unwrap();
    assert_eq!(after.permissions.unwrap() & 0o777, 0o600);
    assert_eq!(after.uid, before.uid);
    assert_eq!(after.gid, before.gid);
    assert!(matches!(
        tools.permissions(path.clone(), 0, 0, before).await,
        Err(WorkspaceError::Conflict)
    ));
    assert!(matches!(
        tools.remove(root.clone(), true).await,
        Err(WorkspaceError::FilesNotEmpty)
    ));
    tools.remove(path.clone(), false).await.unwrap();
    assert!(matches!(
        tools.stat(path).await,
        Err(WorkspaceError::NotFound)
    ));
    tools.remove(root, true).await.unwrap();
    tools.close().await.unwrap();
}
