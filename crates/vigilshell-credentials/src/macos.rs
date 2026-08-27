use std::fmt;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSSecureTextField};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use security_framework::{
    base::Error as SecurityFrameworkError,
    item::{ItemClass, ItemSearchOptions, SearchResult},
    passwords::{
        PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    },
};
use security_framework_sys::base::{
    errSecAuthFailed as ERR_SEC_AUTH_FAILED, errSecItemNotFound as ERR_SEC_ITEM_NOT_FOUND,
};
use vigilshell_core::connection::CredentialRef;
use zeroize::Zeroizing;

use crate::{
    CredentialProviderError, KEYCHAIN_SERVICE, MAX_PASSWORD_BYTES, NativePasswordPrompt,
    PasswordConfigurationStatus, PasswordPromptSettlement, PasswordStore,
};

const KEYCHAIN_ITEM_LABEL: &str = "VigilShell SSH password";
const KEYCHAIN_ITEM_DESCRIPTION: &str = "Human SSH password managed by VigilShell";

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeychainQuery<'a> {
    service: &'static str,
    account: &'a str,
    use_data_protection: bool,
    synchronized: bool,
}

impl fmt::Debug for KeychainQuery<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeychainQuery")
            .field("service", &self.service)
            .field("account", &"[opaque]")
            .field("use_data_protection", &self.use_data_protection)
            .field("synchronized", &self.synchronized)
            .finish()
    }
}

impl<'a> KeychainQuery<'a> {
    fn for_credential(credential_ref: &'a CredentialRef) -> Self {
        Self {
            service: KEYCHAIN_SERVICE,
            account: credential_ref.as_str(),
            use_data_protection: true,
            synchronized: false,
        }
    }

    fn identity_options(self) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(self.service, self.account);
        if self.use_data_protection {
            options.use_protected_keychain();
        }
        options.set_access_synchronized(Some(self.synchronized));
        options
    }

    fn set_options(self) -> PasswordOptions {
        let mut options = self.identity_options();
        options.set_label(KEYCHAIN_ITEM_LABEL);
        options.set_description(KEYCHAIN_ITEM_DESCRIPTION);
        options
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeychainItemKind {
    GenericPassword,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeychainExistenceQuery<'a> {
    identity: KeychainQuery<'a>,
    item_kind: KeychainItemKind,
    limit: i64,
    return_data: bool,
    return_attributes: bool,
    return_refs: bool,
}

impl<'a> KeychainExistenceQuery<'a> {
    fn for_credential(credential_ref: &'a CredentialRef) -> Self {
        Self {
            identity: KeychainQuery::for_credential(credential_ref),
            item_kind: KeychainItemKind::GenericPassword,
            limit: 1,
            return_data: false,
            return_attributes: false,
            return_refs: false,
        }
    }

    fn search(self) -> Result<Vec<SearchResult>, SecurityFrameworkError> {
        let mut options = ItemSearchOptions::new();
        match self.item_kind {
            KeychainItemKind::GenericPassword => options.class(ItemClass::generic_password()),
        };
        options
            .service(self.identity.service)
            .account(self.identity.account)
            .cloud_sync(Some(self.identity.synchronized))
            .limit(self.limit)
            .load_data(self.return_data)
            .load_attributes(self.return_attributes)
            .load_refs(self.return_refs);
        if self.identity.use_data_protection {
            options.ignore_legacy_keychains();
        }

        options.search()
    }
}

pub(crate) struct SystemKeychainStore;

impl PasswordStore for SystemKeychainStore {
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        map_existence_search(KeychainExistenceQuery::for_credential(credential_ref).search())
    }

    fn set_password(
        &self,
        credential_ref: &CredentialRef,
        password: &[u8],
    ) -> Result<(), CredentialProviderError> {
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
            return Err(CredentialProviderError::InvalidSecret);
        }

        set_generic_password_options(
            password,
            KeychainQuery::for_credential(credential_ref).set_options(),
        )
        .map_err(map_keychain_error)
    }

    fn password(&self, credential_ref: &CredentialRef) -> Result<Vec<u8>, CredentialProviderError> {
        let password =
            generic_password(KeychainQuery::for_credential(credential_ref).identity_options())
                .map_err(map_keychain_error)?;
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
            return Err(CredentialProviderError::InvalidSecret);
        }
        Ok(password)
    }

    fn remove_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), CredentialProviderError> {
        delete_generic_password_options(
            KeychainQuery::for_credential(credential_ref).identity_options(),
        )
        .map_err(map_keychain_error)
    }
}

fn map_keychain_error(error: SecurityFrameworkError) -> CredentialProviderError {
    map_keychain_status(error.code())
}

fn map_existence_search(
    result: Result<Vec<SearchResult>, SecurityFrameworkError>,
) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
    match result {
        Ok(_) => Ok(PasswordConfigurationStatus::Configured),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => {
            Ok(PasswordConfigurationStatus::NotConfigured)
        }
        Err(error) => Err(map_keychain_error(error)),
    }
}

fn map_keychain_status(status: i32) -> CredentialProviderError {
    match status {
        ERR_SEC_ITEM_NOT_FOUND => CredentialProviderError::Missing,
        ERR_SEC_AUTH_FAILED => CredentialProviderError::AccessDenied,
        _ => CredentialProviderError::Unavailable,
    }
}

