use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionValueError {
    Empty(&'static str),
    TooLong {
        field: &'static str,
        maximum_bytes: usize,
    },
    ContainsWhitespace(&'static str),
    ContainsControlCharacter(&'static str),
    InvalidPort,
    InvalidRecordVersion,
    InvalidSecurityRevision,
    InvalidHostFingerprint,
}

impl fmt::Display for ConnectionValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(field) => write!(formatter, "{field} must not be empty"),
            Self::TooLong {
                field,
                maximum_bytes,
            } => write!(
                formatter,
                "{field} must not exceed {maximum_bytes} UTF-8 bytes"
            ),
            Self::ContainsWhitespace(field) => {
                write!(formatter, "{field} must not contain whitespace")
            }
            Self::ContainsControlCharacter(field) => {
                write!(formatter, "{field} must not contain control characters")
            }
            Self::InvalidPort => formatter.write_str("SSH port must not be zero"),
            Self::InvalidRecordVersion => {
                formatter.write_str("connection record version must not be zero")
            }
            Self::InvalidSecurityRevision => {
                formatter.write_str("connection security revision must not be zero")
            }
            Self::InvalidHostFingerprint => {
                formatter.write_str("host fingerprint must use canonical SHA-256 encoding")
            }
        }
    }
}

impl Error for ConnectionValueError {}

