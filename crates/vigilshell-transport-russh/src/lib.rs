//! SSH/PTY 适配；取消整个连接尝试时同时关闭底层 socket。
use russh::{
    Channel, ChannelMsg, Disconnect, Preferred, client,
    keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate},
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    net::{Shutdown, TcpStream as StdStream},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpStream;

use vigilshell_application_contracts::ports::{
    FutureResult, ProxyRoute, SecretMaterial, SshForwarding, SshSession, SshTransport,
    TransportEvent,
};
use vigilshell_domain::workspace::*;
use zeroize::Zeroizing;

mod forwarding;
mod proxy;
mod sftp;
mod sftp_transfer;
mod tools;
pub struct RusshTransport;
struct SocketGuard(StdStream);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}
#[derive(Clone, Debug)]
struct TerminalDisconnect {
    error: Option<WorkspaceError>,
    message: Option<String>,
}
type DisconnectState = Arc<Mutex<Option<TerminalDisconnect>>>;
struct Handler {
    disconnected: Option<DisconnectState>,
    forwards: Option<Arc<forwarding::RemoteForwards>>,
    expected: Option<HostKey>,
    observed: Arc<Mutex<Option<HostKey>>>,
}
impl client::Handler for Handler {
    type Error = russh::Error;
    async fn disconnected(
        &mut self,
        reason: client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        let (diagnostic, result) = match reason {
            client::DisconnectReason::ReceivedDisconnect(info) => (
                TerminalDisconnect {
                    error: None,
                    message: Some(info.message.chars().take(1024).collect()),
                },
                Ok(()),
            ),
            client::DisconnectReason::Error(e) => (
                TerminalDisconnect {
                    error: Some(classify_error(&e)),
                    message: None,
                },
                Err(e),
            ),
        };
        if let Some(state) = &self.disconnected {
            *state.lock().unwrap() = Some(diagnostic);
        }
        result
    }
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if let Some(forwards) = &self.forwards {
            forwards
                .accept(channel, connected_address, connected_port, reply)
                .await;
        } else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }
    async fn check_server_key(
        &mut self,
        value: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = value else {
            return Ok(false);
        };
        let observed = HostKey {
            public_key: key.to_openssh().map_err(|_| russh::Error::UnknownKey)?,
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            algorithm: key.algorithm().to_string(),
        };
        *self
            .observed
            .lock()
            .map_err(|_| russh::Error::Inconsistent)? = Some(observed.clone());
        Ok(self.expected.as_ref().is_some_and(|expected| {
            expected.public_key == observed.public_key
                && expected.fingerprint == observed.fingerprint
        }))
    }
}
fn config() -> Arc<client::Config> {
    Arc::new(client::Config {
        preferred: Preferred {
            kex: Cow::Owned(vec![
                russh::kex::CURVE25519,
                russh::kex::CURVE25519_PRE_RFC_8731,
                russh::kex::DH_G14_SHA256,
                russh::kex::EXTENSION_SUPPORT_AS_CLIENT,
                russh::kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
            ]),
            key: Cow::Owned(
                Preferred::DEFAULT
                    .key
                    .iter()
                    .filter(|key| !matches!(key, keys::ssh_key::Algorithm::Rsa { hash: None }))
                    .cloned()
                    .collect(),
            ),
            compression: Cow::Borrowed(&[russh::compression::NONE]),
            ..Default::default()
        },
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        maximum_packet_size: 32 * 1024,
        window_size: 256 * 1024,
        ..Default::default()
    })
}
fn network_error(kind: std::io::ErrorKind) -> WorkspaceError {
    use std::io::ErrorKind;
    match kind {
        ErrorKind::ConnectionRefused => WorkspaceError::ConnectionRefused,
        ErrorKind::TimedOut => WorkspaceError::Timeout,
        ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable => {
            WorkspaceError::NetworkUnreachable
        }
        ErrorKind::ConnectionReset => WorkspaceError::ConnectionReset,
        _ => WorkspaceError::Disconnected,
    }
}
fn classify_error(e: &russh::Error) -> WorkspaceError {
    match e {
        russh::Error::UnknownKey => WorkspaceError::TrustChanged,
        russh::Error::NoCommonAlgo { .. } => WorkspaceError::AlgorithmUnsupported,
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => WorkspaceError::Timeout,
        russh::Error::IO(e) => network_error(e.kind()),
        _ => WorkspaceError::Disconnected,
    }
}
fn error(e: russh::Error) -> WorkspaceError {
    classify_error(&e)
}
async fn deadline<T>(
    future: impl std::future::Future<Output = Result<T, russh::Error>>,
    seconds: u64,
) -> Result<T, WorkspaceError> {
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .map_err(|_| WorkspaceError::Timeout)?
        .map_err(error)
}
async fn socket(input: &ConnectionInput) -> Result<(TcpStream, SocketGuard), WorkspaceError> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let addresses = tokio::net::lookup_host((input.host.as_str(), input.port))
            .await
            .map_err(|_| WorkspaceError::ResolveFailed)?;
        let mut last = WorkspaceError::ResolveFailed;
        for address in addresses {
            match TcpStream::connect(address).await {
                Ok(socket) => {
                    socket
                        .set_nodelay(true)
                        .map_err(|_| WorkspaceError::Disconnected)?;
                    let std = socket
                        .into_std()
                        .map_err(|_| WorkspaceError::Disconnected)?;
                    let guard =
                        SocketGuard(std.try_clone().map_err(|_| WorkspaceError::Disconnected)?);
                    return Ok((
                        TcpStream::from_std(std).map_err(|_| WorkspaceError::Disconnected)?,
                        guard,
                    ));
                }
                Err(e) => last = network_error(e.kind()),
            }
        }
        Err(last)
    })
    .await
    .map_err(|_| WorkspaceError::Timeout)?
}
impl SshTransport for RusshTransport {
    fn agent(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, Arc<dyn vigilshell_application_contracts::ports::AgentConnection>> {
        Box::pin(agent::connect(input, key, secret, route))
    }
    fn tools(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, Arc<dyn vigilshell_application_contracts::ports::ServerTools>> {
        Box::pin(async move { tools::connect(input, key, secret, route).await })
    }

    fn probe(&self, input: ConnectionInput) -> FutureResult<'_, HostKey> {
        self.probe_via(input, None)
    }
    fn probe_via(
        &self,
        input: ConnectionInput,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, HostKey> {
        Box::pin(async move {
            let (socket, _guard) = proxy::socket(&input, route).await?;
            let observed = Arc::new(Mutex::new(None));
            let result = deadline(
                client::connect_stream(
                    config(),
                    socket,
                    Handler {
                        disconnected: None,
                        forwards: None,
                        expected: None,
                        observed: observed.clone(),
                    },
                ),
                10,
            )
            .await;
            match result {
                Err(WorkspaceError::TrustChanged) => observed
                    .lock()
                    .map_err(|_| WorkspaceError::Disconnected)?
                    .clone()
                    .ok_or(WorkspaceError::AlgorithmUnsupported),
                Err(e) => Err(e),
                Ok(handle) => {
                    let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
                    Err(WorkspaceError::Disconnected)
                }
            }
        })
    }
    fn authenticate(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        self.authenticate_via(input, key, secret, None)
    }
    fn authenticate_via(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        Box::pin(async move {
            let disconnected = Arc::new(Mutex::new(None));
            let (handle, guard) =
                authenticated(input, key, secret, route, None, Some(disconnected.clone())).await?;
            Ok(Box::new(Session {
                disconnected,
                handle,
                channel: None,
                pending: VecDeque::new(),
                _guard: guard,
            }) as Box<dyn SshSession>)
        })
    }
    fn forwarding(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
        route: Option<ProxyRoute>,
    ) -> FutureResult<'_, Arc<dyn SshForwarding>> {
        Box::pin(async move { forwarding::connect(input, key, secret, route).await })
    }
}
async fn authenticated(
    input: ConnectionInput,
    key: HostKey,
    secret: SecretMaterial,
    route: Option<ProxyRoute>,
    forwards: Option<Arc<forwarding::RemoteForwards>>,
    disconnected: Option<DisconnectState>,
) -> Result<(client::Handle<Handler>, SocketGuard), WorkspaceError> {
    let diagnostics = std::env::var_os("VIGILSHELL_AUTH_DIAGNOSTICS").is_some_and(|v| v == "1");
    let trace_id = secret.diagnostic.as_ref().map(|t| t.id().to_owned());
    let disconnect_state = disconnected.unwrap_or_default();
    if let Some(trace) = &secret.diagnostic {
        trace.check("ssh_material", &secret.bytes);
        trace.record("ssh_auth_kind", Some(secret.kind == input.auth));
    }
    let (socket, guard) = proxy::socket(&input, route).await?;
    let mut handle = deadline(
        client::connect_stream(
            config(),
            socket,
            Handler {
                disconnected: Some(disconnect_state.clone()),
                forwards,
                expected: Some(key),
                observed: Arc::new(Mutex::new(None)),
            },
        ),
        10,
    )
    .await?;
    if diagnostics {
        tracing::info!(target: "vigilshell_auth_diagnostics", stage="ssh_authentication",
            trace_id=trace_id.as_deref().unwrap_or("untracked"), method=secret.kind.as_str(), outcome="started");
    }
    let attempt = match secret.kind {
        AuthKind::Password => {
            let password = Zeroizing::new(
                String::from_utf8(secret.bytes.to_vec())
                    .map_err(|_| WorkspaceError::CredentialUnavailable)?,
            );
            if let Some(trace) = &secret.diagnostic {
                trace.check("ssh_password_argument", password.as_bytes());
            }
            deadline(
                handle.authenticate_password(input.username.clone(), password.as_str()),
                15,
            )
            .await
        }
        AuthKind::PrivateKey => {
            let text =
                std::str::from_utf8(&secret.bytes).map_err(|_| WorkspaceError::InvalidKey)?;
            let key =
                keys::decode_secret_key(text, None).map_err(|_| WorkspaceError::InvalidKey)?;
            let hash = if key.algorithm().is_rsa() {
                Some(
                    deadline(handle.best_supported_rsa_hash(), 10)
                        .await?
                        .flatten()
                        .ok_or(WorkspaceError::AlgorithmUnsupported)?,
                )
            } else {
                None
            };
            deadline(
                handle.authenticate_publickey(
                    input.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                ),
                15,
            )
            .await
        }
    };
    drop(secret);
    if diagnostics {
        let disconnect = disconnect_state.lock().unwrap();
        let reason = disconnect
            .as_ref()
            .map(|d| d.error.map(|e| e.code()).unwrap_or("peer_disconnect"))
            .unwrap_or("not_observed");
        match &attempt {
            Ok(russh::client::AuthResult::Success) => {
                tracing::info!(target: "vigilshell_auth_diagnostics",
                stage="ssh_authentication", trace_id=trace_id.as_deref().unwrap_or("untracked"), outcome="success")
            }
            Ok(russh::client::AuthResult::Failure {
                remaining_methods,
                partial_success,
            }) => tracing::info!(target: "vigilshell_auth_diagnostics",
                stage="ssh_authentication", trace_id=trace_id.as_deref().unwrap_or("untracked"), outcome="not_authenticated",
                partial_success, remaining_methods=?remaining_methods, transport_closed=handle.is_closed(), disconnect=reason),
            Err(error) => tracing::info!(target: "vigilshell_auth_diagnostics",
                stage="ssh_authentication", trace_id=trace_id.as_deref().unwrap_or("untracked"), outcome="error",
                code=error.code(), transport_closed=handle.is_closed(), disconnect=reason),
        }
    }
    let authenticated = attempt?;
    if !authenticated.success() {
        return Err(WorkspaceError::AuthenticationFailed);
    }
    Ok((handle, guard))
}

async fn confirm(
    channel: &mut Channel<client::Msg>,
    pending: &mut VecDeque<TransportEvent>,
    rejected: WorkspaceError,
) -> Result<(), WorkspaceError> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => return Ok(()),
                Some(ChannelMsg::Failure) | None => return Err(rejected),
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if pending.len() >= 32 {
                        return Err(WorkspaceError::StreamUnavailable);
                    }
                    pending.push_back(TransportEvent::Data(data.to_vec()));
                }
                Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) => {
                    return Err(WorkspaceError::Disconnected);
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| WorkspaceError::Timeout)?
}
struct Session {
    disconnected: DisconnectState,
    handle: client::Handle<Handler>,
    channel: Option<Channel<client::Msg>>,
    pending: VecDeque<TransportEvent>,
    _guard: SocketGuard,
}
impl SshSession for Session {
    fn open_terminal(&mut self, columns: u16, rows: u16) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if columns == 0 || rows == 0 || self.channel.is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
            let mut channel = deadline(self.handle.channel_open_session(), 10)
                .await
                .map_err(|e| {
                    if e == WorkspaceError::Timeout {
                        e
                    } else {
                        WorkspaceError::ChannelRejected
                    }
                })?;
            deadline(
                channel.request_pty(
                    true,
                    "xterm-256color",
                    columns as u32,
                    rows as u32,
                    0,
                    0,
                    &[],
                ),
                10,
            )
            .await?;
            confirm(&mut channel, &mut self.pending, WorkspaceError::PtyRejected).await?;
            deadline(channel.request_shell(true), 10).await?;
            confirm(
                &mut channel,
                &mut self.pending,
                WorkspaceError::ShellRejected,
            )
            .await?;
            self.channel = Some(channel);
            Ok(())
        })
    }

    fn read(&mut self) -> FutureResult<'_, TransportEvent> {
        Box::pin(async move {
            if let Some(event) = self.pending.pop_front() {
                return Ok(event);
            }
            let channel = self
                .channel
                .as_mut()
                .ok_or(WorkspaceError::ChannelRejected)?;
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Data { data })
                    | Some(ChannelMsg::ExtendedData { data, .. }) => {
                        return Ok(TransportEvent::Data(data.to_vec()));
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        return Ok(TransportEvent::Exit(exit_status));
                    }
                    Some(ChannelMsg::ExitSignal {
                        signal_name,
                        error_message,
                        ..
                    }) => {
                        return Ok(TransportEvent::ExitSignal(
                            format!("{signal_name:?}: {error_message}")
                                .chars()
                                .take(1024)
                                .collect(),
                        ));
                    }
                    Some(ChannelMsg::Eof) => continue,
                    Some(ChannelMsg::Close) => return Ok(TransportEvent::Eof),
                    None => {
                        // The task owns the channel senders; joining it preserves its final disconnect reason.
                        let result = (&mut self.handle).await;
                        let diagnostic = self.disconnected.lock().unwrap().clone();
                        return Ok(match diagnostic {
                            Some(d) => TransportEvent::Disconnected {
                                error: d.error,
                                message: d.message,
                            },
                            None => TransportEvent::Disconnected {
                                error: result.err().map(error),
                                message: None,
                            },
                        });
                    }
                    _ => {}
                }
            }
        })
    }
    fn write<'a>(&'a mut self, data: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async move {
            deadline(
                self.channel
                    .as_mut()
                    .ok_or(WorkspaceError::ChannelRejected)?
                    .data(data),
                10,
            )
            .await
        })
    }
    fn resize(&mut self, columns: u16, rows: u16) -> FutureResult<'_, ()> {
        Box::pin(async move {
            deadline(
                self.channel
                    .as_mut()
                    .ok_or(WorkspaceError::ChannelRejected)?
                    .window_change(columns as u32, rows as u32, 0, 0),
                10,
            )
            .await
        })
    }
    fn close(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let _ = tokio::time::timeout(Duration::from_secs(1), async {
                if let Some(channel) = &self.channel {
                    let _ = channel.eof().await;
                    let _ = channel.close().await;
                }
                let _ = self
                    .handle
                    .disconnect(Disconnect::ByApplication, "", "")
                    .await;
            })
            .await;
            let _ = self._guard.0.shutdown(Shutdown::Both);
            Ok(())
        })
    }
}

