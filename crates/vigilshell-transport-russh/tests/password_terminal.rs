use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use russh::{
    Channel, ChannelId, Pty,
    keys::{Algorithm, HashAlg, PrivateKey},
    server::{self, Auth, ChannelOpenHandle, Handler, Msg, Session},
};
use tokio::{io::AsyncReadExt, net::TcpListener, task::JoinHandle};
use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, HostFingerprint, ManagedServerId, SshRoute,
        StoredHostTrust,
    },
    human_terminal::{
        ConnectionProfileRepository, ConnectionRepositoryError, CredentialBroker,
        CredentialBrokerError, HostTrustRepository, HostTrustRepositoryError, HumanCredentialLease,
        HumanTerminalAdmissionService, OpenHumanTerminalRequest, TerminalSize,
    },
};
use vigilshell_transport_russh::{
    RusshPasswordTerminalTransport, RusshTerminalLifecycle, RusshTerminalOpenError,
    RusshTerminalOutput, RusshTerminalOutputError, RusshTerminalOutputSink,
    RusshTerminalSettlement, RusshTerminalTimeouts,
};

#[derive(Clone, Copy)]
enum ReplyBehavior {
    Accept,
    Reject,
    Silent,
}

#[derive(Clone, Copy)]
struct ServerBehavior {
    auth: ReplyBehavior,
    pty: ReplyBehavior,
    shell: ReplyBehavior,
}

impl ServerBehavior {
    const SUCCESS: Self = Self {
        auth: ReplyBehavior::Accept,
        pty: ReplyBehavior::Accept,
        shell: ReplyBehavior::Accept,
    };
}

#[derive(Default)]
struct ServerCounters {
    authentication: AtomicUsize,
    pty: AtomicUsize,
    shell: AtomicUsize,
}

struct OwnerLoopbackServer {
    password: Arc<str>,
    input_sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    resize_sender: tokio::sync::mpsc::UnboundedSender<(u32, u32)>,
    output_after_shell: bool,
}

impl Handler for OwnerLoopbackServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == self.password.as_ref() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        if self.output_after_shell {
            session.data(channel, vec![0_u8, 0xff, b'A'])?;
            session.extended_data(channel, 1, vec![0xfe, b'B'])?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = self.input_sender.send(data.to_vec());
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = self.resize_sender.send((col_width, row_height));
        Ok(())
    }
}

enum CapturedOutput {
    Data(Vec<u8>),
    Lifecycle(RusshTerminalLifecycle),
}

struct ChannelOutputSink(tokio::sync::mpsc::UnboundedSender<CapturedOutput>);

impl RusshTerminalOutputSink for ChannelOutputSink {
    fn send(&mut self, output: RusshTerminalOutput) -> Result<(), RusshTerminalOutputError> {
        let output = match output {
            RusshTerminalOutput::Data(data) => CapturedOutput::Data(data.into_bytes()),
            RusshTerminalOutput::Lifecycle(lifecycle) => CapturedOutput::Lifecycle(lifecycle),
        };
        self.0.send(output).map_err(|_| RusshTerminalOutputError)
    }
}

struct RejectingOutputSink;

impl RusshTerminalOutputSink for RejectingOutputSink {
    fn send(&mut self, _output: RusshTerminalOutput) -> Result<(), RusshTerminalOutputError> {
        Err(RusshTerminalOutputError)
    }
}

struct LoopbackServer {
    behavior: ServerBehavior,
    password: Arc<str>,
    counters: Arc<ServerCounters>,
}

impl Handler for LoopbackServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.counters.authentication.fetch_add(1, Ordering::Relaxed);
        match self.behavior.auth {
            ReplyBehavior::Accept if password == self.password.as_ref() => Ok(Auth::Accept),
            ReplyBehavior::Accept | ReplyBehavior::Reject | ReplyBehavior::Silent => {
                Ok(Auth::reject())
            }
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.counters.pty.fetch_add(1, Ordering::Relaxed);
        match self.behavior.pty {
            ReplyBehavior::Accept => session.channel_success(channel)?,
            ReplyBehavior::Reject => session.channel_failure(channel)?,
            ReplyBehavior::Silent => {}
        }
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.counters.shell.fetch_add(1, Ordering::Relaxed);
        match self.behavior.shell {
            ReplyBehavior::Accept => session.channel_success(channel)?,
            ReplyBehavior::Reject => session.channel_failure(channel)?,
            ReplyBehavior::Silent => {}
        }
        Ok(())
    }
}

