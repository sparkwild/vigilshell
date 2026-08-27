use std::{fmt, path::PathBuf};

#[cfg(debug_assertions)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[cfg(debug_assertions)]
use uuid::Uuid;
use vigilshell_core::{
    connection::CredentialRef,
    human_terminal::{CredentialBroker, CredentialBrokerError, HumanCredentialLease},
};
use vigilshell_credentials::{
    ConfigurePasswordSettlement, CredentialProviderError, HumanPasswordCredentialProvider,
    PasswordConfigurationStatus, PlatformHumanPasswordCredentialProvider, RemovePasswordSettlement,
};

#[derive(Clone)]
pub(crate) struct HumanCredentialRuntime<P = PlatformHumanPasswordCredentialProvider> {
    production: P,
    #[cfg(debug_assertions)]
    development_fixtures: Option<DevelopmentFixtureCredentials>,
}

impl HumanCredentialRuntime {
    pub(crate) fn production(
        secure_credentials_root: Option<PathBuf>,
    ) -> Result<Self, CredentialProviderError> {
        Ok(Self::from_production_provider(
            PlatformHumanPasswordCredentialProvider::new(secure_credentials_root)?,
        ))
    }

    #[cfg(debug_assertions)]
    pub(crate) fn development(
        secure_credentials_root: Option<PathBuf>,
        fixture_refs: impl IntoIterator<Item = CredentialRef>,
    ) -> Result<Self, CredentialProviderError> {
        Ok(Self::with_development_provider(
            PlatformHumanPasswordCredentialProvider::new(secure_credentials_root)?,
            fixture_refs,
        ))
    }
}

impl<P> HumanCredentialRuntime<P> {
    fn from_production_provider(production: P) -> Self {
        Self {
            production,
            #[cfg(debug_assertions)]
            development_fixtures: None,
        }
    }

    #[cfg(debug_assertions)]
    fn with_development_provider(
        production: P,
        fixture_refs: impl IntoIterator<Item = CredentialRef>,
    ) -> Self {
        Self {
            production,
            development_fixtures: Some(DevelopmentFixtureCredentials::new(fixture_refs)),
        }
    }

    pub(crate) fn for_human_terminal(
        &self,
        route: CredentialRoute,
    ) -> HumanTerminalCredentialBroker<'_, P> {
        HumanTerminalCredentialBroker {
            runtime: self,
            route,
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn fixture_enabled(&self) -> bool {
        self.development_fixtures.is_some()
    }
}

impl<P> fmt::Debug for HumanCredentialRuntime<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut runtime = formatter.debug_struct("HumanCredentialRuntime");
        runtime.field("production", &"[redacted adapter]");
        #[cfg(debug_assertions)]
        runtime.field("development_fixtures", &self.development_fixtures.is_some());
        runtime.finish()
    }
}