macro_rules! string_identifier {
    ($name:ident, $field:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ConnectionValueError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(ConnectionValueError::Empty($field));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_identifier!(ManagedServerId, "managed server id");
string_identifier!(ConnectionProfileId, "connection profile id");

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostFingerprint(String);

impl HostFingerprint {
    pub fn parse(value: impl Into<String>) -> Result<Self, ConnectionValueError> {
        let value = value.into();
        let encoded = value
            .strip_prefix("SHA256:")
            .ok_or(ConnectionValueError::InvalidHostFingerprint)?;
        let has_canonical_length = encoded.len() == 43;
        let has_base64_alphabet = encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'));
        let has_canonical_final_bits = encoded.as_bytes().last().is_some_and(|byte| {
            matches!(
                byte,
                b'A' | b'E'
                    | b'I'
                    | b'M'
                    | b'Q'
                    | b'U'
                    | b'Y'
                    | b'c'
                    | b'g'
                    | b'k'
                    | b'o'
                    | b's'
                    | b'w'
                    | b'0'
                    | b'4'
                    | b'8'
            )
        });

        if !has_canonical_length || !has_base64_alphabet || !has_canonical_final_bits {
            return Err(ConnectionValueError::InvalidHostFingerprint);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HostFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HostFingerprint([redacted])")
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CredentialRef(String);

impl CredentialRef {
    pub fn parse(value: impl Into<String>) -> Result<Self, ConnectionValueError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ConnectionValueError::Empty("credential reference"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CredentialRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialRef([opaque])")
    }
}

/// Human 可见且经过规范化的 Connection 名称。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionDisplayName(String);

impl ConnectionDisplayName {
    pub fn parse(value: impl Into<String>) -> Result<Self, ConnectionValueError> {
        const MAXIMUM_BYTES: usize = 128;

        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(ConnectionValueError::Empty("connection display name"));
        }
        if value.len() > MAXIMUM_BYTES {
            return Err(ConnectionValueError::TooLong {
                field: "connection display name",
                maximum_bytes: MAXIMUM_BYTES,
            });
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Connection 行记录的乐观并发版本。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionRecordVersion(u64);

impl ConnectionRecordVersion {
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, ConnectionValueError> {
        if value == 0 {
            return Err(ConnectionValueError::InvalidRecordVersion);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionSecurityRevision(u64);

impl ConnectionSecurityRevision {
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, ConnectionValueError> {
        if value == 0 {
            return Err(ConnectionValueError::InvalidSecurityRevision);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SshRoute {
    host: String,
    port: u16,
    username: String,
}

impl SshRoute {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
    ) -> Result<Self, ConnectionValueError> {
        let host = host.into();
        let username = username.into();
        let host = host.trim();
        let username = username.trim();
        if host.is_empty() {
            return Err(ConnectionValueError::Empty("SSH host"));
        }
        if host.len() > 255 {
            return Err(ConnectionValueError::TooLong {
                field: "SSH host",
                maximum_bytes: 255,
            });
        }
        if host.chars().any(char::is_whitespace) {
            return Err(ConnectionValueError::ContainsWhitespace("SSH host"));
        }
        if host.chars().any(char::is_control) {
            return Err(ConnectionValueError::ContainsControlCharacter("SSH host"));
        }
        if username.is_empty() {
            return Err(ConnectionValueError::Empty("SSH username"));
        }
        if username.len() > 128 {
            return Err(ConnectionValueError::TooLong {
                field: "SSH username",
                maximum_bytes: 128,
            });
        }
        if username.chars().any(char::is_control) {
            return Err(ConnectionValueError::ContainsControlCharacter(
                "SSH username",
            ));
        }
        if port == 0 {
            return Err(ConnectionValueError::InvalidPort);
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            username: username.to_owned(),
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub const fn port(&self) -> u16 {
        self.port
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

impl fmt::Debug for SshRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SshRoute")
            .field("host", &"[redacted]")
            .field("port", &self.port)
            .field("username", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionProfile {
    id: ConnectionProfileId,
    server_id: ManagedServerId,
    display_name: ConnectionDisplayName,
    record_version: ConnectionRecordVersion,
    security_revision: ConnectionSecurityRevision,
    route: SshRoute,
    credential_ref: CredentialRef,
    enabled: bool,
}

impl ConnectionProfile {
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the complete validated profile record"
    )]
    pub fn new(
        id: ConnectionProfileId,
        server_id: ManagedServerId,
        display_name: ConnectionDisplayName,
        record_version: ConnectionRecordVersion,
        security_revision: ConnectionSecurityRevision,
        route: SshRoute,
        credential_ref: CredentialRef,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            server_id,
            display_name,
            record_version,
            security_revision,
            route,
            credential_ref,
            enabled,
        }
    }

    pub fn id(&self) -> &ConnectionProfileId {
        &self.id
    }

    pub fn server_id(&self) -> &ManagedServerId {
        &self.server_id
    }

    pub fn display_name(&self) -> &ConnectionDisplayName {
        &self.display_name
    }

    pub const fn record_version(&self) -> ConnectionRecordVersion {
        self.record_version
    }

    pub const fn security_revision(&self) -> ConnectionSecurityRevision {
        self.security_revision
    }

    pub fn route(&self) -> &SshRoute {
        &self.route
    }

    pub fn credential_ref(&self) -> &CredentialRef {
        &self.credential_ref
    }

    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl fmt::Debug for ConnectionProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionProfile")
            .field("id", &self.id)
            .field("server_id", &self.server_id)
            .field("display_name", &self.display_name)
            .field("record_version", &self.record_version)
            .field("security_revision", &self.security_revision)
            .field("route", &self.route)
            .field("credential_ref", &self.credential_ref)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredHostTrust {
    profile_id: ConnectionProfileId,
    security_revision: ConnectionSecurityRevision,
    fingerprint: HostFingerprint,
}

impl StoredHostTrust {
    pub fn new(
        profile_id: ConnectionProfileId,
        security_revision: ConnectionSecurityRevision,
        fingerprint: HostFingerprint,
    ) -> Self {
        Self {
            profile_id,
            security_revision,
            fingerprint,
        }
    }

    pub fn profile_id(&self) -> &ConnectionProfileId {
        &self.profile_id
    }

    pub const fn security_revision(&self) -> ConnectionSecurityRevision {
        self.security_revision
    }

    pub fn fingerprint(&self) -> &HostFingerprint {
        &self.fingerprint
    }

    pub fn matches(&self, profile: &ConnectionProfile) -> bool {
        self.profile_id == *profile.id() && self.security_revision == profile.security_revision()
    }
}
