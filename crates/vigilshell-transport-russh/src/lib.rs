use std::{
    borrow::Cow,
    error::Error,
    fmt,
    net::{Shutdown, TcpStream as StandardTcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

use russh::{
    AlgorithmKind, Disconnect, Preferred, cipher,
    client::{self, Handler},
    compression, kex,
    keys::{Algorithm, EcdsaCurve, HashAlg, PublicKey},
    mac,
};
use tokio::net::TcpStream;
use vigilshell_core::{
    connection::{HostFingerprint, SshRoute},
    host_trust::{HostKeyAlgorithm, ObservedHostKey},
};

mod terminal;

pub use terminal::{
    EstablishedRusshTerminal, RusshPasswordTerminalTransport, RusshTerminalControlError,
    RusshTerminalControlHandle, RusshTerminalData, RusshTerminalExitSignal, RusshTerminalLifecycle,
    RusshTerminalOpenError, RusshTerminalOutput, RusshTerminalOutputError, RusshTerminalOutputSink,
    RusshTerminalOwner, RusshTerminalSettlement, RusshTerminalTimeouts, RusshTerminalTimeoutsError,
};

const PRODUCTION_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

const APPROVED_KEX_ALGORITHMS: &[kex::Name] = &[
    kex::CURVE25519,
    kex::CURVE25519_PRE_RFC_8731,
    kex::DH_GEX_SHA256,
    kex::DH_G14_SHA256,
    kex::DH_G15_SHA512,
    kex::DH_G16_SHA512,
    kex::DH_G17_SHA512,
    kex::DH_G18_SHA512,
    kex::EXTENSION_SUPPORT_AS_CLIENT,
    kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
];

const APPROVED_HOST_KEY_ALGORITHMS: &[Algorithm] = &[
    Algorithm::Ed25519,
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP384,
    },
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP521,
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha512),
    },
    Algorithm::Rsa {
        hash: Some(HashAlg::Sha256),
    },
];

const APPROVED_CIPHERS: &[cipher::Name] = &[
    cipher::CHACHA20_POLY1305,
    cipher::AES_256_GCM,
    cipher::AES_256_CTR,
    cipher::AES_192_CTR,
    cipher::AES_128_CTR,
];

const APPROVED_MACS: &[mac::Name] = &[
    mac::HMAC_SHA512_ETM,
    mac::HMAC_SHA256_ETM,
    mac::HMAC_SHA512,
    mac::HMAC_SHA256,
];

const APPROVED_COMPRESSION: &[compression::Name] = &[compression::NONE];

/// Host Key probe 配置可公开的稳定校验失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKeyProbeConfigurationError {
    ZeroTimeout,
}

impl fmt::Display for HostKeyProbeConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host key probe timeout must not be zero")
    }
}

impl Error for HostKeyProbeConfigurationError {}

/// KEX-only Host Key probe 可公开的稳定失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKeyProbeError {
    Timeout,
    ConnectionUnavailable,
    UnsupportedKeyExchange,
    UnsupportedHostKey,
    ProtocolFailure,
    ObservationUnavailable,
    UnexpectedCompletion,
}

impl fmt::Display for HostKeyProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Timeout => "host key probe timed out",
            Self::ConnectionUnavailable => "host key probe connection is unavailable",
            Self::UnsupportedKeyExchange => "remote key exchange policy is unsupported",
            Self::UnsupportedHostKey => "remote host key policy is unsupported",
            Self::ProtocolFailure => "host key probe protocol failed",
            Self::ObservationUnavailable => "host key observation is unavailable",
            Self::UnexpectedCompletion => "host key probe completed unexpectedly",
        };
        formatter.write_str(message)
    }
}

impl Error for HostKeyProbeError {}

/// 使用固定 production 算法策略执行 KEX-only Host Key 探测。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RusshHostKeyProbe {
    timeout: Duration,
}

impl RusshHostKeyProbe {
    pub fn new(timeout: Duration) -> Result<Self, HostKeyProbeConfigurationError> {
        if timeout.is_zero() {
            return Err(HostKeyProbeConfigurationError::ZeroTimeout);
        }
        Ok(Self { timeout })
    }