#[tokio::test]
async fn exact_host_identity_authenticates_once_and_settles_pty_and_shell() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let counters = Arc::new(ServerCounters::default());
    let (address, server) = spawn_server(
        key,
        ServerBehavior::SUCCESS,
        Arc::clone(&password),
        Arc::clone(&counters),
    )
    .await;

    let established = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await
        .expect("matching identity and accepted setup should establish");

    assert_eq!(counters.authentication.load(Ordering::Relaxed), 1);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 1);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 1);
    drop(established);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn host_key_mismatch_terminates_before_authentication() {
    let password = runtime_password();
    let key = generated_key();
    let counters = Arc::new(ServerCounters::default());
    let (address, server) = spawn_server(
        key,
        ServerBehavior::SUCCESS,
        Arc::clone(&password),
        Arc::clone(&counters),
    )
    .await;

    let result = transport(Duration::from_secs(3))
        .establish(authorized_request(
            address,
            fingerprint(&generated_key()),
            password.as_bytes(),
        ))
        .await;

    assert!(matches!(
        result,
        Err(RusshTerminalOpenError::HostKeyMismatch)
    ));
    assert_eq!(counters.authentication.load(Ordering::Relaxed), 0);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 0);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 0);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn authentication_rejection_does_not_open_a_channel() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let counters = Arc::new(ServerCounters::default());
    let behavior = ServerBehavior {
        auth: ReplyBehavior::Reject,
        ..ServerBehavior::SUCCESS
    };
    let (address, server) =
        spawn_server(key, behavior, Arc::clone(&password), Arc::clone(&counters)).await;

    let result = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await;

    assert!(matches!(result, Err(RusshTerminalOpenError::AuthRejected)));
    assert_eq!(counters.authentication.load(Ordering::Relaxed), 1);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 0);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 0);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn pty_rejection_does_not_request_a_shell() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let counters = Arc::new(ServerCounters::default());
    let behavior = ServerBehavior {
        pty: ReplyBehavior::Reject,
        ..ServerBehavior::SUCCESS
    };
    let (address, server) =
        spawn_server(key, behavior, Arc::clone(&password), Arc::clone(&counters)).await;

    let result = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await;

    assert!(matches!(result, Err(RusshTerminalOpenError::PtyRejected)));
    assert_eq!(counters.authentication.load(Ordering::Relaxed), 1);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 1);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 0);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn shell_rejection_fails_after_pty_success() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let counters = Arc::new(ServerCounters::default());
    let behavior = ServerBehavior {
        shell: ReplyBehavior::Reject,
        ..ServerBehavior::SUCCESS
    };
    let (address, server) =
        spawn_server(key, behavior, Arc::clone(&password), Arc::clone(&counters)).await;

    let result = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await;

    assert!(matches!(result, Err(RusshTerminalOpenError::ShellRejected)));
    assert_eq!(counters.authentication.load(Ordering::Relaxed), 1);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 1);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 1);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn missing_pty_reply_times_out_and_tears_down_the_attempt() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let counters = Arc::new(ServerCounters::default());
    let behavior = ServerBehavior {
        pty: ReplyBehavior::Silent,
        ..ServerBehavior::SUCCESS
    };
    let (address, server) =
        spawn_server(key, behavior, Arc::clone(&password), Arc::clone(&counters)).await;

    let result = transport(Duration::from_millis(100))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await;

    assert!(matches!(result, Err(RusshTerminalOpenError::Timeout)));
    assert_eq!(counters.authentication.load(Ordering::Relaxed), 1);
    assert_eq!(counters.pty.load(Ordering::Relaxed), 1);
    assert_eq!(counters.shell.load(Ordering::Relaxed), 0);
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn owner_preserves_raw_order_and_applies_input_resize_and_local_close() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let (input_sender, mut input_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (resize_sender, mut resize_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (address, server) = spawn_owner_server(
        key,
        Arc::clone(&password),
        input_sender,
        resize_sender,
        true,
    )
    .await;
    let established = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await
        .expect("matching owner test session should establish");
    let (output_sender, mut output_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (control, owner) = established.into_owner(ChannelOutputSink(output_sender));
    let owner_task = tokio::spawn(owner.run());

    assert!(matches!(
        next_output(&mut output_receiver).await,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::Ready)
    ));
    let first = next_output(&mut output_receiver).await;
    let second = next_output(&mut output_receiver).await;
    assert!(matches!(first, CapturedOutput::Data(data) if data == [0, 0xff, b'A']));
    assert!(matches!(second, CapturedOutput::Data(data) if data == [0xfe, b'B']));

    control
        .try_input(vec![0x00, 0xff, 0x7f])
        .expect("bounded input should enqueue");
    control
        .try_resize(TerminalSize::new(132, 44).expect("resize should be valid"))
        .expect("valid resize should enqueue");
    let input = tokio::time::timeout(Duration::from_secs(1), input_receiver.recv())
        .await
        .expect("server should receive input")
        .expect("input channel should remain open");
    assert_eq!(input, [0x00, 0xff, 0x7f]);
    let resize = tokio::time::timeout(Duration::from_secs(1), resize_receiver.recv())
        .await
        .expect("server should receive resize")
        .expect("resize channel should remain open");
    assert_eq!(resize, (132, 44));

    control.close();
    let lifecycle = next_output(&mut output_receiver).await;
    assert!(matches!(
        lifecycle,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::Closed)
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), owner_task)
            .await
            .expect("owner should settle")
            .expect("owner task should not panic"),
        RusshTerminalSettlement::LocalClosed
    );
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn rejected_ready_output_closes_transport_and_settles_owner() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let (input_sender, _input_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (resize_sender, _resize_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (address, server) = spawn_owner_server(
        key,
        Arc::clone(&password),
        input_sender,
        resize_sender,
        false,
    )
    .await;
    let established = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await
        .expect("matching output-failure test session should establish");
    let (_control, owner) = established.into_owner(RejectingOutputSink);

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), owner.run())
            .await
            .expect("owner should settle after rejected Ready"),
        RusshTerminalSettlement::OutputUnavailable
    );
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn owner_chunks_output_and_requires_credit_before_draining_more_remote_data() {
    let password = runtime_password();
    let key = generated_key();
    let expected = fingerprint(&key);
    let (input_sender, _input_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (resize_sender, _resize_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (address, server, handle_receiver, channel_receiver) =
        spawn_controlled_owner_server(key, Arc::clone(&password), input_sender, resize_sender)
            .await;
    let established = transport(Duration::from_secs(3))
        .establish(authorized_request(address, expected, password.as_bytes()))
        .await
        .expect("matching credit test session should establish");
    let server_handle = ControlledServerHandle {
        handle: handle_receiver
            .await
            .expect("server handle should be available"),
        channel_id: channel_receiver
            .await
            .expect("server channel should be available"),
    };
    let (output_sender, mut output_receiver) = tokio::sync::mpsc::unbounded_channel();
    let (control, owner) = established.into_owner(ChannelOutputSink(output_sender));
    let owner_task = tokio::spawn(owner.run());

    assert!(matches!(
        next_output(&mut output_receiver).await,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::Ready)
    ));

    let first_batch = vec![0x80; 256 * 1024];
    server_handle
        .handle
        .data(server_handle.channel_id, first_batch)
        .await
        .expect("server should send initial credit-sized batch");
    let mut chunks = Vec::new();
    for _ in 0..8 {
        let output = next_output(&mut output_receiver).await;
        match output {
            CapturedOutput::Data(data) => {
                assert!(data.len() <= 32 * 1024);
                chunks.push(data);
            }
            CapturedOutput::Lifecycle(_) => panic!("expected data before lifecycle"),
        }
    }
    assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 256 * 1024);

    server_handle
        .handle
        .data(server_handle.channel_id, vec![0xff; 32 * 1024])
        .await
        .expect("server should queue post-credit data");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), output_receiver.recv())
            .await
            .is_err(),
        "owner must not emit more remote bytes before renderer grants credit"
    );

    control
        .try_grant_credit(32 * 1024)
        .expect("bounded credit should enqueue");
    let resumed = next_output(&mut output_receiver).await;
    assert!(matches!(resumed, CapturedOutput::Data(data) if data == vec![0xff; 32 * 1024]));
    control
        .try_grant_credit(1)
        .expect("one byte of credit lets the owner continue polling lifecycle messages");
    server_handle
        .handle
        .exit_status_request(server_handle.channel_id, 7)
        .await
        .expect("server should send exit status");
    server_handle
        .handle
        .eof(server_handle.channel_id)
        .await
        .expect("server should send EOF");
    server_handle
        .handle
        .close(server_handle.channel_id)
        .await
        .expect("server should close channel");
    assert!(matches!(
        next_output(&mut output_receiver).await,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::ExitStatus(7))
    ));
    assert!(matches!(
        next_output(&mut output_receiver).await,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::Eof)
    ));
    assert!(matches!(
        next_output(&mut output_receiver).await,
        CapturedOutput::Lifecycle(RusshTerminalLifecycle::Closed)
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), owner_task)
            .await
            .expect("owner should settle")
            .expect("owner task should not panic"),
        RusshTerminalSettlement::RemoteClosed
    );
    expect_server_settlement(server).await;
}

