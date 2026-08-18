use std::{error::Error, fmt, sync::Arc};

use vigilshell_core::{
    connection::CredentialRef,
    human_terminal::{CredentialBroker, CredentialBrokerError, HumanCredentialLease},
};
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
mod platform;

/// VigilShell 密码条目的固定 Keychain service。
pub const KEYCHAIN_SERVICE: &str = "com.sparkwild.vigilshell.credentials";

/// 原生密码配置动作的安全结算结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurePasswordSettlement {
    Configured,
    Cancelled,
}

/// 密码条目是否已在平台凭据存储中配置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordConfigurationStatus {
    Configured,
    NotConfigured,
}

/// 原生密码移除动作的安全结算结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovePasswordSettlement {
    Removed,
    AlreadyMissing,
    Cancelled,
}

/// macOS 凭据边界可公开的稳定错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialProviderError {
    Missing,
    AccessDenied,
    Unavailable,
    MainThreadRequired,
    InvalidSecret,
}

impl fmt::Display for CredentialProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("credential is not configured"),
            Self::AccessDenied => formatter.write_str("credential access is denied"),
            Self::Unavailable => formatter.write_str("credential provider is unavailable"),
            Self::MainThreadRequired => {
                formatter.write_str("credential prompt requires the main thread")
            }
            Self::InvalidSecret => formatter.write_str("credential secret is invalid"),
        }
    }
}

impl Error for CredentialProviderError {}

/// Tauri 适配层可替换的 Human 密码配置端口。
pub trait HumanPasswordCredentialProvider: Send + Sync {
    /// 仅查询密码条目是否存在，不读取密码内容。
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError>;

    fn configure_password_on_main_thread(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<ConfigurePasswordSettlement, CredentialProviderError>;

    fn remove_password_on_main_thread(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError>;

    /// 在调用方已经取得 Human 删除确认后移除密码，不再打开第二个确认框。
    fn remove_password_after_confirmation(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError>;
}

trait PasswordStore: Send + Sync {
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError>;

    fn set_password(
        &self,
        credential_ref: &CredentialRef,
        password: &[u8],
    ) -> Result<(), CredentialProviderError>;

    fn password(&self, credential_ref: &CredentialRef) -> Result<Vec<u8>, CredentialProviderError>;

    fn remove_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), CredentialProviderError>;
}

enum PasswordPromptSettlement {
    Submitted(Zeroizing<Vec<u8>>),
    Cancelled,
}

trait NativePasswordPrompt: Send + Sync {
    fn capture_password_on_main_thread(
        &self,
    ) -> Result<PasswordPromptSettlement, CredentialProviderError>;

    fn confirm_removal_on_main_thread(&self) -> Result<bool, CredentialProviderError>;
}

/// Security.framework 与 AppKit 组成的 macOS Human 密码提供器。
#[derive(Clone)]
pub struct MacOsHumanPasswordCredentialProvider {
    store: Arc<dyn PasswordStore>,
    prompt: Arc<dyn NativePasswordPrompt>,
}

impl MacOsHumanPasswordCredentialProvider {
    /// 创建使用系统 data-protection Keychain 与原生安全输入框的提供器。
    pub fn new() -> Self {
        Self::from_system_drivers()
    }

    #[cfg(target_os = "macos")]
    fn from_system_drivers() -> Self {
        Self {
            store: Arc::new(platform::SystemKeychainStore),
            prompt: Arc::new(platform::AppKitPasswordPrompt),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn from_system_drivers() -> Self {
        Self {
            store: Arc::new(UnsupportedStore),
            prompt: Arc::new(UnsupportedPrompt),
        }
    }

    #[cfg(test)]
    fn with_drivers(store: Arc<dyn PasswordStore>, prompt: Arc<dyn NativePasswordPrompt>) -> Self {
        Self { store, prompt }
    }
}

impl Default for MacOsHumanPasswordCredentialProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MacOsHumanPasswordCredentialProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacOsHumanPasswordCredentialProvider")
            .finish_non_exhaustive()
    }
}

impl HumanPasswordCredentialProvider for MacOsHumanPasswordCredentialProvider {
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        match self.store.password_configuration_status(credential_ref) {
            Err(CredentialProviderError::Missing) => Ok(PasswordConfigurationStatus::NotConfigured),
            result => result,
        }
    }

    fn configure_password_on_main_thread(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<ConfigurePasswordSettlement, CredentialProviderError> {
        let password = match self.prompt.capture_password_on_main_thread()? {
            PasswordPromptSettlement::Submitted(password) => password,
            PasswordPromptSettlement::Cancelled => {
                return Ok(ConfigurePasswordSettlement::Cancelled);
            }
        };
        if password.is_empty() {
            return Err(CredentialProviderError::InvalidSecret);
        }

        self.store.set_password(credential_ref, &password)?;
        Ok(ConfigurePasswordSettlement::Configured)
    }

    fn remove_password_on_main_thread(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
        if !self.prompt.confirm_removal_on_main_thread()? {
            return Ok(RemovePasswordSettlement::Cancelled);
        }

        match self.store.remove_password(credential_ref) {
            Ok(()) => Ok(RemovePasswordSettlement::Removed),
            Err(CredentialProviderError::Missing) => Ok(RemovePasswordSettlement::AlreadyMissing),
            Err(error) => Err(error),
        }
    }

    fn remove_password_after_confirmation(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
        match self.store.remove_password(credential_ref) {
            Ok(()) => Ok(RemovePasswordSettlement::Removed),
            Err(CredentialProviderError::Missing) => Ok(RemovePasswordSettlement::AlreadyMissing),
            Err(error) => Err(error),
        }
    }
}

impl CredentialBroker for MacOsHumanPasswordCredentialProvider {
    fn lease_for_human_terminal(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError> {
        let password = self
            .store
            .password(credential_ref)
            .map_err(|_| CredentialBrokerError::Unavailable)?;
        HumanCredentialLease::from_password(password)
            .map_err(|_| CredentialBrokerError::Unavailable)
    }
}

#[cfg(not(target_os = "macos"))]
struct UnsupportedStore;

#[cfg(not(target_os = "macos"))]
impl PasswordStore for UnsupportedStore {
    fn password_configuration_status(
        &self,
        _credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }

    fn set_password(
        &self,
        _credential_ref: &CredentialRef,
        _password: &[u8],
    ) -> Result<(), CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }

    fn password(
        &self,
        _credential_ref: &CredentialRef,
    ) -> Result<Vec<u8>, CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }

    fn remove_password(
        &self,
        _credential_ref: &CredentialRef,
    ) -> Result<(), CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }
}

#[cfg(not(target_os = "macos"))]
struct UnsupportedPrompt;

#[cfg(not(target_os = "macos"))]
impl NativePasswordPrompt for UnsupportedPrompt {
    fn capture_password_on_main_thread(
        &self,
    ) -> Result<PasswordPromptSettlement, CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }

    fn confirm_removal_on_main_thread(&self) -> Result<bool, CredentialProviderError> {
        Err(CredentialProviderError::Unavailable)
    }
}

#[cfg(test)]
mod tests;