    pub const fn production() -> Self {
        Self {
            timeout: PRODUCTION_PROBE_TIMEOUT,
        }
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    pub async fn probe(&self, route: &SshRoute) -> Result<ObservedHostKey, HostKeyProbeError> {
        tokio::time::timeout(self.timeout, probe_once(route))
            .await
            .map_err(|_| HostKeyProbeError::Timeout)?
    }
}

impl Default for RusshHostKeyProbe {
    fn default() -> Self {
        Self::production()
    }
}

fn approved_preferred() -> Preferred {
    Preferred {
        kex: Cow::Borrowed(APPROVED_KEX_ALGORITHMS),
        key: Cow::Borrowed(APPROVED_HOST_KEY_ALGORITHMS),
        cipher: Cow::Borrowed(APPROVED_CIPHERS),
        mac: Cow::Borrowed(APPROVED_MACS),
        compression: Cow::Borrowed(APPROVED_COMPRESSION),
    }
}

async fn probe_once(route: &SshRoute) -> Result<ObservedHostKey, HostKeyProbeError> {
    let socket = TcpStream::connect((route.host(), route.port()))
        .await
        .map_err(|_| HostKeyProbeError::ConnectionUnavailable)?;
    socket
        .set_nodelay(true)
        .map_err(|_| HostKeyProbeError::ConnectionUnavailable)?;

    let standard_socket = socket
        .into_std()
        .map_err(|_| HostKeyProbeError::ConnectionUnavailable)?;
    let shutdown_socket = standard_socket
        .try_clone()
        .map_err(|_| HostKeyProbeError::ConnectionUnavailable)?;
    let socket = TcpStream::from_std(standard_socket)
        .map_err(|_| HostKeyProbeError::ConnectionUnavailable)?;
    let _attempt_guard = ProbeAttemptGuard(shutdown_socket);

    let observation = Arc::new(Mutex::new(ProbeObservation::default()));
    let handler = ProbeHandler {
        observation: Arc::clone(&observation),
    };
    let config = Arc::new(client::Config {
        preferred: approved_preferred(),
        inactivity_timeout: None,
        keepalive_interval: None,
        anonymous: false,
        nodelay: true,
        ..client::Config::default()
    });

    match client::connect_stream(config, socket, handler).await {
        Err(error) => settle_expected_rejection(error, &observation),
        Ok(handle) => {
            let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
            drop(handle);
            Err(HostKeyProbeError::UnexpectedCompletion)
        }
    }
}

fn settle_expected_rejection(
    error: ProbeHandlerError,
    observation: &Mutex<ProbeObservation>,
) -> Result<ObservedHostKey, HostKeyProbeError> {
    match error.0 {
        russh::Error::UnknownKey => observation
            .lock()
            .map_err(|_| HostKeyProbeError::ObservationUnavailable)?
            .observed
            .clone()
            .ok_or(HostKeyProbeError::ObservationUnavailable),
        russh::Error::NoCommonAlgo {
            kind: AlgorithmKind::Kex,
            ..
        } => Err(HostKeyProbeError::UnsupportedKeyExchange),
        russh::Error::NoCommonAlgo {
            kind: AlgorithmKind::Key,
            ..
        } => Err(HostKeyProbeError::UnsupportedHostKey),
        russh::Error::IO(_) | russh::Error::HUP | russh::Error::ConnectionTimeout => {
            Err(HostKeyProbeError::ConnectionUnavailable)
        }
        _ => Err(HostKeyProbeError::ProtocolFailure),
    }
}

struct ProbeAttemptGuard(StandardTcpStream);

impl Drop for ProbeAttemptGuard {
    fn drop(&mut self) {
        // russh 的会话任务与 connect future 分离；关闭克隆 socket 才能让 timeout/drop 终止整个 attempt。
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

#[derive(Default)]
struct ProbeObservation {
    negotiated_algorithm: Option<HostKeyAlgorithm>,
    observed: Option<ObservedHostKey>,
}

struct ProbeHandler {
    observation: Arc<Mutex<ProbeObservation>>,
}

impl Handler for ProbeHandler {
    type Error = ProbeHandlerError;

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        names: &russh::Names,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let mut observation = self
            .observation
            .lock()
            .map_err(|_| ProbeHandlerError(russh::Error::Inconsistent))?;
        observation.negotiated_algorithm = map_negotiated_algorithm(&names.key);
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let mut observation = self
            .observation
            .lock()
            .map_err(|_| ProbeHandlerError(russh::Error::Inconsistent))?;
        let Some(algorithm) = observation.negotiated_algorithm else {
            return Ok(false);
        };
        if !negotiated_algorithm_matches_key(algorithm, &server_public_key.algorithm()) {
            return Ok(false);
        }

        let fingerprint = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        if let Ok(fingerprint) = HostFingerprint::parse(fingerprint) {
            observation.observed = Some(ObservedHostKey::new(algorithm, fingerprint));
        }
        Ok(false)
    }
}

struct ProbeHandlerError(russh::Error);

impl From<russh::Error> for ProbeHandlerError {
    fn from(error: russh::Error) -> Self {
        Self(error)
    }
}

impl fmt::Debug for ProbeHandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProbeHandlerError([redacted])")
    }
}

impl fmt::Display for ProbeHandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host key probe handler failed")
    }
}

impl Error for ProbeHandlerError {}

