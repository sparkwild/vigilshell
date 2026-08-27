use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use vigilshell_core::{connection::CredentialRef, human_terminal::CredentialBroker};
use zeroize::Zeroizing;

use crate::{
    ConfigurePasswordSettlement, CredentialProviderError, HumanPasswordCredentialProvider,
    NativePasswordPrompt, PasswordConfigurationStatus, PasswordPromptSettlement, PasswordStore,
    PlatformHumanPasswordCredentialProvider, RemovePasswordSettlement,
};

#[derive(Default)]
struct FakePasswordStore {
    passwords: Mutex<HashMap<String, Vec<u8>>>,
    set_error: Mutex<Option<CredentialProviderError>>,
    status_error: Mutex<Option<CredentialProviderError>>,
    remove_error: Mutex<Option<CredentialProviderError>>,
}

impl FakePasswordStore {
    fn password_copy(&self, credential_ref: &CredentialRef) -> Option<Vec<u8>> {
        self.passwords
            .lock()
            .expect("password store lock")
            .get(credential_ref.as_str())
            .cloned()
    }

    fn fail_next_set_with(&self, error: CredentialProviderError) {
        *self.set_error.lock().expect("set error lock") = Some(error);
    }

    fn fail_next_status_with(&self, error: CredentialProviderError) {
        *self.status_error.lock().expect("status error lock") = Some(error);
    }

    fn fail_next_remove_with(&self, error: CredentialProviderError) {
        *self.remove_error.lock().expect("remove error lock") = Some(error);
    }
}

impl PasswordStore for FakePasswordStore {
    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        if let Some(error) = self.status_error.lock().expect("status error lock").take() {
            return Err(error);
        }

        Ok(if self.password_copy(credential_ref).is_some() {
            PasswordConfigurationStatus::Configured
        } else {
            PasswordConfigurationStatus::NotConfigured
        })
    }

    fn set_password(
        &self,
        credential_ref: &CredentialRef,
        password: &[u8],
    ) -> Result<(), CredentialProviderError> {
        if let Some(error) = self.set_error.lock().expect("set error lock").take() {
            return Err(error);
        }
        self.passwords
            .lock()
            .expect("password store lock")
            .insert(credential_ref.as_str().to_owned(), password.to_vec());
        Ok(())
    }

    fn password(&self, credential_ref: &CredentialRef) -> Result<Vec<u8>, CredentialProviderError> {
        self.password_copy(credential_ref)
            .ok_or(CredentialProviderError::Missing)
    }

    fn remove_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<(), CredentialProviderError> {
        if let Some(error) = self.remove_error.lock().expect("remove error lock").take() {
            return Err(error);
        }
        self.passwords
            .lock()
            .expect("password store lock")
            .remove(credential_ref.as_str())
            .map(|_| ())
            .ok_or(CredentialProviderError::Missing)
    }
}

#[derive(Default)]
struct FakePrompt {
    passwords: Mutex<VecDeque<Result<PasswordPromptSettlement, CredentialProviderError>>>,
    removals: Mutex<VecDeque<Result<bool, CredentialProviderError>>>,
}

impl FakePrompt {
    fn submit(&self, password: Vec<u8>) {
        self.passwords
            .lock()
            .expect("password prompt lock")
            .push_back(Ok(PasswordPromptSettlement::Submitted(Zeroizing::new(
                password,
            ))));
    }

    fn cancel_password(&self) {
        self.passwords
            .lock()
            .expect("password prompt lock")
            .push_back(Ok(PasswordPromptSettlement::Cancelled));
    }

    fn fail_password_with(&self, error: CredentialProviderError) {
        self.passwords
            .lock()
            .expect("password prompt lock")
            .push_back(Err(error));
    }

    fn confirm_removal(&self, confirmed: bool) {
        self.removals
            .lock()
            .expect("removal prompt lock")
            .push_back(Ok(confirmed));
    }
}

impl NativePasswordPrompt for FakePrompt {
    fn capture_password_on_main_thread(
        &self,
    ) -> Result<PasswordPromptSettlement, CredentialProviderError> {
        self.passwords
            .lock()
            .expect("password prompt lock")
            .pop_front()
            .unwrap_or(Err(CredentialProviderError::Unavailable))
    }

    fn confirm_removal_on_main_thread(&self) -> Result<bool, CredentialProviderError> {
        self.removals
            .lock()
            .expect("removal prompt lock")
            .pop_front()
            .unwrap_or(Err(CredentialProviderError::Unavailable))
    }
}

fn fixture_reference() -> CredentialRef {
    CredentialRef::parse("credential-fixture").expect("valid fixture reference")
}

fn fixture_provider(
    store: Arc<FakePasswordStore>,
    prompt: Arc<FakePrompt>,
) -> PlatformHumanPasswordCredentialProvider {
    PlatformHumanPasswordCredentialProvider::with_drivers(store, prompt)
}

#[test]
fn configure_creates_and_updates_the_same_opaque_item() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt.clone());
    let credential_ref = fixture_reference();
    prompt.submit(vec![0x61; 12]);
    prompt.submit(vec![0x62; 18]);

    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Ok(ConfigurePasswordSettlement::Configured)
    );
    assert_eq!(store.password_copy(&credential_ref), Some(vec![0x61; 12]));

    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Ok(ConfigurePasswordSettlement::Configured)
    );
    assert_eq!(store.password_copy(&credential_ref), Some(vec![0x62; 18]));
    assert_eq!(
        store.passwords.lock().expect("password store lock").len(),
        1
    );
}

