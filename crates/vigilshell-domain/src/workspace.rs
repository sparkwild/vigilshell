//! 工作区领域值与纯规则。秘密和存储引用不属于桌面读模型。
pub use crate::vault::*;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidInput,
    NotFound,
    Conflict,
    Busy,
    DuplicateName,
    StorageUnavailable,
    AgentConfigurationUnavailable,
    PolicyUpgradeRequired,
    ConfigurationLocked,
    ConfigurationCorrupt,
    ConfigurationFormatUnsupported,
    CredentialMissing,
    CredentialDenied,
    CredentialUnavailable,
    VaultKeyMissing,
    VaultKeyInvalid,
    CredentialCorrupt,
    CredentialFormatUnsupported,
    InvalidKey,
    UnsupportedKeyFormat,
    WrongPassphrase,
    NativeUnavailable,
    Cancelled,
    TooLate,
    TrustRequired,
    TrustChanged,
    TrustExpired,
    ResolveFailed,
    ConnectionRefused,
    NetworkUnreachable,
    ConnectionReset,
    Timeout,
    AlgorithmUnsupported,
    AuthenticationFailed,
    ChannelRejected,
    PtyRejected,
    ShellRejected,
    Disconnected,
    StreamUnavailable,
    RevisionLimit,
    EncodingUnsupported,
    UnmappableInput,
    ProxyFailed,
    ProxyAuthenticationFailed,
    ProxyUnavailable,
    ProxyTimeout,
    ProxyInUse,
    TunnelFailed,
    ToolsUnavailable,
    FilesPermissionDenied,
    FilesExists,
    FilesUnsupported,
    FilesTooLarge,
    FilesNotEmpty,
}
impl WorkspaceError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Busy => "busy",
            Self::DuplicateName => "duplicate_name",
            Self::StorageUnavailable => "storage_unavailable",
            Self::AgentConfigurationUnavailable => "agent_configuration_unavailable",
            Self::PolicyUpgradeRequired => "policy_upgrade_required",
            Self::ConfigurationLocked => "configuration_locked",
            Self::ConfigurationCorrupt => "configuration_corrupt",
            Self::ConfigurationFormatUnsupported => "configuration_format_unsupported",
            Self::CredentialMissing => "credential_missing",
            Self::CredentialDenied => "credential_denied",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::VaultKeyMissing => "vault_key_missing",
            Self::VaultKeyInvalid => "vault_key_invalid",
            Self::CredentialCorrupt => "credential_corrupt",
            Self::CredentialFormatUnsupported => "credential_format_unsupported",
            Self::InvalidKey => "invalid_key",
            Self::UnsupportedKeyFormat => "unsupported_key_format",
            Self::WrongPassphrase => "wrong_passphrase",
            Self::NativeUnavailable => "native_unavailable",
            Self::Cancelled => "cancelled",
            Self::TooLate => "too_late",
            Self::TrustRequired => "trust_required",
            Self::TrustChanged => "trust_changed",
            Self::TrustExpired => "trust_expired",
            Self::ResolveFailed => "resolve_failed",
            Self::ConnectionRefused => "connection_refused",
            Self::NetworkUnreachable => "network_unreachable",
            Self::ConnectionReset => "connection_reset",
            Self::Timeout => "connection_timeout",
            Self::AlgorithmUnsupported => "algorithm_unsupported",
            Self::AuthenticationFailed => "authentication_failed",
            Self::ChannelRejected => "channel_rejected",
            Self::PtyRejected => "pty_rejected",
            Self::ShellRejected => "shell_rejected",
            Self::Disconnected => "disconnected",
            Self::StreamUnavailable => "stream_unavailable",
            Self::RevisionLimit => "revision_limit",
            Self::EncodingUnsupported => "encoding_unsupported",
            Self::UnmappableInput => "unmappable_input",
            Self::ProxyFailed => "proxy_failed",
            Self::ProxyAuthenticationFailed => "proxy_authentication_failed",
            Self::ProxyUnavailable => "proxy_unavailable",
            Self::ProxyTimeout => "proxy_timeout",
            Self::ProxyInUse => "proxy_in_use",
            Self::TunnelFailed => "tunnel_failed",
            Self::ToolsUnavailable => "tools_unavailable",
            Self::FilesPermissionDenied => "files_permission_denied",
            Self::FilesExists => "files_exists",
            Self::FilesUnsupported => "files_unsupported",
            Self::FilesTooLarge => "files_too_large",
            Self::FilesNotEmpty => "files_not_empty",
        }
    }
}
impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for WorkspaceError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AuthKind {
    Password,
    PrivateKey,
}
impl AuthKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "privateKey",
        }
    }
    pub fn parse(s: &str) -> Result<Self, WorkspaceError> {
        match s {
            "password" => Ok(Self::Password),
            "privateKey" => Ok(Self::PrivateKey),
            _ => Err(WorkspaceError::InvalidInput),
        }
    }
}

