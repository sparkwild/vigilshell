use std::{error::Error, fmt};

use zeroize::Zeroizing;

use crate::connection::{ConnectionProfile, ConnectionProfileId, HostFingerprint, StoredHostTrust};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalValueError {
    ZeroColumns,
    ZeroRows,
    Empty(&'static str),
    InvalidSessionId,
}

impl fmt::Display for TerminalValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroColumns => formatter.write_str("terminal columns must not be zero"),
            Self::ZeroRows => formatter.write_str("terminal rows must not be zero"),
            Self::Empty(field) => write!(formatter, "{field} must not be empty"),
            Self::InvalidSessionId => {
                formatter.write_str("human terminal session id must be a canonical UUID v4")
            }
        }
    }
}

impl Error for TerminalValueError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    columns: u16,
    rows: u16,
}

impl TerminalSize {
    pub fn new(columns: u16, rows: u16) -> Result<Self, TerminalValueError> {
        if columns == 0 {
            return Err(TerminalValueError::ZeroColumns);
        }
        if rows == 0 {
            return Err(TerminalValueError::ZeroRows);
        }
        Ok(Self { columns, rows })
    }

    pub const fn columns(self) -> u16 {
        self.columns
    }

    pub const fn rows(self) -> u16 {
        self.rows
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenHumanTerminalRequest {
    profile_id: ConnectionProfileId,
    terminal_size: TerminalSize,
}

impl OpenHumanTerminalRequest {
    pub fn new(profile_id: ConnectionProfileId, terminal_size: TerminalSize) -> Self {
        Self {
            profile_id,
            terminal_size,
        }
    }

    pub fn profile_id(&self) -> &ConnectionProfileId {
        &self.profile_id
    }

    pub const fn terminal_size(&self) -> TerminalSize {
        self.terminal_size
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionRepositoryError {
    Unavailable,
}

impl fmt::Display for ConnectionRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("connection repository is unavailable")
    }
}

impl Error for ConnectionRepositoryError {}

pub trait ConnectionProfileRepository {
    fn find(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostTrustRepositoryError {
    Unavailable,
}

impl fmt::Display for HostTrustRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host trust repository is unavailable")
    }
}

impl Error for HostTrustRepositoryError {}

pub trait HostTrustRepository {
    fn find(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<StoredHostTrust>, HostTrustRepositoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialBrokerError {
    Unavailable,
}

impl fmt::Display for CredentialBrokerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("credential broker is unavailable")
    }
}

impl Error for CredentialBrokerError {}

pub struct HumanCredentialLease {
    password: Zeroizing<Vec<u8>>,
}

impl HumanCredentialLease {
    pub fn from_password(password: impl Into<Vec<u8>>) -> Result<Self, TerminalValueError> {
        let password = password.into();
        if password.is_empty() {
            return Err(TerminalValueError::Empty("credential lease id"));
        }
        Ok(Self {
            password: Zeroizing::new(password),
        })
    }

    pub fn password(&self) -> &[u8] {
        &self.password
    }
}

impl fmt::Debug for HumanCredentialLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HumanCredentialLease([redacted])")
    }
}

pub trait CredentialBroker {
    fn lease_for_human_terminal(
        &self,
        credential_ref: &crate::connection::CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError>;
}

pub struct PreparedHumanTerminalOpen {
    profile: ConnectionProfile,
    expected_host_fingerprint: HostFingerprint,
    terminal_size: TerminalSize,
}

impl PreparedHumanTerminalOpen {
    pub fn profile(&self) -> &ConnectionProfile {
        &self.profile
    }

    pub fn expected_host_fingerprint(&self) -> &HostFingerprint {
        &self.expected_host_fingerprint
    }

    pub const fn terminal_size(&self) -> TerminalSize {
        self.terminal_size
    }

    pub fn acquire_credential<Credentials>(
        self,
        credentials: &Credentials,
    ) -> Result<AuthorizedHumanTerminalOpen, OpenHumanTerminalError>
    where
        Credentials: CredentialBroker,
    {
        let credential_lease = credentials
            .lease_for_human_terminal(self.profile.credential_ref())
            .map_err(|_| OpenHumanTerminalError::CredentialUnavailable)?;

        Ok(AuthorizedHumanTerminalOpen::new(
            self.profile,
            self.expected_host_fingerprint,
            credential_lease,
            self.terminal_size,
        ))
    }
}

impl fmt::Debug for PreparedHumanTerminalOpen {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedHumanTerminalOpen")
            .field("profile", &self.profile)
            .field("expected_host_fingerprint", &self.expected_host_fingerprint)
            .field("terminal_size", &self.terminal_size)
            .finish()
    }
}