#[tokio::test]
async fn dropped_setup_future_closes_the_complete_socket_attempt() {
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
            .expect("SSH identification should arrive");
        assert!(first_read > 0);
        let _ = ready_sender.send(());
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    let password = runtime_password();
    let request = authorized_request(address, fingerprint(&generated_key()), password.as_bytes());
    let setup = transport(Duration::from_secs(3));
    let setup_task = tokio::spawn(async move { setup.establish(request).await });
    tokio::time::timeout(Duration::from_secs(1), ready_receiver)
        .await
        .expect("setup should start")
        .expect("readiness sender should remain available");

    setup_task.abort();
    assert!(
        setup_task
            .await
            .expect_err("aborted setup must not complete")
            .is_cancelled()
    );
    expect_server_settlement(server).await;
}

fn transport(timeout: Duration) -> RusshPasswordTerminalTransport {
    RusshPasswordTerminalTransport::new(
        RusshTerminalTimeouts::new(timeout, timeout, timeout, timeout, timeout)
            .expect("test timeout is nonzero"),
    )
}

fn authorized_request(
    address: SocketAddr,
    expected_fingerprint: HostFingerprint,
    password: &[u8],
) -> vigilshell_core::human_terminal::AuthorizedHumanTerminalOpen {
    let profile = profile(address);
    let repository = Repository {
        profile: profile.clone(),
        trust: StoredHostTrust::new(
            profile.id().clone(),
            profile.security_revision(),
            expected_fingerprint,
        ),
    };
    let request = OpenHumanTerminalRequest::new(
        profile.id().clone(),
        TerminalSize::new(100, 30).expect("terminal size should be valid"),
    );
    let prepared = HumanTerminalAdmissionService::new(&repository, &repository)
        .prepare(request)
        .expect("test profile and trust should pass admission");
    prepared
        .acquire_credential(&StaticCredential(password.to_vec()))
        .expect("test credential should be available")
}