impl<P> HumanCredentialRuntime<P>
where
    P: HumanPasswordCredentialProvider,
{
    pub(crate) fn password_configuration_status(
        &self,
        route: CredentialRoute,
        credential_ref: &CredentialRef,
    ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
        match route {
            CredentialRoute::Production => self
                .production
                .password_configuration_status(credential_ref),
            #[cfg(debug_assertions)]
            CredentialRoute::Fixture => self
                .development_fixtures
                .as_ref()
                .ok_or(CredentialProviderError::Unavailable)?
                .password_configuration_status(credential_ref)?
                .ok_or(CredentialProviderError::Unavailable),
        }
    }

    pub(crate) fn configure_password_on_main_thread(
        &self,
        route: CredentialRoute,
        credential_ref: &CredentialRef,
    ) -> Result<ConfigurePasswordSettlement, CredentialProviderError> {
        match route {
            CredentialRoute::Production => self
                .production
                .configure_password_on_main_thread(credential_ref),
            #[cfg(debug_assertions)]
            CredentialRoute::Fixture => self
                .development_fixtures
                .as_ref()
                .ok_or(CredentialProviderError::Unavailable)?
                .configure_password(credential_ref)?
                .ok_or(CredentialProviderError::Unavailable),
        }
    }

    pub(crate) fn remove_password_on_main_thread(
        &self,
        route: CredentialRoute,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
        match route {
            CredentialRoute::Production => self
                .production
                .remove_password_on_main_thread(credential_ref),
            #[cfg(debug_assertions)]
            CredentialRoute::Fixture => self
                .development_fixtures
                .as_ref()
                .ok_or(CredentialProviderError::Unavailable)?
                .remove_password(credential_ref)?
                .ok_or(CredentialProviderError::Unavailable),
        }
    }

    pub(crate) fn remove_password_after_confirmation(
        &self,
        route: CredentialRoute,
        credential_ref: &CredentialRef,
    ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
        match route {
            CredentialRoute::Production => self
                .production
                .remove_password_after_confirmation(credential_ref),
            #[cfg(debug_assertions)]
            CredentialRoute::Fixture => self
                .development_fixtures
                .as_ref()
                .ok_or(CredentialProviderError::Unavailable)?
                .remove_password(credential_ref)?
                .ok_or(CredentialProviderError::Unavailable),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialRoute {
    Production,
    #[cfg(debug_assertions)]
    Fixture,
}

pub(crate) struct HumanTerminalCredentialBroker<'a, P> {
    runtime: &'a HumanCredentialRuntime<P>,
    route: CredentialRoute,
}

impl<P> fmt::Debug for HumanTerminalCredentialBroker<'_, P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HumanTerminalCredentialBroker")
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}

impl<P> CredentialBroker for HumanTerminalCredentialBroker<'_, P>
where
    P: CredentialBroker,
{
    fn lease_for_human_terminal(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError> {
        match self.route {
            CredentialRoute::Production => self
                .runtime
                .production
                .lease_for_human_terminal(credential_ref),
            #[cfg(debug_assertions)]
            CredentialRoute::Fixture => self
                .runtime
                .development_fixtures
                .as_ref()
                .ok_or(CredentialBrokerError::Unavailable)?
                .lease_for_human_terminal(credential_ref),
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Clone)]
struct DevelopmentFixtureCredentials {
    configured: Arc<Mutex<HashMap<CredentialRef, bool>>>,
}

#[cfg(debug_assertions)]
impl DevelopmentFixtureCredentials {
    fn new(fixture_refs: impl IntoIterator<Item = CredentialRef>) -> Self {
        Self {
            configured: Arc::new(Mutex::new(
                fixture_refs
                    .into_iter()
                    .map(|reference| (reference, true))
                    .collect(),
            )),
        }
    }

    fn password_configuration_status(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<Option<PasswordConfigurationStatus>, CredentialProviderError> {
        let configured = self
            .configured
            .lock()
            .map_err(|_| CredentialProviderError::Unavailable)?;
        Ok(configured.get(credential_ref).copied().map(|configured| {
            if configured {
                PasswordConfigurationStatus::Configured
            } else {
                PasswordConfigurationStatus::NotConfigured
            }
        }))
    }

    fn configure_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<Option<ConfigurePasswordSettlement>, CredentialProviderError> {
        let mut configured = self
            .configured
            .lock()
            .map_err(|_| CredentialProviderError::Unavailable)?;
        let Some(configured) = configured.get_mut(credential_ref) else {
            return Ok(None);
        };
        *configured = true;
        Ok(Some(ConfigurePasswordSettlement::Configured))
    }

    fn remove_password(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<Option<RemovePasswordSettlement>, CredentialProviderError> {
        let mut configured = self
            .configured
            .lock()
            .map_err(|_| CredentialProviderError::Unavailable)?;
        let Some(configured) = configured.get_mut(credential_ref) else {
            return Ok(None);
        };
        let settlement = if *configured {
            *configured = false;
            RemovePasswordSettlement::Removed
        } else {
            RemovePasswordSettlement::AlreadyMissing
        };
        Ok(Some(settlement))
    }

    fn lease_for_human_terminal(
        &self,
        credential_ref: &CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError> {
        let configured = self
            .configured
            .lock()
            .map_err(|_| CredentialBrokerError::Unavailable)?;
        if !matches!(configured.get(credential_ref), Some(true)) {
            return Err(CredentialBrokerError::Unavailable);
        }
        drop(configured);

        HumanCredentialLease::from_password(Uuid::new_v4().as_bytes().to_vec())
            .map_err(|_| CredentialBrokerError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[derive(Default)]
    struct Calls {
        status: AtomicUsize,
        configure: AtomicUsize,
        remove: AtomicUsize,
        remove_after_confirmation: AtomicUsize,
        lease: AtomicUsize,
    }

    #[derive(Clone, Default)]
    struct FakeProductionProvider {
        calls: Arc<Calls>,
    }

    impl HumanPasswordCredentialProvider for FakeProductionProvider {
        fn password_configuration_status(
            &self,
            _credential_ref: &CredentialRef,
        ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
            self.calls.status.fetch_add(1, Ordering::Relaxed);
            Ok(PasswordConfigurationStatus::Configured)
        }

        fn configure_password_on_main_thread(
            &self,
            _credential_ref: &CredentialRef,
        ) -> Result<ConfigurePasswordSettlement, CredentialProviderError> {
            self.calls.configure.fetch_add(1, Ordering::Relaxed);
            Ok(ConfigurePasswordSettlement::Configured)
        }

        fn remove_password_on_main_thread(
            &self,
            _credential_ref: &CredentialRef,
        ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
            self.calls.remove.fetch_add(1, Ordering::Relaxed);
            Ok(RemovePasswordSettlement::Removed)
        }

        fn remove_password_after_confirmation(
            &self,
            _credential_ref: &CredentialRef,
        ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
            self.calls
                .remove_after_confirmation
                .fetch_add(1, Ordering::Relaxed);
            Ok(RemovePasswordSettlement::Removed)
        }
    }

    impl CredentialBroker for FakeProductionProvider {
        fn lease_for_human_terminal(
            &self,
            _credential_ref: &CredentialRef,
        ) -> Result<HumanCredentialLease, CredentialBrokerError> {
            self.calls.lease.fetch_add(1, Ordering::Relaxed);
            HumanCredentialLease::from_password(b"synthetic-production-material".to_vec())
                .map_err(|_| CredentialBrokerError::Unavailable)
        }
    }

    fn reference(value: &str) -> CredentialRef {
        CredentialRef::parse(value).expect("credential reference should be valid")
    }

    #[test]
    fn production_runtime_delegates_every_operation() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let runtime = HumanCredentialRuntime::from_production_provider(provider);
        let credential_ref = reference("ordinary-credential");

        assert_eq!(
            runtime.password_configuration_status(CredentialRoute::Production, &credential_ref),
            Ok(PasswordConfigurationStatus::Configured)
        );
        assert_eq!(
            runtime.configure_password_on_main_thread(CredentialRoute::Production, &credential_ref),
            Ok(ConfigurePasswordSettlement::Configured)
        );
        assert_eq!(
            runtime.remove_password_on_main_thread(CredentialRoute::Production, &credential_ref),
            Ok(RemovePasswordSettlement::Removed)
        );
        assert_eq!(
            runtime
                .remove_password_after_confirmation(CredentialRoute::Production, &credential_ref),
            Ok(RemovePasswordSettlement::Removed)
        );
        let lease = runtime
            .for_human_terminal(CredentialRoute::Production)
            .lease_for_human_terminal(&credential_ref)
            .expect("production lease should be delegated");

        assert_eq!(lease.password(), b"synthetic-production-material");
        assert_eq!(calls.status.load(Ordering::Relaxed), 1);
        assert_eq!(calls.configure.load(Ordering::Relaxed), 1);
        assert_eq!(calls.remove.load(Ordering::Relaxed), 1);
        assert_eq!(calls.remove_after_confirmation.load(Ordering::Relaxed), 1);
        assert_eq!(calls.lease.load(Ordering::Relaxed), 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn fixture_configuration_state_never_delegates() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let fixture_ref = reference("reserved-fixture-credential");
        let runtime =
            HumanCredentialRuntime::with_development_provider(provider, [fixture_ref.clone()]);

        assert!(runtime.fixture_enabled());
        assert_eq!(
            runtime.password_configuration_status(CredentialRoute::Fixture, &fixture_ref),
            Ok(PasswordConfigurationStatus::Configured)
        );
        assert_eq!(
            runtime.remove_password_on_main_thread(CredentialRoute::Fixture, &fixture_ref),
            Ok(RemovePasswordSettlement::Removed)
        );
        assert_eq!(
            runtime.password_configuration_status(CredentialRoute::Fixture, &fixture_ref),
            Ok(PasswordConfigurationStatus::NotConfigured)
        );
        assert_eq!(
            runtime.remove_password_after_confirmation(CredentialRoute::Fixture, &fixture_ref),
            Ok(RemovePasswordSettlement::AlreadyMissing)
        );
        assert_eq!(
            runtime.configure_password_on_main_thread(CredentialRoute::Fixture, &fixture_ref),
            Ok(ConfigurePasswordSettlement::Configured)
        );

        assert_eq!(calls.status.load(Ordering::Relaxed), 0);
        assert_eq!(calls.configure.load(Ordering::Relaxed), 0);
        assert_eq!(calls.remove.load(Ordering::Relaxed), 0);
        assert_eq!(calls.remove_after_confirmation.load(Ordering::Relaxed), 0);
        assert_eq!(calls.lease.load(Ordering::Relaxed), 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn explicit_production_configuration_route_still_delegates() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let runtime = HumanCredentialRuntime::with_development_provider(
            provider,
            [reference("reserved-fixture-credential")],
        );
        let ordinary_ref = reference("ordinary-credential");

        runtime
            .password_configuration_status(CredentialRoute::Production, &ordinary_ref)
            .expect("status should be delegated");
        runtime
            .configure_password_on_main_thread(CredentialRoute::Production, &ordinary_ref)
            .expect("configuration should be delegated");
        runtime
            .remove_password_on_main_thread(CredentialRoute::Production, &ordinary_ref)
            .expect("removal should be delegated");
        runtime
            .remove_password_after_confirmation(CredentialRoute::Production, &ordinary_ref)
            .expect("confirmed removal should be delegated");

        assert_eq!(calls.status.load(Ordering::Relaxed), 1);
        assert_eq!(calls.configure.load(Ordering::Relaxed), 1);
        assert_eq!(calls.remove.load(Ordering::Relaxed), 1);
        assert_eq!(calls.remove_after_confirmation.load(Ordering::Relaxed), 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn fixture_configuration_route_rejects_unreserved_references_without_delegating() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let runtime = HumanCredentialRuntime::with_development_provider(
            provider,
            [reference("reserved-fixture-credential")],
        );
        let ordinary_ref = reference("ordinary-credential");

        assert_eq!(
            runtime.password_configuration_status(CredentialRoute::Fixture, &ordinary_ref),
            Err(CredentialProviderError::Unavailable)
        );
        assert_eq!(
            runtime.configure_password_on_main_thread(CredentialRoute::Fixture, &ordinary_ref),
            Err(CredentialProviderError::Unavailable)
        );
        assert_eq!(
            runtime.remove_password_on_main_thread(CredentialRoute::Fixture, &ordinary_ref),
            Err(CredentialProviderError::Unavailable)
        );
        assert_eq!(
            runtime.remove_password_after_confirmation(CredentialRoute::Fixture, &ordinary_ref),
            Err(CredentialProviderError::Unavailable)
        );

        assert_eq!(calls.status.load(Ordering::Relaxed), 0);
        assert_eq!(calls.configure.load(Ordering::Relaxed), 0);
        assert_eq!(calls.remove.load(Ordering::Relaxed), 0);
        assert_eq!(calls.remove_after_confirmation.load(Ordering::Relaxed), 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn terminal_lease_requires_an_explicit_route() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let fixture_ref = reference("reserved-fixture-credential");
        let ordinary_ref = reference("ordinary-credential");
        let runtime =
            HumanCredentialRuntime::with_development_provider(provider, [fixture_ref.clone()]);

        let first_fixture_lease = runtime
            .for_human_terminal(CredentialRoute::Fixture)
            .lease_for_human_terminal(&fixture_ref)
            .expect("reserved fixture should have a synthetic lease");
        let second_fixture_lease = runtime
            .for_human_terminal(CredentialRoute::Fixture)
            .lease_for_human_terminal(&fixture_ref)
            .expect("reserved fixture should get a fresh synthetic lease");
        assert_ne!(
            first_fixture_lease.password(),
            second_fixture_lease.password()
        );
        assert_eq!(calls.lease.load(Ordering::Relaxed), 0);

        let production_lease = runtime
            .for_human_terminal(CredentialRoute::Production)
            .lease_for_human_terminal(&fixture_ref)
            .expect("an explicit production route should delegate");
        assert_eq!(
            production_lease.password(),
            b"synthetic-production-material"
        );
        assert_eq!(calls.lease.load(Ordering::Relaxed), 1);

        let rejected = runtime
            .for_human_terminal(CredentialRoute::Fixture)
            .lease_for_human_terminal(&ordinary_ref);
        assert!(matches!(rejected, Err(CredentialBrokerError::Unavailable)));
        assert_eq!(calls.lease.load(Ordering::Relaxed), 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn removed_fixture_cannot_issue_a_terminal_lease() {
        let provider = FakeProductionProvider::default();
        let calls = provider.calls.clone();
        let fixture_ref = reference("reserved-fixture-credential");
        let runtime =
            HumanCredentialRuntime::with_development_provider(provider, [fixture_ref.clone()]);

        assert_eq!(
            runtime.remove_password_after_confirmation(CredentialRoute::Fixture, &fixture_ref),
            Ok(RemovePasswordSettlement::Removed)
        );
        let rejected = runtime
            .for_human_terminal(CredentialRoute::Fixture)
            .lease_for_human_terminal(&fixture_ref);

        assert!(matches!(rejected, Err(CredentialBrokerError::Unavailable)));
        assert_eq!(calls.lease.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn debug_output_does_not_expose_credential_material_or_references() {
        let provider = FakeProductionProvider::default();
        let runtime = HumanCredentialRuntime::from_production_provider(provider);
        let runtime_debug = format!("{runtime:?}");
        let broker_debug = format!(
            "{:?}",
            runtime.for_human_terminal(CredentialRoute::Production)
        );

        assert!(!runtime_debug.contains("synthetic-production-material"));
        assert!(!runtime_debug.contains("ordinary-credential"));
        assert!(!broker_debug.contains("synthetic-production-material"));
        assert!(!broker_debug.contains("ordinary-credential"));
    }
}