pub struct AuthorizedHumanTerminalOpen {
    profile: ConnectionProfile,
    expected_host_fingerprint: HostFingerprint,
    credential_lease: HumanCredentialLease,
    terminal_size: TerminalSize,
}

impl AuthorizedHumanTerminalOpen {
    fn new(
        profile: ConnectionProfile,
        expected_host_fingerprint: HostFingerprint,
        credential_lease: HumanCredentialLease,
        terminal_size: TerminalSize,
    ) -> Self {
        Self {
            profile,
            expected_host_fingerprint,
            credential_lease,
            terminal_size,
        }
    }

    pub fn profile(&self) -> &ConnectionProfile {
        &self.profile
    }

    pub fn expected_host_fingerprint(&self) -> &HostFingerprint {
        &self.expected_host_fingerprint
    }

    pub fn password(&self) -> &[u8] {
        self.credential_lease.password()
    }

    pub const fn terminal_size(&self) -> TerminalSize {
        self.terminal_size
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HumanTerminalSessionId(String);

impl HumanTerminalSessionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, TerminalValueError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let canonical_hyphens = [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes.get(index) == Some(&b'-'));
        let canonical_hexadecimal = bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(byte)
        });
        let version_four = bytes.get(14) == Some(&b'4');
        let rfc_variant = bytes
            .get(19)
            .is_some_and(|byte| matches!(byte, b'8' | b'9' | b'a' | b'b'));
        if bytes.len() != 36
            || !canonical_hyphens
            || !canonical_hexadecimal
            || !version_four
            || !rfc_variant
        {
            return Err(TerminalValueError::InvalidSessionId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenHumanTerminalError {
    ConnectionRepositoryUnavailable,
    ConnectionNotFound,
    ConnectionRecordMismatch,
    ConnectionDisabled,
    HostTrustRepositoryUnavailable,
    HostTrustRequired,
    HostTrustStale,
    CredentialUnavailable,
}

impl fmt::Display for OpenHumanTerminalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ConnectionRepositoryUnavailable => "connection repository unavailable",
            Self::ConnectionNotFound => "connection profile not found",
            Self::ConnectionRecordMismatch => "connection repository returned a mismatched profile",
            Self::ConnectionDisabled => "connection profile is disabled",
            Self::HostTrustRepositoryUnavailable => "host trust repository unavailable",
            Self::HostTrustRequired => "host trust is required",
            Self::HostTrustStale => "host trust does not match the connection security revision",
            Self::CredentialUnavailable => "credential is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for OpenHumanTerminalError {}

pub struct HumanTerminalAdmissionService<'a, Profiles, Trust> {
    profiles: &'a Profiles,
    trust: &'a Trust,
}

impl<'a, Profiles, Trust> HumanTerminalAdmissionService<'a, Profiles, Trust>
where
    Profiles: ConnectionProfileRepository,
    Trust: HostTrustRepository,
{
    pub const fn new(profiles: &'a Profiles, trust: &'a Trust) -> Self {
        Self { profiles, trust }
    }

    pub fn prepare(
        &self,
        request: OpenHumanTerminalRequest,
    ) -> Result<PreparedHumanTerminalOpen, OpenHumanTerminalError> {
        let profile = self
            .profiles
            .find(request.profile_id())
            .map_err(|_| OpenHumanTerminalError::ConnectionRepositoryUnavailable)?
            .ok_or(OpenHumanTerminalError::ConnectionNotFound)?;

        if profile.id() != request.profile_id() {
            return Err(OpenHumanTerminalError::ConnectionRecordMismatch);
        }
        if !profile.is_enabled() {
            return Err(OpenHumanTerminalError::ConnectionDisabled);
        }

        let trust = self
            .trust
            .find(profile.id())
            .map_err(|_| OpenHumanTerminalError::HostTrustRepositoryUnavailable)?
            .ok_or(OpenHumanTerminalError::HostTrustRequired)?;
        if !trust.matches(&profile) {
            return Err(OpenHumanTerminalError::HostTrustStale);
        }

        Ok(PreparedHumanTerminalOpen {
            profile,
            expected_host_fingerprint: trust.fingerprint().clone(),
            terminal_size: request.terminal_size(),
        })
    }
}