#[derive(Clone)]
struct Repository {
    profile: ConnectionProfile,
    trust: StoredHostTrust,
}

impl ConnectionProfileRepository for Repository {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        Ok(Some(self.profile.clone()))
    }
}

impl HostTrustRepository for Repository {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<StoredHostTrust>, HostTrustRepositoryError> {
        Ok(Some(self.trust.clone()))
    }
}

struct StaticCredential(Vec<u8>);

impl CredentialBroker for StaticCredential {
    fn lease_for_human_terminal(
        &self,
        _credential_ref: &CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError> {
        HumanCredentialLease::from_password(self.0.clone())
            .map_err(|_| CredentialBrokerError::Unavailable)
    }
}

fn profile(address: SocketAddr) -> ConnectionProfile {
    ConnectionProfile::new(
        ConnectionProfileId::parse("loopback-profile").expect("profile id should be valid"),
        ManagedServerId::parse("loopback-server").expect("server id should be valid"),
        ConnectionDisplayName::parse("Loopback").expect("display name should be valid"),
        ConnectionRecordVersion::INITIAL,
        ConnectionSecurityRevision::INITIAL,
        SshRoute::new(address.ip().to_string(), address.port(), "operator")
            .expect("loopback route should be valid"),
        CredentialRef::parse("loopback-credential").expect("credential ref should be valid"),
        true,
    )
}

fn generated_key() -> PrivateKey {
    let mut random = russh::keys::key::safe_rng();
    PrivateKey::random(&mut random, Algorithm::Ed25519)
        .expect("runtime key generation should succeed")
}

fn runtime_password() -> Arc<str> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    Arc::from(format!("runtime-{nonce:x}-{}", std::process::id()))
}

