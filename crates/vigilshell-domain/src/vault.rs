//! 密码库内部合同；这些类型不进入桌面协议。
use crate::workspace::{AuthKind, WorkspaceError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialPurpose {
    SshPassword,
    SshPrivateKey,
    ProxyPassword,
}
impl CredentialPurpose {
    pub fn code(self) -> &'static str {
        match self {
            Self::SshPassword => "ssh_password",
            Self::SshPrivateKey => "ssh_private_key",
            Self::ProxyPassword => "proxy_password",
        }
    }
    pub fn kind(self) -> AuthKind {
        match self {
            Self::SshPrivateKey => AuthKind::PrivateKey,
            _ => AuthKind::Password,
        }
    }
    pub fn parse(value: &str) -> Result<Self, WorkspaceError> {
        match value {
            "ssh_password" => Ok(Self::SshPassword),
            "ssh_private_key" => Ok(Self::SshPrivateKey),
            "proxy_password" => Ok(Self::ProxyPassword),
            _ => Err(WorkspaceError::CredentialCorrupt),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialContext {
    pub owner_id: String,
    pub purpose: CredentialPurpose,
}
impl CredentialContext {
    pub fn login(owner_id: &str, kind: AuthKind) -> Self {
        Self {
            owner_id: owner_id.into(),
            purpose: match kind {
                AuthKind::Password => CredentialPurpose::SshPassword,
                AuthKind::PrivateKey => CredentialPurpose::SshPrivateKey,
            },
        }
    }
    pub fn proxy(owner_id: &str) -> Self {
        Self {
            owner_id: owner_id.into(),
            purpose: CredentialPurpose::ProxyPassword,
        }
    }
}
