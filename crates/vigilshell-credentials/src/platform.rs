use crate::key_store::VaultKeyStore;
use vigilshell_domain::workspace::*;
use zeroize::Zeroizing;

pub struct PlatformVaultKeyStore {
    #[cfg(any(target_os = "macos", test))]
    development: bool,
}
impl PlatformVaultKeyStore {
    pub fn new(development: bool) -> Self {
        #[cfg(not(any(target_os = "macos", test)))]
        let _ = development;
        Self {
            #[cfg(any(target_os = "macos", test))]
            development,
        }
    }
    #[cfg(any(target_os = "macos", test))]
    fn label(&self) -> &'static str {
        // Only used at creation; existing item labels and access controls stay unchanged.
        if self.development {
            "VigilShell Dev Data Encryption Key"
        } else {
            "VigilShell Data Encryption Key"
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use core_foundation::{
        base::TCFType, boolean::CFBoolean, data::CFData, dictionary::CFDictionary, string::CFString,
    };
    use security_framework::passwords::{PasswordOptions, generic_password};
    use security_framework_sys::{base::errSecDuplicateItem, item::*, keychain_item::SecItemAdd};

    const SERVICE: &str = "com.sparkwild.vigilshell.vault";
    fn account(namespace: &str, version: u32) -> String {
        format!("{namespace}/key/{version}")
    }
    fn error(code: i32) -> WorkspaceError {
        match code {
            -25300 => WorkspaceError::VaultKeyMissing,
            -25293 | -128 => WorkspaceError::CredentialDenied,
            _ => WorkspaceError::CredentialUnavailable,
        }
    }
    impl VaultKeyStore for PlatformVaultKeyStore {
        fn read(
            &self,
            namespace: &str,
            version: u32,
        ) -> Result<Zeroizing<Vec<u8>>, WorkspaceError> {
            tracing::info!(
                application = "vigilshell",
                pid = std::process::id(),
                stage = "vault_key_read",
                outcome = "started"
            );
            let mut options =
                PasswordOptions::new_generic_password(SERVICE, &account(namespace, version));
            options.set_access_synchronized(Some(false));
            let result = generic_password(options)
                .map(Zeroizing::new)
                .map_err(|e| error(e.code()));
            tracing::info!(
                application = "vigilshell",
                pid = std::process::id(),
                stage = "vault_key_read",
                outcome = if result.is_ok() { "ok" } else { "failed" },
                code = result.as_ref().err().map(|e| e.code()).unwrap_or("ok")
            );
            result
        }
        fn create(
            &self,
            namespace: &str,
            version: u32,
            key: &[u8],
        ) -> Result<bool, WorkspaceError> {
            if key.len() != 32 {
                return Err(WorkspaceError::VaultKeyInvalid);
            }
            // SAFETY: Security.framework attribute constants have process lifetime; all values
            // are retained by the dictionary throughout this synchronous SecItemAdd call.
            let status = unsafe {
                let attributes = CFDictionary::from_CFType_pairs(&[
                    (
                        CFString::wrap_under_get_rule(kSecClass).as_CFType(),
                        CFString::wrap_under_get_rule(kSecClassGenericPassword).as_CFType(),
                    ),
                    (
                        CFString::wrap_under_get_rule(kSecAttrService).as_CFType(),
                        CFString::new(SERVICE).as_CFType(),
                    ),
                    (
                        CFString::wrap_under_get_rule(kSecAttrAccount).as_CFType(),
                        CFString::new(&account(namespace, version)).as_CFType(),
                    ),
                    (
                        CFString::wrap_under_get_rule(kSecAttrLabel).as_CFType(),
                        CFString::new(self.label()).as_CFType(),
                    ),
                    (
                        CFString::wrap_under_get_rule(kSecAttrSynchronizable).as_CFType(),
                        CFBoolean::false_value().as_CFType(),
                    ),
                    (
                        CFString::wrap_under_get_rule(kSecValueData).as_CFType(),
                        CFData::from_buffer(key).as_CFType(),
                    ),
                ]);
                SecItemAdd(attributes.as_concrete_TypeRef(), std::ptr::null_mut())
            };
            match status {
                0 => Ok(true),
                n if n == errSecDuplicateItem => Ok(false),
                n => Err(error(n)),
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl VaultKeyStore for PlatformVaultKeyStore {
    fn read(&self, _: &str, _: u32) -> Result<Zeroizing<Vec<u8>>, WorkspaceError> {
        Err(WorkspaceError::NativeUnavailable)
    }
    fn create(&self, _: &str, _: u32, _: &[u8]) -> Result<bool, WorkspaceError> {
        Err(WorkspaceError::NativeUnavailable)
    }
}
#[cfg(test)]
mod presentation_tests {
    use super::*;
    #[test]
    fn fixed_names_keep_channels_distinct() {
        let dev = PlatformVaultKeyStore::new(true);
        let release = PlatformVaultKeyStore::new(false);
        assert_eq!(dev.label(), "VigilShell Dev Data Encryption Key");
        assert_eq!(release.label(), "VigilShell Data Encryption Key");
    }
}
