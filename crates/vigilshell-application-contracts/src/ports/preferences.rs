use vigilshell_domain::{
    Preferences, PreferencesUpdate, SettingsError, UpdatePreferencesRequest, ui_state,
};

/// 唯一持久化入口；合并检查和提交必须位于同一写事务。
pub trait PreferencesRepository: Send {
    fn ui_state(&self, _scope: &str) -> Result<ui_state::UiStateRecord, SettingsError> {
        Err(SettingsError::InvalidRequest)
    }
    fn update_ui_state(
        &mut self,
        _request: ui_state::UiStateUpdate,
    ) -> Result<ui_state::UiStateRecord, SettingsError> {
        Err(SettingsError::InvalidRequest)
    }

    fn get(&self) -> Result<Preferences, SettingsError>;
    fn update(
        &mut self,
        request: UpdatePreferencesRequest,
    ) -> Result<PreferencesUpdate, SettingsError>;
}