fn fingerprint(key: &PrivateKey) -> HostFingerprint {
    HostFingerprint::parse(key.public_key().fingerprint(HashAlg::Sha256).to_string())
        .expect("generated key should have a canonical fingerprint")
}

async fn spawn_server(
    key: PrivateKey,
    behavior: ServerBehavior,
    password: Arc<str>,
    counters: Arc<ServerCounters>,
) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: Some(Duration::from_secs(5)),
        ..server::Config::default()
    });
    let server = tokio::spawn(async move {
        let (socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        if let Ok(session) = server::run_stream(
            config,
            socket,
            LoopbackServer {
                behavior,
                password,
                counters,
            },
        )
        .await
        {
            let _ = session.await;
        }
    });
    (address, server)
}

async fn expect_server_settlement(mut server: JoinHandle<()>) {
    match tokio::time::timeout(Duration::from_secs(2), &mut server).await {
        Ok(result) => result.expect("loopback server task should not panic"),
        Err(_) => {
            server.abort();
            let _ = server.await;
            panic!("transport must terminate the complete connection attempt");
        }
    }
}

async fn next_output(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<CapturedOutput>,
) -> CapturedOutput {
    tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("owner should deliver output")
        .expect("output channel should remain open")
}

async fn spawn_owner_server(
    key: PrivateKey,
    password: Arc<str>,
    input_sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    resize_sender: tokio::sync::mpsc::UnboundedSender<(u32, u32)>,
    output_after_shell: bool,
) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: Some(Duration::from_secs(5)),
        ..server::Config::default()
    });
    let server = tokio::spawn(async move {
        let (socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        if let Ok(session) = server::run_stream(
            config,
            socket,
            OwnerLoopbackServer {
                password,
                input_sender,
                resize_sender,
                output_after_shell,
            },
        )
        .await
        {
            let _ = session.await;
        }
    });
    (address, server)
}

struct ControlledServer {
    password: Arc<str>,
    channel_sender: Option<tokio::sync::oneshot::Sender<ChannelId>>,
}

impl Handler for ControlledServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if password == self.password.as_ref() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        if let Some(sender) = self.channel_sender.take() {
            let _ = sender.send(channel);
        }
        Ok(())
    }
}

struct ControlledServerHandle {
    handle: server::Handle,
    channel_id: ChannelId,
}

async fn spawn_controlled_owner_server(
    key: PrivateKey,
    password: Arc<str>,
    _input_sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    _resize_sender: tokio::sync::mpsc::UnboundedSender<(u32, u32)>,
) -> (
    SocketAddr,
    JoinHandle<()>,
    tokio::sync::oneshot::Receiver<server::Handle>,
    tokio::sync::oneshot::Receiver<ChannelId>,
) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener should bind");
    let address = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        inactivity_timeout: Some(Duration::from_secs(5)),
        ..server::Config::default()
    });
    let (handle_sender, handle_receiver) = tokio::sync::oneshot::channel();
    let (channel_sender, channel_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (socket, _) = listener
            .accept()
            .await
            .expect("loopback connection should arrive");
        if let Ok(session) = server::run_stream(
            config,
            socket,
            ControlledServer {
                password,
                channel_sender: Some(channel_sender),
            },
        )
        .await
        {
            let _ = handle_sender.send(session.handle());
            let _ = session.await;
        }
    });
    (address, server, handle_receiver, channel_receiver)
}
