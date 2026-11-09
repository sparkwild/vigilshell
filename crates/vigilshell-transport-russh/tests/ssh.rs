use russh::server::{self, Server as _};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use vigilshell_application_contracts::ports::SshTransport;

use vigilshell_application_contracts::ports::SecretMaterial;
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;
#[derive(Clone)]
struct Fixture(Arc<AtomicUsize>);
impl server::Server for Fixture {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}
impl server::Handler for Fixture {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(if user == "fixture" && password == "synthetic-password" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
}
struct Diagnostic(std::sync::Mutex<Vec<(&'static str, bool)>>);
impl vigilshell_application_contracts::ports::SecretDiagnostic for Diagnostic {
    fn id(&self) -> &str {
        "synthetic-diagnostic"
    }
    fn check(&self, checkpoint: &'static str, bytes: &[u8]) -> bool {
        let same = bytes == b"synthetic-password";
        self.0.lock().unwrap().push((checkpoint, same));
        same
    }
    fn record(&self, _: &'static str, _: Option<bool>) {}
}
#[tokio::test]
async fn verifies_host_before_password_and_distinguishes_authentication_from_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("host");
    assert!(
        std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let key = russh::keys::load_secret_key(&path, None).unwrap();
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: std::time::Duration::from_millis(1),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut fixture = Fixture(calls.clone());
    let task = tokio::spawn(async move { fixture.run_on_socket(config, &listener).await.unwrap() });
    let input = ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: Vec::new(),
        environment: Environment::None,
        name: "Fixture".into(),
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        auth: AuthKind::Password,
    };
    let transport = RusshTransport;
    let key = transport.probe(input.clone()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let secret = |value: &str| SecretMaterial {
        diagnostic: None,
        kind: AuthKind::Password,
        bytes: value.as_bytes().to_vec().into(),
        label: String::new(),
    };
    let mut changed = key.clone();
    changed.fingerprint = "different".into();
    assert!(matches!(
        transport
            .authenticate(input.clone(), changed, secret("synthetic-password"))
            .await,
        Err(WorkspaceError::TrustChanged)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        transport
            .authenticate(input.clone(), key.clone(), secret("wrong"))
            .await,
        Err(WorkspaceError::AuthenticationFailed)
    ));
    let diagnostic = Arc::new(Diagnostic(std::sync::Mutex::new(Vec::new())));
    let mut captured = secret("synthetic-password");
    captured.diagnostic = Some(diagnostic.clone());
    let mut session = transport.authenticate(input, key, captured).await.unwrap();
    assert_eq!(
        *diagnostic.0.lock().unwrap(),
        vec![("ssh_material", true), ("ssh_password_argument", true)]
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "diagnostics must not add authentication attempts"
    );
    assert_eq!(
        session.open_terminal(80, 24).await,
        Err(WorkspaceError::ChannelRejected)
    );
    session.close().await.unwrap();
    task.abort();
}
