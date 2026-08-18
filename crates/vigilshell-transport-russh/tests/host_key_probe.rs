use std::{
    borrow::Cow,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use russh::{
    Preferred,
    keys::{Algorithm, HashAlg, PrivateKey, ssh_key},
    server::{self, Auth, Handler},
};
use tokio::{io::AsyncReadExt, net::TcpListener, task::JoinHandle};
use vigilshell_core::{
    connection::{HostFingerprint, SshRoute},
    host_trust::HostKeyAlgorithm,
};
use vigilshell_transport_russh::{HostKeyProbeError, RusshHostKeyProbe};

#[derive(Default)]
struct AuthenticationCounters {
    none: AtomicUsize,
    password: AtomicUsize,
    keyboard_interactive: AtomicUsize,
    public_key_offered: AtomicUsize,
    public_key: AtomicUsize,
    certificate: AtomicUsize,
}

impl AuthenticationCounters {
    fn all_zero(&self) -> bool {
        self.none.load(Ordering::Relaxed) == 0
            && self.password.load(Ordering::Relaxed) == 0
            && self.keyboard_interactive.load(Ordering::Relaxed) == 0
            && self.public_key_offered.load(Ordering::Relaxed) == 0
            && self.public_key.load(Ordering::Relaxed) == 0
            && self.certificate.load(Ordering::Relaxed) == 0
    }
}

struct CountingServer {
    counters: Arc<AuthenticationCounters>,
}

impl Handler for CountingServer {
    type Error = russh::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        self.counters.none.fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        self.counters.password.fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        self.counters
            .keyboard_interactive
            .fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        _public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.counters
            .public_key_offered
            .fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.counters.public_key.fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }

    async fn auth_openssh_certificate(
        &mut self,
        _user: &str,
        _certificate: &ssh_key::Certificate,
    ) -> Result<Auth, Self::Error> {
        self.counters.certificate.fetch_add(1, Ordering::Relaxed);
        Ok(Auth::reject())
    }
}

#[tokio::test]
async fn supported_probe_returns_canonical_fingerprint_without_authentication() {
    let key = generated_key(Algorithm::Ed25519);
    let expected_fingerprint =
        HostFingerprint::parse(key.public_key().fingerprint(HashAlg::Sha256).to_string())
            .expect("generated key should have a canonical fingerprint");
    let counters = Arc::new(AuthenticationCounters::default());
    let (address, server) = spawn_ssh_server(key, None, Arc::clone(&counters)).await;
    let route = loopback_route(address);

    let observed = RusshHostKeyProbe::new(Duration::from_secs(3))
        .expect("timeout is nonzero")
        .probe(&route)
        .await
        .expect("supported host key should be observed");

    assert_eq!(observed.algorithm(), HostKeyAlgorithm::Ed25519);
    assert_eq!(observed.fingerprint(), &expected_fingerprint);
    expect_server_settlement(server).await;
    assert!(counters.all_zero());
}

#[tokio::test]
async fn legacy_rsa_host_key_policy_is_rejected_without_authentication() {
    let key = generated_key(Algorithm::Rsa { hash: None });
    let server_preferred = Preferred {
        key: Cow::Owned(vec![Algorithm::Rsa { hash: None }]),
        ..Preferred::default()
    };
    let counters = Arc::new(AuthenticationCounters::default());
    let (address, server) =
        spawn_ssh_server(key, Some(server_preferred), Arc::clone(&counters)).await;
    let route = loopback_route(address);

    let result = RusshHostKeyProbe::new(Duration::from_secs(3))
        .expect("timeout is nonzero")
        .probe(&route)
        .await;

    assert_eq!(result, Err(HostKeyProbeError::UnsupportedHostKey));
    expect_server_settlement(server).await;
    assert!(counters.all_zero());
}

#[tokio::test]
async fn stalled_handshake_times_out_and_does_not_authenticate() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let counters = Arc::new(AuthenticationCounters::default());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        let mut buffer = [0_u8; 128];
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    let route = loopback_route(address);

    let result = RusshHostKeyProbe::new(Duration::from_millis(100))
        .expect("timeout is nonzero")
        .probe(&route)
        .await;

    assert_eq!(result, Err(HostKeyProbeError::Timeout));
    expect_server_settlement(server).await;
    assert!(counters.all_zero());
}

#[tokio::test]
async fn dropped_probe_destroys_the_connection_attempt() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let (ready_sender, ready_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        let mut buffer = [0_u8; 128];
        let first_read = socket
            .read(&mut buffer)
            .await
            .expect("probe identification should arrive");
        assert!(first_read > 0);
        let _ = ready_sender.send(());
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    let route = loopback_route(address);
    let probe = RusshHostKeyProbe::new(Duration::from_secs(3)).expect("timeout is nonzero");
    let probe_task = tokio::spawn(async move { probe.probe(&route).await });
    tokio::time::timeout(Duration::from_secs(1), ready_receiver)
        .await
        .expect("probe should start the loopback handshake")
        .expect("loopback readiness sender should remain available");

    probe_task.abort();
    assert!(
        probe_task
            .await
            .expect_err("aborted probe task must not complete")
            .is_cancelled()
    );
    expect_server_settlement(server).await;
}

fn generated_key(algorithm: Algorithm) -> PrivateKey {
    let mut random = russh::keys::key::safe_rng();
    PrivateKey::random(&mut random, algorithm).expect("runtime key generation should succeed")
}

async fn spawn_ssh_server(
    key: PrivateKey,
    preferred: Option<Preferred>,
    counters: Arc<AuthenticationCounters>,
) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let mut config = server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: Some(Duration::from_secs(3)),
        ..server::Config::default()
    };
    if let Some(preferred) = preferred {
        config.preferred = preferred;
    }
    let config = Arc::new(config);
    let server = tokio::spawn(async move {
        let (socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        if let Ok(session) = server::run_stream(config, socket, CountingServer { counters }).await {
            let _ = session.await;
        }
    });
    (address, server)
}

fn loopback_route(address: SocketAddr) -> SshRoute {
    SshRoute::new(address.ip().to_string(), address.port(), "unused")
        .expect("runtime loopback route should be valid")
}

async fn expect_server_settlement(mut server: JoinHandle<()>) {
    match tokio::time::timeout(Duration::from_secs(2), &mut server).await {
        Ok(result) => result.expect("loopback server task should not panic"),
        Err(_) => {
            server.abort();
            let _ = server.await;
            panic!("probe must terminate the complete connection attempt");
        }
    }
}
