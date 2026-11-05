use vigilshell_application_contracts::ports::SshTransport;
#[path = "../../../tests/fixtures/ssh_server.rs"]
mod fixture;
#[path = "../../../tests/fixtures/proxy_server.rs"]
mod proxy_fixture;
use fixture::{Fixture, unused_port};
use proxy_fixture::proxy;
use std::{sync::atomic::Ordering, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use vigilshell_application_contracts::ports::{ProxyRoute, SecretMaterial, TransportEvent};
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;
use zeroize::Zeroizing;
fn input(port: u16) -> ConnectionInput {
    ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
        environment: Environment::None,
        name: "Fixture".into(),
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
        bytes: Zeroizing::new(b"sidebar-fixture".to_vec()),
        label: String::new(),
    }
}
fn route(port: u16, kind: &str, password: Option<&str>) -> ProxyRoute {
    ProxyRoute {
        proxy: ProxyRecord {
            id: new_id(),
            name: "Fixture proxy".into(),
            kind: kind.into(),
            host: "127.0.0.1".into(),
            port,
            username: "proxy-user".into(),
            version: 1,
            credential: None,
        },
        secret: password.map(|p| SecretMaterial {
            diagnostic: None,
            kind: AuthKind::Password,
            bytes: Zeroizing::new(p.as_bytes().to_vec()),
            label: String::new(),
        }),
    }
}
async fn echo() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (
        port,
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        }),
    )
}
async fn roundtrip(mut stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin) {
    let value = b"forwarded unicode: \xe4\xb8\xad\xe6\x96\x87";
    stream.write_all(value).await.unwrap();
    let mut got = vec![0; value.len()];
    tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut got))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, value);
}
#[tokio::test]
async fn proxies_carry_real_ssh_and_reject_bad_password_and_changed_key() {
    let fixture = Fixture::start().await;
    for kind in ["socks5", "http"] {
        for auth in [false, true] {
            let (port, task) = proxy(kind, auth).await;
            let key = RusshTransport
                .probe_via(
                    input(fixture.port),
                    Some(route(port, kind, auth.then_some("proxy-pass"))),
                )
                .await
                .unwrap();
            let mut session = RusshTransport
                .authenticate_via(
                    input(fixture.port),
                    key.clone(),
                    secret(),
                    Some(route(port, kind, auth.then_some("proxy-pass"))),
                )
                .await
                .unwrap();
            session.open_terminal(80, 24).await.unwrap();
            session.write(b"echo").await.unwrap();
            let event = tokio::time::timeout(Duration::from_secs(3), session.read())
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(event,TransportEvent::Data(b) if b==b"echo"));
            session.close().await.unwrap();
            if auth {
                assert!(matches!(
                    RusshTransport
                        .probe_via(input(fixture.port), Some(route(port, kind, Some("bad"))))
                        .await,
                    Err(WorkspaceError::ProxyAuthenticationFailed)
                ));
            }
            let mut wrong = key;
            wrong.public_key.push('x');
            assert!(matches!(
                RusshTransport
                    .authenticate_via(
                        input(fixture.port),
                        wrong,
                        secret(),
                        Some(route(port, kind, auth.then_some("proxy-pass")))
                    )
                    .await,
                Err(WorkspaceError::TrustChanged)
            ));
            task.abort();
        }
    }
}
fn rule(kind: &str, port: u16, target: u16) -> TunnelRule {
    TunnelRule {
        id: new_id(),
        name: kind.into(),
        kind: kind.into(),
        bind_host: "127.0.0.1".into(),
        listen_port: port,
        target_host: (kind != "socks5").then(|| "127.0.0.1".into()),
        target_port: (kind != "socks5").then_some(target),
        enabled: true,
    }
}
#[tokio::test]
async fn all_three_forwarders_transfer_retry_and_release_ports() {
    let fixture = Fixture::start().await;
    let key = RusshTransport.probe(input(fixture.port)).await.unwrap();
    let forward = RusshTransport
        .forwarding(input(fixture.port), key, secret(), None)
        .await
        .unwrap();
    let (target, echo) = echo().await;
    let mut ports = vec![];
    for kind in ["local", "remote", "socks5"] {
        let port = unused_port().await;
        ports.push(port);
        let rule = rule(kind, port, target);
        assert_eq!(forward.start(rule.clone()).await.unwrap().phase, "running");
        if kind == "socks5" {
            roundtrip(
                tokio_socks::tcp::Socks5Stream::connect(("127.0.0.1", port), ("127.0.0.1", target))
                    .await
                    .unwrap(),
            )
            .await;
        } else {
            roundtrip(TcpStream::connect(("127.0.0.1", port)).await.unwrap()).await;
        }
        assert_eq!(forward.start(rule).await.unwrap().phase, "running");
    }
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let retry = rule("local", occupied.local_addr().unwrap().port(), target);
    assert_eq!(forward.start(retry.clone()).await.unwrap().phase, "failed");
    drop(occupied);
    assert_eq!(forward.start(retry.clone()).await.unwrap().phase, "running");
    ports.push(retry.listen_port);
    fixture.shared.deny_remote.store(true, Ordering::SeqCst);
    let failed = rule("remote", unused_port().await, target);
    assert_eq!(
        forward.start(failed).await.unwrap().error.as_deref(),
        Some("tunnel_remote_refused")
    );
    forward.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if ports
                .iter()
                .all(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    echo.abort();
}
#[tokio::test]
async fn cancelling_proxy_handshake_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let _ = RusshTransport
            .probe_via(input(22), Some(route(port, "http", None)))
            .await;
    });
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut b = [0; 1024];
    assert!(socket.read(&mut b).await.unwrap() > 0);
    task.abort();
    let _ = task.await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), socket.read(&mut b))
            .await
            .unwrap()
            .unwrap(),
        0
    );
}