#[cfg(test)]
mod network_diagnostics {
    use super::*;
    #[test]
    fn preserves_known_network_causes_without_guessing_unknown_errors() {
        use std::io::ErrorKind;
        assert_eq!(
            network_error(ErrorKind::ConnectionRefused),
            WorkspaceError::ConnectionRefused
        );
        assert_eq!(network_error(ErrorKind::TimedOut), WorkspaceError::Timeout);
        assert_eq!(
            network_error(ErrorKind::NetworkUnreachable),
            WorkspaceError::NetworkUnreachable
        );
        assert_eq!(
            network_error(ErrorKind::HostUnreachable),
            WorkspaceError::NetworkUnreachable
        );
        assert_eq!(
            network_error(ErrorKind::ConnectionReset),
            WorkspaceError::ConnectionReset
        );
        assert_eq!(
            network_error(ErrorKind::Other),
            WorkspaceError::Disconnected
        );
        assert_eq!(
            error(russh::Error::IO(std::io::Error::from(ErrorKind::TimedOut))),
            WorkspaceError::Timeout
        );
    }
}

#[cfg(test)]
mod disconnect_tests {
    use super::*;
    use russh::client::Handler as _;

    #[tokio::test]
    async fn disconnect_callback_preserves_known_errors_and_bounds_remote_text() {
        let state = Arc::new(Mutex::new(None));
        let mut handler = Handler {
            disconnected: Some(state.clone()),
            forwards: None,
            expected: None,
            observed: Arc::new(Mutex::new(None)),
        };
        for failure in [
            russh::Error::ConnectionTimeout,
            russh::Error::KeepaliveTimeout,
            russh::Error::InactivityTimeout,
        ] {
            assert!(
                handler
                    .disconnected(client::DisconnectReason::Error(failure))
                    .await
                    .is_err()
            );
            assert_eq!(
                state.lock().unwrap().as_ref().unwrap().error,
                Some(WorkspaceError::Timeout)
            );
        }
        assert!(
            handler
                .disconnected(client::DisconnectReason::Error(russh::Error::IO(
                    std::io::ErrorKind::ConnectionReset.into()
                )))
                .await
                .is_err()
        );
        assert_eq!(
            state.lock().unwrap().as_ref().unwrap().error,
            Some(WorkspaceError::ConnectionReset)
        );
        handler
            .disconnected(client::DisconnectReason::ReceivedDisconnect(
                client::RemoteDisconnectInfo {
                    reason_code: Disconnect::ByApplication,
                    message: "远".repeat(2048),
                    lang_tag: "".into(),
                },
            ))
            .await
            .unwrap();
        let value = state.lock().unwrap().clone().unwrap();
        assert_eq!(value.error, None);
        assert_eq!(value.message.unwrap().chars().count(), 1024);
    }
}
mod agent;