#[test]
fn status_query_reports_presence_without_reading_a_password() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt);
    let credential_ref = fixture_reference();

    assert_eq!(
        provider.password_configuration_status(&credential_ref),
        Ok(PasswordConfigurationStatus::NotConfigured)
    );

    store
        .set_password(&credential_ref, &[0x61; 12])
        .expect("seed fake store");
    assert_eq!(
        provider.password_configuration_status(&credential_ref),
        Ok(PasswordConfigurationStatus::Configured)
    );

    store.fail_next_status_with(CredentialProviderError::AccessDenied);
    assert_eq!(
        provider.password_configuration_status(&credential_ref),
        Err(CredentialProviderError::AccessDenied)
    );
    store.fail_next_status_with(CredentialProviderError::Missing);
    assert_eq!(
        provider.password_configuration_status(&credential_ref),
        Ok(PasswordConfigurationStatus::NotConfigured)
    );
    store.fail_next_status_with(CredentialProviderError::Unavailable);
    assert_eq!(
        provider.password_configuration_status(&credential_ref),
        Err(CredentialProviderError::Unavailable)
    );
}

#[test]
fn configure_cancel_is_a_non_error_and_does_not_write() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt.clone());
    let credential_ref = fixture_reference();
    prompt.cancel_password();

    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Ok(ConfigurePasswordSettlement::Cancelled)
    );
    assert_eq!(store.password_copy(&credential_ref), None);
}

#[test]
fn configure_rejects_empty_password_and_preserves_stable_errors() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt.clone());
    let credential_ref = fixture_reference();
    prompt.submit(Vec::new());

    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Err(CredentialProviderError::InvalidSecret)
    );

    prompt.fail_password_with(CredentialProviderError::MainThreadRequired);
    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Err(CredentialProviderError::MainThreadRequired)
    );

    prompt.submit(vec![0x63; 8]);
    store.fail_next_set_with(CredentialProviderError::AccessDenied);
    assert_eq!(
        provider.configure_password_on_main_thread(&credential_ref),
        Err(CredentialProviderError::AccessDenied)
    );
}

#[test]
fn remove_requires_confirmation_and_settles_idempotently() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt.clone());
    let credential_ref = fixture_reference();
    store
        .set_password(&credential_ref, &[0x64; 14])
        .expect("seed fake store");

    prompt.confirm_removal(false);
    assert_eq!(
        provider.remove_password_on_main_thread(&credential_ref),
        Ok(RemovePasswordSettlement::Cancelled)
    );
    assert!(store.password_copy(&credential_ref).is_some());

    prompt.confirm_removal(true);
    assert_eq!(
        provider.remove_password_on_main_thread(&credential_ref),
        Ok(RemovePasswordSettlement::Removed)
    );
    assert_eq!(store.password_copy(&credential_ref), None);

    prompt.confirm_removal(true);
    assert_eq!(
        provider.remove_password_on_main_thread(&credential_ref),
        Ok(RemovePasswordSettlement::AlreadyMissing)
    );
}

#[test]
fn remove_preserves_access_failure_without_platform_detail() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt.clone());
    let credential_ref = fixture_reference();
    prompt.confirm_removal(true);
    store.fail_next_remove_with(CredentialProviderError::AccessDenied);

    assert_eq!(
        provider.remove_password_on_main_thread(&credential_ref),
        Err(CredentialProviderError::AccessDenied)
    );
}

#[test]
fn confirmed_connection_delete_removes_without_opening_another_prompt() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt);
    let credential_ref = fixture_reference();
    store
        .set_password(&credential_ref, &[0x66; 16])
        .expect("seed fake store");

    assert_eq!(
        provider.remove_password_after_confirmation(&credential_ref),
        Ok(RemovePasswordSettlement::Removed)
    );
    assert_eq!(store.password_copy(&credential_ref), None);
    assert_eq!(
        provider.remove_password_after_confirmation(&credential_ref),
        Ok(RemovePasswordSettlement::AlreadyMissing)
    );
}

#[test]
fn broker_returns_a_password_lease_and_fails_closed_when_missing() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store.clone(), prompt);
    let credential_ref = fixture_reference();
    let expected = vec![0x65; 24];
    store
        .set_password(&credential_ref, &expected)
        .expect("seed fake store");

    let lease = provider
        .lease_for_human_terminal(&credential_ref)
        .expect("credential lease");
    assert_eq!(lease.password(), expected);

    let missing_ref = CredentialRef::parse("credential-missing").expect("valid missing reference");
    assert!(provider.lease_for_human_terminal(&missing_ref).is_err());
}

#[test]
fn debug_and_display_are_redacted_and_provider_is_object_safe() {
    let store = Arc::new(FakePasswordStore::default());
    let prompt = Arc::new(FakePrompt::default());
    let provider = fixture_provider(store, prompt);
    let provider_debug = format!("{provider:?}");
    assert_eq!(
        provider_debug,
        "PlatformHumanPasswordCredentialProvider { .. }"
    );

    let platform_detail = "credential-fixture";
    for error in [
        CredentialProviderError::Missing,
        CredentialProviderError::AccessDenied,
        CredentialProviderError::Unavailable,
        CredentialProviderError::MainThreadRequired,
        CredentialProviderError::InvalidSecret,
    ] {
        assert!(!format!("{error:?}").contains(platform_detail));
        assert!(!error.to_string().contains(platform_detail));
    }

    let object_safe: Box<dyn HumanPasswordCredentialProvider> = Box::new(provider.clone());
    drop(object_safe);
    assert_send_sync::<PlatformHumanPasswordCredentialProvider>();
}

fn assert_send_sync<T: Send + Sync>() {}