fn map_negotiated_algorithm(algorithm: &Algorithm) -> Option<HostKeyAlgorithm> {
    match algorithm {
        Algorithm::Ed25519 => Some(HostKeyAlgorithm::Ed25519),
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        } => Some(HostKeyAlgorithm::EcdsaP256),
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        } => Some(HostKeyAlgorithm::EcdsaP384),
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        } => Some(HostKeyAlgorithm::EcdsaP521),
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        } => Some(HostKeyAlgorithm::RsaSha512),
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        } => Some(HostKeyAlgorithm::RsaSha256),
        _ => None,
    }
}

fn negotiated_algorithm_matches_key(
    negotiated: HostKeyAlgorithm,
    key_algorithm: &Algorithm,
) -> bool {
    matches!(
        (negotiated, key_algorithm),
        (HostKeyAlgorithm::Ed25519, Algorithm::Ed25519)
            | (
                HostKeyAlgorithm::EcdsaP256,
                Algorithm::Ecdsa {
                    curve: EcdsaCurve::NistP256,
                },
            )
            | (
                HostKeyAlgorithm::EcdsaP384,
                Algorithm::Ecdsa {
                    curve: EcdsaCurve::NistP384,
                },
            )
            | (
                HostKeyAlgorithm::EcdsaP521,
                Algorithm::Ecdsa {
                    curve: EcdsaCurve::NistP521,
                },
            )
            | (
                HostKeyAlgorithm::RsaSha512 | HostKeyAlgorithm::RsaSha256,
                Algorithm::Rsa { .. }
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_policy_uses_exact_approved_algorithm_lists() {
        let preferred = approved_preferred();
        let kex: Vec<_> = preferred.kex.iter().map(AsRef::as_ref).collect();
        let keys: Vec<_> = preferred.key.iter().map(Algorithm::as_str).collect();
        let ciphers: Vec<_> = preferred.cipher.iter().map(AsRef::as_ref).collect();
        let macs: Vec<_> = preferred.mac.iter().map(AsRef::as_ref).collect();
        let compression: Vec<_> = preferred.compression.iter().map(AsRef::as_ref).collect();

        assert_eq!(
            kex,
            [
                "curve25519-sha256",
                "curve25519-sha256@libssh.org",
                "diffie-hellman-group-exchange-sha256",
                "diffie-hellman-group14-sha256",
                "diffie-hellman-group15-sha512",
                "diffie-hellman-group16-sha512",
                "diffie-hellman-group17-sha512",
                "diffie-hellman-group18-sha512",
                "ext-info-c",
                "kex-strict-c-v00@openssh.com",
            ]
        );
        assert_eq!(
            keys,
            [
                "ssh-ed25519",
                "ecdsa-sha2-nistp256",
                "ecdsa-sha2-nistp384",
                "ecdsa-sha2-nistp521",
                "rsa-sha2-512",
                "rsa-sha2-256",
            ]
        );
        assert_eq!(
            ciphers,
            [
                "chacha20-poly1305@openssh.com",
                "aes256-gcm@openssh.com",
                "aes256-ctr",
                "aes192-ctr",
                "aes128-ctr",
            ]
        );
        assert_eq!(
            macs,
            [
                "hmac-sha2-512-etm@openssh.com",
                "hmac-sha2-256-etm@openssh.com",
                "hmac-sha2-512",
                "hmac-sha2-256",
            ]
        );
        assert_eq!(compression, ["none"]);
    }

    #[test]
    fn timeout_must_be_nonzero() {
        assert_eq!(
            RusshHostKeyProbe::new(Duration::ZERO),
            Err(HostKeyProbeConfigurationError::ZeroTimeout)
        );
        assert_eq!(
            RusshHostKeyProbe::production().timeout(),
            PRODUCTION_PROBE_TIMEOUT
        );
    }

    #[test]
    fn public_errors_do_not_render_sensitive_context() {
        let errors = [
            HostKeyProbeError::Timeout,
            HostKeyProbeError::ConnectionUnavailable,
            HostKeyProbeError::UnsupportedKeyExchange,
            HostKeyProbeError::UnsupportedHostKey,
            HostKeyProbeError::ProtocolFailure,
            HostKeyProbeError::ObservationUnavailable,
            HostKeyProbeError::UnexpectedCompletion,
        ];

        for error in errors {
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("sensitive-endpoint"));
            assert!(!rendered.contains("sensitive-remote-text"));
            assert!(!rendered.contains("SHA256:"));
        }

        let internal = ProbeHandlerError(russh::Error::NoCommonAlgo {
            kind: AlgorithmKind::Key,
            ours: vec!["sensitive-local-policy".to_owned()],
            theirs: vec!["sensitive-remote-policy".to_owned()],
        });
        let rendered = format!("{internal:?} {internal}");
        assert!(!rendered.contains("sensitive-local-policy"));
        assert!(!rendered.contains("sensitive-remote-policy"));

        let internal = ProbeHandlerError(russh::Error::IO(std::io::Error::other(
            "sensitive-endpoint",
        )));
        let rendered = format!("{internal:?} {internal}");
        assert!(!rendered.contains("sensitive-endpoint"));
    }
}