pub(crate) struct AppKitPasswordPrompt;

impl NativePasswordPrompt for AppKitPasswordPrompt {
    fn capture_password_on_main_thread(
        &self,
    ) -> Result<PasswordPromptSettlement, CredentialProviderError> {
        let mtm = MainThreadMarker::new().ok_or(CredentialProviderError::MainThreadRequired)?;
        let alert = password_alert(mtm);
        let field = NSSecureTextField::new(mtm);
        field.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(320.0, 24.0)));
        field.setPlaceholderString(Some(&NSString::from_str("SSH password")));
        alert.setAccessoryView(Some(&field));
        alert.layout();
        alert.window().makeFirstResponder(Some(&field));

        if alert.runModal() != NSAlertFirstButtonReturn {
            clear_secure_field(&field);
            return Ok(PasswordPromptSettlement::Cancelled);
        }

        let password = Zeroizing::new(field.stringValue().to_string().into_bytes());
        clear_secure_field(&field);
        if password.is_empty() {
            return Err(CredentialProviderError::InvalidSecret);
        }
        Ok(PasswordPromptSettlement::Submitted(password))
    }

    fn confirm_removal_on_main_thread(&self) -> Result<bool, CredentialProviderError> {
        let mtm = MainThreadMarker::new().ok_or(CredentialProviderError::MainThreadRequired)?;
        let alert = NSAlert::new(mtm);
        alert.setAlertStyle(NSAlertStyle::Warning);
        alert.setMessageText(&NSString::from_str("Remove saved SSH password?"));
        alert.setInformativeText(&NSString::from_str(
            "VigilShell will remove this password from the macOS Keychain.",
        ));
        alert.addButtonWithTitle(&NSString::from_str("Remove Password"));
        alert.addButtonWithTitle(&NSString::from_str("Cancel"));
        Ok(alert.runModal() == NSAlertFirstButtonReturn)
    }
}

fn password_alert(mtm: MainThreadMarker) -> objc2::rc::Retained<NSAlert> {
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Informational);
    alert.setMessageText(&NSString::from_str("Configure SSH password"));
    alert.setInformativeText(&NSString::from_str(
        "The password is saved directly to the macOS Keychain and is never sent to the app interface.",
    ));
    alert.addButtonWithTitle(&NSString::from_str("Save Password"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert
}

fn clear_secure_field(field: &NSSecureTextField) {
    field.setStringValue(&NSString::from_str(""));
}

#[cfg(test)]
mod tests {
    use security_framework_sys::base::{errSecAuthFailed, errSecItemNotFound};
    use vigilshell_core::connection::CredentialRef;

    use security_framework::base::Error as SecurityFrameworkError;

    use super::{
        KeychainExistenceQuery, KeychainItemKind, KeychainQuery, map_existence_search,
        map_keychain_status,
    };
    use crate::{CredentialProviderError, KEYCHAIN_SERVICE, PasswordConfigurationStatus};

    #[test]
    fn keychain_query_uses_only_the_fixed_service_and_opaque_reference() {
        let credential_ref = CredentialRef::parse("credential-opaque-17").expect("valid reference");
        let query = KeychainQuery::for_credential(&credential_ref);

        assert_eq!(query.service, KEYCHAIN_SERVICE);
        assert_eq!(query.account, "credential-opaque-17");
        assert!(query.use_data_protection);
        assert!(!query.synchronized);
        assert!(!format!("{query:?}").contains("credential-opaque-17"));
    }

    #[test]
    fn existence_query_freezes_metadata_only_single_item_contract() {
        let credential_ref = CredentialRef::parse("credential-opaque-17").expect("valid reference");
        let query = KeychainExistenceQuery::for_credential(&credential_ref);

        assert_eq!(query.item_kind, KeychainItemKind::GenericPassword);
        assert_eq!(query.identity.service, KEYCHAIN_SERVICE);
        assert_eq!(query.identity.account, "credential-opaque-17");
        assert!(query.identity.use_data_protection);
        assert!(!query.identity.synchronized);
        assert_eq!(query.limit, 1);
        assert!(!query.return_data);
        assert!(!query.return_attributes);
        assert!(!query.return_refs);
    }

    #[test]
    fn keychain_statuses_map_to_stable_redacted_errors() {
        assert_eq!(
            map_keychain_status(errSecItemNotFound),
            CredentialProviderError::Missing
        );
        assert_eq!(
            map_keychain_status(errSecAuthFailed),
            CredentialProviderError::AccessDenied
        );
        assert_eq!(
            map_keychain_status(-1),
            CredentialProviderError::Unavailable
        );
    }

    #[test]
    fn metadata_existence_search_maps_only_presence_and_safe_failures() {
        assert_eq!(
            map_existence_search(Ok(Vec::new())),
            Ok(PasswordConfigurationStatus::Configured)
        );
        assert_eq!(
            map_existence_search(Err(SecurityFrameworkError::from_code(errSecItemNotFound))),
            Ok(PasswordConfigurationStatus::NotConfigured)
        );
        assert_eq!(
            map_existence_search(Err(SecurityFrameworkError::from_code(errSecAuthFailed))),
            Err(CredentialProviderError::AccessDenied)
        );
        assert_eq!(
            map_existence_search(Err(SecurityFrameworkError::from_code(-1))),
            Err(CredentialProviderError::Unavailable)
        );
    }
}
