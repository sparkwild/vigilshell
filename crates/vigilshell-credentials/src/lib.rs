use std::{error::Error, fmt, path::PathBuf, sync::Arc};

use vigilshell_core::{
    connection::CredentialRef,
    human_terminal::{CredentialBroker, CredentialBrokerError, HumanCredentialLease},
};
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// 单个 Human SSH 密码允许进入平台秘密存储的最大字节数。
pub const MAX_PASSWORD_BYTES: usize = 64 * 1024;

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

/// 平台凭据边界可公开的稳定错误。
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

/// 由当前平台安全存储与原生输入界面组成的 Human 密码提供器。
#[derive(Clone)]
pub struct PlatformHumanPasswordCredentialProvider {
    store: Arc<dyn PasswordStore>,
    prompt: Arc<dyn NativePasswordPrompt>,
}

impl PlatformHumanPasswordCredentialProvider {
    /// 创建当前平台的生产凭据提供器。
    ///
    /// macOS 忽略文件根并使用现有 Keychain；Windows 要求传入已经验证的
    /// Current-User DPAPI 密文目录。
    pub fn new(secure_credentials_root: Option<PathBuf>) -> Result<Self, CredentialProviderError> {
        Self::from_system_drivers(secure_credentials_root)
    }

    #[cfg(target_os = "macos")]
    fn from_system_drivers(
        _secure_credentials_root: Option<PathBuf>,
    ) -> Result<Self, CredentialProviderError> {
        Ok(Self {
            store: Arc::new(macos::SystemKeychainStore),
            prompt: Arc::new(macos::AppKitPasswordPrompt),
        })
    }

    #[cfg(windows)]
    fn from_system_drivers(
        secure_credentials_root: Option<PathBuf>,
    ) -> Result<Self, CredentialProviderError> {
        let root = secure_credentials_root.ok_or(CredentialProviderError::Unavailable)?;
        Ok(Self {
            store: Arc::new(windows::WindowsDpapiPasswordStore::new(root)?),
            prompt: Arc::new(windows::WindowsPasswordPrompt),
        })
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn from_system_drivers(
        _secure_credentials_root: Option<PathBuf>,
    ) -> Result<Self, CredentialProviderError> {
        Ok(Self {
            store: Arc::new(UnsupportedStore),
            prompt: Arc::new(UnsupportedPrompt),
        })
    }

    #[cfg(test)]
    fn with_drivers(store: Arc<dyn PasswordStore>, prompt: Arc<dyn NativePasswordPrompt>) -> Self {
        Self { store, prompt }
    }
}

impl fmt::Debug for PlatformHumanPasswordCredentialProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlatformHumanPasswordCredentialProvider")
            .finish_non_exhaustive()
    }
}

impl HumanPasswordCredentialProvider for PlatformHumanPasswordCredentialProvider {
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
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
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

impl CredentialBroker for PlatformHumanPasswordCredentialProvider {
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

#[cfg(not(any(target_os = "macos", windows)))]
struct UnsupportedStore;

#[cfg(not(any(target_os = "macos", windows)))]
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

#[cfg(not(any(target_os = "macos", windows)))]
struct UnsupportedPrompt;

#[cfg(not(any(target_os = "macos", windows)))]
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