pub fn name(value: &str) -> Result<String, WorkspaceError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(value.to_owned())
}
pub fn valid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn next_version(version: u64) -> Result<u64, WorkspaceError> {
    if version >= crate::MAX_REVISION {
        Err(WorkspaceError::RevisionLimit)
    } else {
        Ok(version + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Environment {
    #[default]
    None,
    Development,
    Test,
    Staging,
    Production,
}
impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
    pub fn parse(value: &str) -> Result<Self, WorkspaceError> {
        match value {
            "none" => Ok(Self::None),
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err(WorkspaceError::InvalidInput),
        }
    }
}
#[derive(Clone, Debug)]
pub struct GroupBucket {
    pub group_id: Option<String>,
    pub connection_ids: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct GroupLayout {
    pub revision: u64,
    pub group_ids: Vec<String>,
    pub buckets: Vec<GroupBucket>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalSettings {
    pub encoding: String,
    pub backspace: String,
    pub delete: String,
}
impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            encoding: "utf-8".into(),
            backspace: "del".into(),
            delete: "vt220".into(),
        }
    }
}
impl TerminalSettings {
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if !["utf-8", "gb18030", "gbk", "big5"].contains(&self.encoding.as_str())
            || !["del", "bs"].contains(&self.backspace.as_str())
            || !["vt220", "del"].contains(&self.delete.as_str())
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TunnelRule {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bind_host: String,
    pub listen_port: u16,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub enabled: bool,
}
impl TunnelRule {
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if !valid_id(&self.id)
            || name(&self.name)? != self.name
            || self.listen_port == 0
            || self.bind_host.parse::<std::net::IpAddr>().is_err()
            || !["local", "remote", "socks5"].contains(&self.kind.as_str())
        {
            return Err(WorkspaceError::InvalidInput);
        }
        if self.kind == "socks5" {
            if self.target_host.is_some() || self.target_port.is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
        } else if self.target_port.is_none_or(|p| p == 0)
            || self.target_host.as_ref().is_none_or(|h| !valid_host(h))
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(())
    }
}
pub fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '/' | '\\' | '@' | '?' | '#' | '[' | ']' | '"' | '\'')
        })
        && (!value.contains(':') || value.parse::<std::net::Ipv6Addr>().is_ok())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyRecord {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub version: u64,
    pub credential: Option<String>,
}
impl ProxyRecord {
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if !valid_id(&self.id)
            || name(&self.name)? != self.name
            || !valid_host(&self.host)
            || self.port == 0
            || !["socks5", "http"].contains(&self.kind.as_str())
            || (self.kind == "http" && self.username.contains(':'))
            || self.username.len() > 255
            || self.username.chars().any(char::is_control)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ProxySave {
    pub proxy: ProxyRecord,
    pub expected_version: Option<u64>,
    pub draft_id: Option<String>,
    pub clear_password: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelStatus {
    pub rule_id: String,
    pub phase: String,
    pub error: Option<String>,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionInput {
    pub notes: String,
    pub terminal: Box<TerminalSettings>,
    pub proxy_id: Option<String>,
    pub tunnels: Vec<TunnelRule>,
    pub environment: Environment,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthKind,
}
impl ConnectionInput {
    pub fn normalized(mut self) -> Result<Self, WorkspaceError> {
        self.name = name(&self.name)?;
        if self.notes.len() > 4096
            || self.notes.contains('\0')
            || self.proxy_id.as_ref().is_some_and(|id| !valid_id(id))
            || self.tunnels.len() > 32
        {
            return Err(WorkspaceError::InvalidInput);
        }
        self.terminal.validate()?;
        let mut ids = std::collections::HashSet::new();
        for rule in &self.tunnels {
            rule.validate()?;
            if !ids.insert(&rule.id) {
                return Err(WorkspaceError::InvalidInput);
            }
        }

        let host = self.host.trim();
        self.host = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            if inner.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(WorkspaceError::InvalidInput);
            }
            inner.to_owned()
        } else {
            host.to_owned()
        };
        self.username = self.username.trim().to_owned();
        if self.host.is_empty()
            || self.host.len() > 255
            || self.port == 0
            || self.host.chars().any(|c| {
                c.is_whitespace()
                    || c.is_control()
                    || matches!(c, '/' | '\\' | '@' | '?' | '#' | '[' | ']' | '"' | '\'')
            })
            || self.username.is_empty()
            || self.username.len() > 128
            || self.username.chars().any(char::is_control)
            || (self.host.contains(':') && self.host.parse::<std::net::Ipv6Addr>().is_err())
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(self)
    }
    pub fn same_endpoint(&self, other: &Self) -> bool {
        self.host.eq_ignore_ascii_case(&other.host) && self.port == other.port
    }
    pub fn same_auth_target(&self, other: &Self) -> bool {
        self.same_endpoint(other)
            && self.username == other.username
            && self.auth == other.auth
            && self.terminal == other.terminal
            && self.proxy_id == other.proxy_id
            && self.tunnels == other.tunnels
    }
}
impl fmt::Debug for ConnectionInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionInput")
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionGroup {
    pub sort_order: u64,
    pub id: String,
    pub name: String,
    pub version: u64,
}
#[derive(Clone)]
pub struct ConnectionRecord {
    pub id: String,
    pub group_id: Option<String>,
    pub group_sort_order: u64,
    pub created_at: Option<String>,
    pub version: u64,
    pub revision: u64,
    pub input: ConnectionInput,
    pub credential: Option<String>,
    pub credential_label: Option<String>,
    pub deleting: bool,
    pub credential_pending: bool,
}
impl fmt::Debug for ConnectionRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionRecord")
            .field("id", &self.id)
            .field("version", &self.version)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct HostKey {
    pub public_key: String,
    pub fingerprint: String,
    pub algorithm: String,
}
impl fmt::Debug for HostKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostKey")
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug)]
pub struct HostTrust {
    pub connection_id: String,
    pub host: String,
    pub port: u16,
    pub key: HostKey,
    pub confirmed_at: String,
}
#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    pub version: u64,
    pub groups: Vec<ConnectionGroup>,
    pub layout_revision: u64,
    pub connections: Vec<ConnectionRecord>,
    pub next: Option<u32>,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input(host: &str) -> ConnectionInput {
        ConnectionInput {
            notes: String::new(),
            terminal: Box::default(),
            proxy_id: None,
            tunnels: Vec::new(),
            environment: Environment::None,
            name: " SSH ".into(),
            host: host.into(),
            port: 22,
            username: " user ".into(),
            auth: AuthKind::Password,
        }
    }
    #[test]
    fn normalization_and_route_boundaries() {
        let normalized = input(" [::1] ").normalized().unwrap();
        assert_eq!(normalized.host, "::1");
        assert_eq!(normalized.username, "user");
        for host in [
            "",
            "ssh://example.com",
            "user@host",
            "a b",
            "host:22",
            "bad\nname",
        ] {
            assert!(input(host).normalized().is_err(), "{host}");
        }
        assert!(input("example.com").normalized().is_ok());
        assert_eq!(
            next_version(crate::MAX_REVISION),
            Err(WorkspaceError::RevisionLimit)
        );
    }
    #[test]
    fn debug_does_not_contain_connection_or_secret_text() {
        let value = input("private-host");
        assert!(!format!("{value:?}").contains("private-host"));
    }
}

/// Availability of the shared key, independent of IPC readiness.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionAccessPhase {
    Locked,
    Unlocking,
    Ready,
    Blocked,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionAccessState {
    pub phase: ConnectionAccessPhase,
    pub revision: u64,
    pub code: Option<String>,
}
impl Default for ConnectionAccessState {
    fn default() -> Self {
        Self {
            phase: ConnectionAccessPhase::Locked,
            revision: 0,
            code: None,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionAccessSnapshot {
    pub runtime_id: Option<String>,
    pub state: Option<ConnectionAccessState>,
}
