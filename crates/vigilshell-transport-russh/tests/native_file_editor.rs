//! Opt-in disposable loopback SSH fixture; never reads application connection data.
use vigilshell_application_contracts::ports::*;
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;
#[tokio::test]
#[ignore = "requires explicitly selected disposable loopback SSH service"]
async fn external_editor_roundtrip_rejects_content_and_metadata_conflicts() {
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        name: "editor fixture".into(),
        auth: AuthKind::Password,
        environment: Environment::Test,
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
    };
    let key = RusshTransport.probe(input.clone()).await.unwrap();
    let client = RusshTransport
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
    let directory = format!("/tmp/vigil-editor-{}", new_id());
    client.mkdir(directory.clone()).await.unwrap();
    let root = std::env::temp_dir().join(format!("vigil-editor-test-{}", new_id()));
    tokio::fs::create_dir(&root).await.unwrap();
    let source = root.join("source");
    tokio::fs::write(&source, b"original\r\nbytes\0")
        .await
        .unwrap();
    let path = format!("{directory}/file.txt");
    let version = client
        .edit_upload(path.clone(), source.clone(), None)
        .await
        .unwrap();
    let copy = root.join("download");
    assert_eq!(
        client
            .edit_download(path.clone(), copy.clone())
            .await
            .unwrap(),
        version
    );
    assert_eq!(
        tokio::fs::read(&copy).await.unwrap(),
        b"original\r\nbytes\0"
    );
    tokio::fs::write(&source, b"new content").await.unwrap();
    let updated = client
        .edit_upload(path.clone(), source.clone(), Some(version.clone()))
        .await
        .unwrap();
    assert_eq!(updated.permissions, version.permissions);
    assert_eq!(updated.uid, version.uid);
    assert_eq!(updated.gid, version.gid);
    assert!(matches!(
        client
            .edit_upload(path.clone(), source.clone(), Some(version))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    let entry = client.stat(path.clone()).await.unwrap();
    client
        .permissions(path.clone(), 0o400, 0o277, entry)
        .await
        .unwrap();
    assert!(matches!(
        client
            .edit_upload(path.clone(), source.clone(), Some(updated))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    let current = client.edit_version(path.clone()).await.unwrap();
    let accepted = client
        .edit_upload(path.clone(), source.clone(), Some(current.clone()))
        .await
        .unwrap();
    assert_eq!(accepted.permissions, current.permissions);
    assert!(matches!(
        client.edit_upload(path.clone(), source.clone(), None).await,
        Err(WorkspaceError::FilesExists)
    ));
    client.remove(path, false).await.unwrap();
    client.remove(directory, true).await.unwrap();
    client.close().await.unwrap();
    tokio::fs::remove_dir_all(root).await.unwrap();
}
