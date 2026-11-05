#[path = "../../../tests/fixtures/ssh_server.rs"]
mod fixture;
use std::sync::atomic::Ordering;
use vigilshell_application_contracts::ports::{SecretMaterial, SshTransport};
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;

fn profile(port: u16) -> ConnectionInput {
    ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
        environment: Environment::Test,
        name: "MCP fixture".into(),
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        auth: AuthKind::Password,
    }
}
fn secret() -> SecretMaterial {
    SecretMaterial {
        diagnostic: None,
        kind: AuthKind::Password,
        bytes: b"sidebar-fixture".to_vec().into(),
        label: String::new(),
    }
}

#[tokio::test]
async fn missing_server_is_reported_without_starting_an_agent_session() {
    let port = fixture::unused_port().await;
    assert!(RusshTransport.probe(profile(port)).await.is_err());
}

#[tokio::test]
async fn checked_exec_preserves_early_output_eof_and_exit_without_runtime_preparation() {
    use vigilshell_application_contracts::ports::AgentProcessEvent;
    use vigilshell_domain::agent::*;
    let f = fixture::Fixture::start().await;
    f.shared.allow_exec.store(true, Ordering::SeqCst);
    let p = profile(f.port);
    let key = RusshTransport.probe(p.clone()).await.unwrap();
    let connection = RusshTransport.agent(p, key, secret(), None).await.unwrap();
    let request:ExecutionRequest=serde_json::from_value(serde_json::json!({"identity":{"taskId":"task","entryId":"entry","entryGeneration":1,"connectionId":"connection","connectionRevision":1,"sessionId":"session","policyRevision":1},"requestKey":"request","operation":{"kind":"command","command":"pwd","directory":{"scopeId":"__account__","path":""}}})).unwrap();
    let policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: true,
        execution: ExecutionPolicy {
            level: OperationLevel::ReadOnly,
            ..Default::default()
        },
    };
    let mut process = connection
        .execute_command(check_command(&request, &policy).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        process.input(&vec![0; 65537]).await,
        Err(WorkspaceError::InvalidInput)
    ));
    assert!(matches!(
        process.resize(0, 24).await,
        Err(WorkspaceError::InvalidInput)
    ));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut eof = false;
    let mut exit = None;
    loop {
        match process.read().await.unwrap() {
            AgentProcessEvent::Stdout(b) => stdout.extend(b),
            AgentProcessEvent::Stderr(b) => stderr.extend(b),
            AgentProcessEvent::Exit(code) => exit = Some(code),
            AgentProcessEvent::Eof => eof = true,
            AgentProcessEvent::Closed => break,
            AgentProcessEvent::ExitSignal { .. } => panic!("unexpected signal"),
        }
    }
    assert_eq!(stdout, "fixture-中文\n".as_bytes());
    assert_eq!(stderr, b"fixture-stderr\n");
    assert_eq!(exit, Some(7));
    assert!(eof);
    assert_eq!(f.shared.executed.lock().unwrap().len(), 1);
    process.stop().await.unwrap();
    connection.close().await.unwrap();
}
