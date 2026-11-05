//! 应用用例与能力端口；Backend 只负责组装和调用。
use ports::PreferencesRepository;
pub use vigilshell_application_contracts::{ports, tools};
use vigilshell_domain::{Preferences, PreferencesUpdate, SettingsError, UpdatePreferencesRequest};
pub struct SettingsService<R: PreferencesRepository> {
    repository: R,
}
impl<R: PreferencesRepository> SettingsService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub fn repository_mut(&mut self) -> &mut R {
        &mut self.repository
    }
    pub fn get(&self) -> Result<Preferences, SettingsError> {
        self.repository.get()
    }
    pub fn update(
        &mut self,
        request: UpdatePreferencesRequest,
    ) -> Result<PreferencesUpdate, SettingsError> {
        request.validate()?;
        self.repository.update(request)
    }
}

pub mod agent;
pub mod workspace;
