//! 独立于桌面与数据库的设置值、字段合并规则和持久化端口。
pub mod notifications;
use notifications::NotificationMode;
pub mod tools;
pub mod ui_state;
pub mod vault;
pub mod workspace;
pub mod workspace_control;
pub const MAX_REVISION: u64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleMode {
    System,
    English,
    Chinese,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    pub system_notification_mode: NotificationMode,
    pub attention_required_notification_enabled: bool,
    pub execution_issue_notification_enabled: bool,
    pub auto_check_updates: bool,
    pub theme: ThemeMode,
    pub locale: LocaleMode,
    pub terminal_font_size: u16,
    pub terminal_line_height: u16,
    pub terminal_scrollback: u32,
    pub terminal_cursor_style: String,
    pub terminal_cursor_blink: bool,
    pub connection_open_behavior: String,
    pub files_show_hidden: bool,
    pub files_show_tree: bool,
    pub files_columns: Vec<String>,
    pub download_mode: String,
    pub download_directory: String,
    pub monitor_sample_seconds: u16,
    pub monitor_full_seconds: u16,
    pub terminal_copy_on_select: bool,
    pub terminal_theme_mode: String,
    pub terminal_font_family: String,
    pub sidebar_hover_preview: bool,
    pub files_auto_open: bool,
    pub remote_directory_mode: String,
    pub file_editor_application: String,
    pub monitor_display: String,

    pub revision: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            system_notification_mode: NotificationMode::Unfocused,
            attention_required_notification_enabled: true,
            execution_issue_notification_enabled: true,
            auto_check_updates: true,
            theme: ThemeMode::System,
            locale: LocaleMode::System,
            terminal_font_size: 12,
            terminal_line_height: 14,
            terminal_scrollback: 10000,
            terminal_cursor_style: "block".into(),
            terminal_cursor_blink: true,
            connection_open_behavior: "ask".into(),
            files_show_hidden: true,
            files_show_tree: true,
            files_columns: ["name", "size", "kind", "modified", "permissions", "owner"]
                .map(str::to_owned)
                .to_vec(),
            download_mode: "ask".into(),
            download_directory: String::new(),
            monitor_sample_seconds: 5,
            monitor_full_seconds: 60,
            terminal_copy_on_select: false,
            terminal_theme_mode: "inherit".into(),
            terminal_font_family: "jetbrains-mono".into(),
            sidebar_hover_preview: true,
            files_auto_open: true,
            remote_directory_mode: "initial".into(),
            file_editor_application: String::new(),
            monitor_display: "bar".into(),

            revision: 0,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct PreferencesPatch {
    pub system_notification_mode: Option<NotificationMode>,
    pub attention_required_notification_enabled: Option<bool>,
    pub execution_issue_notification_enabled: Option<bool>,
    pub auto_check_updates: Option<bool>,
    pub theme_mode: Option<ThemeMode>,
    pub locale_mode: Option<LocaleMode>,
    pub terminal_font_size: Option<u16>,
    pub terminal_line_height: Option<u16>,
    pub terminal_scrollback: Option<u32>,
    pub terminal_cursor_style: Option<String>,
    pub terminal_cursor_blink: Option<bool>,
    pub connection_open_behavior: Option<String>,
    pub files_show_hidden: Option<bool>,
    pub files_show_tree: Option<bool>,
    pub files_columns: Option<Vec<String>>,
    pub download_mode: Option<String>,
    pub download_directory: Option<String>,
    pub monitor_sample_seconds: Option<u16>,
    pub monitor_full_seconds: Option<u16>,
    pub terminal_copy_on_select: Option<bool>,
    pub terminal_theme_mode: Option<String>,
    pub terminal_font_family: Option<String>,
    pub sidebar_hover_preview: Option<bool>,
    pub files_auto_open: Option<bool>,
    pub remote_directory_mode: Option<String>,
    pub file_editor_application: Option<String>,
    pub monitor_display: Option<String>,
}
#[derive(Debug, Clone)]
pub struct UpdatePreferencesRequest {
    pub patch: PreferencesPatch,
    pub base: PreferencesPatch,
    pub expected_revision: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferencesUpdate {
    pub preferences: Preferences,
    pub applied: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Conflict(Box<Preferences>),
    StorageUnavailable,
    InvalidRequest,
    RevisionLimit,
}
impl Preferences {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if !["inherit", "light", "dark"].contains(&self.terminal_theme_mode.as_str()) {
            return Err(SettingsError::InvalidRequest);
        }
        if ![
            "jetbrains-mono",
            "menlo",
            "monaco",
            "consolas",
            "cascadia-mono",
            "cascadia-code",
            "dejavu-sans-mono",
            "liberation-mono",
            "courier-new",
        ]
        .contains(&self.terminal_font_family.as_str())
        {
            return Err(SettingsError::InvalidRequest);
        }
        if !["bar", "chart"].contains(&self.monitor_display.as_str())
            || !valid_editor_application(&self.file_editor_application)
            || !["last", "initial"].contains(&self.remote_directory_mode.as_str())
        {
            return Err(SettingsError::InvalidRequest);
        }

        if !(((10..=24).contains(&self.terminal_font_size))
            && ((10..=20).contains(&self.terminal_line_height))
            && ([1000, 5000, 10000, 20000, 50000].contains(&self.terminal_scrollback))
            && (["block", "bar", "underline"].contains(&self.terminal_cursor_style.as_str()))
            && (["ask", "reuse", "new"].contains(&self.connection_open_behavior.as_str()))
            && (self.files_columns.first().is_some_and(|v| v == "name")
                && self.files_columns.len() <= 6
                && self.files_columns.iter().enumerate().all(|(i, v)| {
                    ["name", "size", "kind", "modified", "permissions", "owner"]
                        .contains(&v.as_str())
                        && !self.files_columns[..i].contains(v)
                }))
            && (["ask", "directory"].contains(&self.download_mode.as_str()))
            && (self.download_directory.len() <= 4096
                && !self.download_directory.contains('\0')
                && (self.download_directory.is_empty()
                    || std::path::Path::new(&self.download_directory).is_absolute())
                && (self.download_mode != "directory" || !self.download_directory.is_empty()))
            && ([1, 5, 15].contains(&self.monitor_sample_seconds))
            && (self.monitor_full_seconds
                == if self.monitor_sample_seconds == 15 {
                    120
                } else {
                    60
                }))
        {
            return Err(SettingsError::InvalidRequest);
        }
        Ok(())
    }
}
impl PreferencesPatch {
    fn validate_values(&self) -> Result<(), SettingsError> {
        if self
            .monitor_display
            .as_ref()
            .is_some_and(|v| !["bar", "chart"].contains(&v.as_str()))
        {
            return Err(SettingsError::InvalidRequest);
        }

        if self
            .file_editor_application
            .as_ref()
            .is_some_and(|v| !valid_editor_application(v))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .terminal_theme_mode
            .as_ref()
            .is_some_and(|value| !["inherit", "light", "dark"].contains(&value.as_str()))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self.terminal_font_family.as_ref().is_some_and(|value| {
            ![
                "jetbrains-mono",
                "menlo",
                "monaco",
                "consolas",
                "cascadia-mono",
                "cascadia-code",
                "dejavu-sans-mono",
                "liberation-mono",
                "courier-new",
            ]
            .contains(&value.as_str())
        }) {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .remote_directory_mode
            .as_ref()
            .is_some_and(|value| !["last", "initial"].contains(&value.as_str()))
        {
            return Err(SettingsError::InvalidRequest);
        }

        if self
            .terminal_font_size
            .as_ref()
            .is_some_and(|value| !((10..=24).contains(value)))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .terminal_line_height
            .as_ref()
            .is_some_and(|value| !((10..=20).contains(value)))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .terminal_scrollback
            .as_ref()
            .is_some_and(|value| !([1000, 5000, 10000, 20000, 50000].contains(value)))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .terminal_cursor_style
            .as_ref()
            .is_some_and(|value| !(["block", "bar", "underline"].contains(&value.as_str())))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .connection_open_behavior
            .as_ref()
            .is_some_and(|value| !(["ask", "reuse", "new"].contains(&value.as_str())))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self.files_columns.as_ref().is_some_and(|value| {
            !(value.first().is_some_and(|v| v == "name")
                && value.len() <= 6
                && value.iter().enumerate().all(|(i, v)| {
                    ["name", "size", "kind", "modified", "permissions", "owner"]
                        .contains(&v.as_str())
                        && !value[..i].contains(v)
                }))
        }) {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .download_mode
            .as_ref()
            .is_some_and(|value| !(["ask", "directory"].contains(&value.as_str())))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self.download_directory.as_ref().is_some_and(|value| {
            !(value.len() <= 4096
                && !value.contains('\0')
                && (value.is_empty() || std::path::Path::new(value).is_absolute()))
        }) {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .monitor_sample_seconds
            .as_ref()
            .is_some_and(|value| !([1, 5, 15].contains(value)))
        {
            return Err(SettingsError::InvalidRequest);
        }
        if self
            .monitor_full_seconds
            .as_ref()
            .is_some_and(|value| !([60, 120].contains(value)))
        {
            return Err(SettingsError::InvalidRequest);
        }
        Ok(())
    }
}
impl UpdatePreferencesRequest {
    pub fn validate(&self) -> Result<(), SettingsError> {
        self.patch.validate_values()?;
        self.base.validate_values()?;
        if self.expected_revision > MAX_REVISION
            || !(self.patch.theme_mode.is_some()
                || self.patch.locale_mode.is_some()
                || self.patch.terminal_font_size.is_some()
                || self.patch.terminal_line_height.is_some()
                || self.patch.terminal_scrollback.is_some()
                || self.patch.terminal_cursor_style.is_some()
                || self.patch.terminal_cursor_blink.is_some()
                || self.patch.connection_open_behavior.is_some()
                || self.patch.files_show_hidden.is_some()
                || self.patch.files_show_tree.is_some()
                || self.patch.files_columns.is_some()
                || self.patch.download_mode.is_some()
                || self.patch.download_directory.is_some()
                || self.patch.monitor_sample_seconds.is_some()
                || self.patch.monitor_full_seconds.is_some()
                || self.patch.terminal_copy_on_select.is_some()
                || self.patch.terminal_theme_mode.is_some()
                || self.patch.terminal_font_family.is_some()
                || self.patch.sidebar_hover_preview.is_some()
                || self.patch.files_auto_open.is_some()
                || self.patch.system_notification_mode.is_some()
                || self.patch.attention_required_notification_enabled.is_some()
                || self.patch.execution_issue_notification_enabled.is_some()
                || self.patch.auto_check_updates.is_some()
                || self.patch.remote_directory_mode.is_some()
                || self.patch.file_editor_application.is_some()
                || self.patch.monitor_display.is_some())
            || (self.patch.system_notification_mode.is_some()
                != self.base.system_notification_mode.is_some())
            || (self.patch.attention_required_notification_enabled.is_some()
                != self.base.attention_required_notification_enabled.is_some())
            || (self.patch.execution_issue_notification_enabled.is_some()
                != self.base.execution_issue_notification_enabled.is_some())
            || (self.patch.auto_check_updates.is_some() != self.base.auto_check_updates.is_some())
            || (self.patch.theme_mode.is_some() != self.base.theme_mode.is_some())
            || (self.patch.locale_mode.is_some() != self.base.locale_mode.is_some())
            || (self.patch.terminal_font_size.is_some() != self.base.terminal_font_size.is_some())
            || (self.patch.terminal_line_height.is_some()
                != self.base.terminal_line_height.is_some())
            || (self.patch.terminal_scrollback.is_some() != self.base.terminal_scrollback.is_some())
            || (self.patch.terminal_cursor_style.is_some()
                != self.base.terminal_cursor_style.is_some())
            || (self.patch.terminal_cursor_blink.is_some()
                != self.base.terminal_cursor_blink.is_some())
            || (self.patch.connection_open_behavior.is_some()
                != self.base.connection_open_behavior.is_some())
            || (self.patch.files_show_hidden.is_some() != self.base.files_show_hidden.is_some())
            || (self.patch.files_show_tree.is_some() != self.base.files_show_tree.is_some())
            || (self.patch.files_columns.is_some() != self.base.files_columns.is_some())
            || (self.patch.download_mode.is_some() != self.base.download_mode.is_some())
            || (self.patch.download_directory.is_some() != self.base.download_directory.is_some())
            || (self.patch.monitor_sample_seconds.is_some()
                != self.base.monitor_sample_seconds.is_some())
            || (self.patch.monitor_full_seconds.is_some()
                != self.base.monitor_full_seconds.is_some())
            || (self.patch.terminal_copy_on_select.is_some()
                != self.base.terminal_copy_on_select.is_some())
            || (self.patch.terminal_theme_mode.is_some() != self.base.terminal_theme_mode.is_some())
            || (self.patch.terminal_font_family.is_some()
                != self.base.terminal_font_family.is_some())
            || (self.patch.sidebar_hover_preview.is_some()
                != self.base.sidebar_hover_preview.is_some())
            || (self.patch.files_auto_open.is_some() != self.base.files_auto_open.is_some())
            || (self.patch.monitor_display.is_some() != self.base.monitor_display.is_some())
            || (self.patch.file_editor_application.is_some()
                != self.base.file_editor_application.is_some())
            || (self.patch.remote_directory_mode.is_some()
                != self.base.remote_directory_mode.is_some())
        {
            return Err(SettingsError::InvalidRequest);
        }
        Ok(())
    }
    /// 仅合并用户修改的字段；同字段冲突整笔拒绝。
    pub fn merge(&self, current: &Preferences) -> Result<PreferencesUpdate, SettingsError> {
        self.validate()?;
        if self.expected_revision > current.revision {
            return Err(SettingsError::Conflict(Box::new(current.clone())));
        }
        let mut next = current.clone();
        if let Some(desired) = self.patch.attention_required_notification_enabled {
            if self.expected_revision < current.revision
                && self.base.attention_required_notification_enabled
                    != Some(current.attention_required_notification_enabled)
                && desired != current.attention_required_notification_enabled
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.attention_required_notification_enabled = desired;
        }
        if let Some(desired) = self.patch.execution_issue_notification_enabled {
            if self.expected_revision < current.revision
                && self.base.execution_issue_notification_enabled
                    != Some(current.execution_issue_notification_enabled)
                && desired != current.execution_issue_notification_enabled
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.execution_issue_notification_enabled = desired;
        }
        if let Some(desired) = self.patch.auto_check_updates {
            if self.expected_revision < current.revision
                && self.base.auto_check_updates != Some(current.auto_check_updates)
                && desired != current.auto_check_updates
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.auto_check_updates = desired;
        }
        if let Some(desired) = self.patch.system_notification_mode {
            if self.expected_revision < current.revision
                && self.base.system_notification_mode != Some(current.system_notification_mode)
                && desired != current.system_notification_mode
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.system_notification_mode = desired;
        }
        if let Some(desired) = &self.patch.theme_mode {
            if self.expected_revision < current.revision
                && self.base.theme_mode.as_ref() != Some(&current.theme)
                && desired != &current.theme
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.theme = *desired;
        }
        if let Some(desired) = &self.patch.locale_mode {
            if self.expected_revision < current.revision
                && self.base.locale_mode.as_ref() != Some(&current.locale)
                && desired != &current.locale
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.locale = *desired;
        }
        if let Some(desired) = &self.patch.terminal_font_size {
            if self.expected_revision < current.revision
                && self.base.terminal_font_size.as_ref() != Some(&current.terminal_font_size)
                && desired != &current.terminal_font_size
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_font_size = *desired;
        }
        if let Some(desired) = &self.patch.terminal_line_height {
            if self.expected_revision < current.revision
                && self.base.terminal_line_height.as_ref() != Some(&current.terminal_line_height)
                && desired != &current.terminal_line_height
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_line_height = *desired;
        }
        if let Some(desired) = &self.patch.terminal_scrollback {
            if self.expected_revision < current.revision
                && self.base.terminal_scrollback.as_ref() != Some(&current.terminal_scrollback)
                && desired != &current.terminal_scrollback
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_scrollback = *desired;
        }
        if let Some(desired) = &self.patch.terminal_cursor_style {
            if self.expected_revision < current.revision
                && self.base.terminal_cursor_style.as_ref() != Some(&current.terminal_cursor_style)
                && desired != &current.terminal_cursor_style
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_cursor_style = desired.clone();
        }
        if let Some(desired) = &self.patch.terminal_cursor_blink {
            if self.expected_revision < current.revision
                && self.base.terminal_cursor_blink.as_ref() != Some(&current.terminal_cursor_blink)
                && desired != &current.terminal_cursor_blink
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_cursor_blink = *desired;
        }
        if let Some(desired) = &self.patch.connection_open_behavior {
            if self.expected_revision < current.revision
                && self.base.connection_open_behavior.as_ref()
                    != Some(&current.connection_open_behavior)
                && desired != &current.connection_open_behavior
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.connection_open_behavior = desired.clone();
        }
        if let Some(desired) = &self.patch.files_show_hidden {
            if self.expected_revision < current.revision
                && self.base.files_show_hidden.as_ref() != Some(&current.files_show_hidden)
                && desired != &current.files_show_hidden
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.files_show_hidden = *desired;
        }
        if let Some(desired) = &self.patch.files_show_tree {
            if self.expected_revision < current.revision
                && self.base.files_show_tree.as_ref() != Some(&current.files_show_tree)
                && desired != &current.files_show_tree
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.files_show_tree = *desired;
        }
        if let Some(desired) = &self.patch.files_columns {
            if self.expected_revision < current.revision
                && self.base.files_columns.as_ref() != Some(&current.files_columns)
                && desired != &current.files_columns
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.files_columns = desired.clone();
        }
        if let Some(desired) = &self.patch.download_mode {
            if self.expected_revision < current.revision
                && self.base.download_mode.as_ref() != Some(&current.download_mode)
                && desired != &current.download_mode
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.download_mode = desired.clone();
        }
        if let Some(desired) = &self.patch.download_directory {
            if self.expected_revision < current.revision
                && self.base.download_directory.as_ref() != Some(&current.download_directory)
                && desired != &current.download_directory
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.download_directory = desired.clone();
        }
        if let Some(desired) = &self.patch.monitor_sample_seconds {
            if self.expected_revision < current.revision
                && self.base.monitor_sample_seconds.as_ref()
                    != Some(&current.monitor_sample_seconds)
                && desired != &current.monitor_sample_seconds
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.monitor_sample_seconds = *desired;
        }
        if let Some(desired) = &self.patch.monitor_full_seconds {
            if self.expected_revision < current.revision
                && self.base.monitor_full_seconds.as_ref() != Some(&current.monitor_full_seconds)
                && desired != &current.monitor_full_seconds
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.monitor_full_seconds = *desired;
        }
        if let Some(desired) = &self.patch.terminal_copy_on_select {
            if self.expected_revision < current.revision
                && self.base.terminal_copy_on_select.as_ref()
                    != Some(&current.terminal_copy_on_select)
                && desired != &current.terminal_copy_on_select
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_copy_on_select = *desired;
        }
        if let Some(desired) = &self.patch.terminal_theme_mode {
            if self.expected_revision < current.revision
                && self.base.terminal_theme_mode.as_ref() != Some(&current.terminal_theme_mode)
                && desired != &current.terminal_theme_mode
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_theme_mode = desired.clone();
        }
        if let Some(desired) = &self.patch.terminal_font_family {
            if self.expected_revision < current.revision
                && self.base.terminal_font_family.as_ref() != Some(&current.terminal_font_family)
                && desired != &current.terminal_font_family
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.terminal_font_family = desired.clone();
        }
        if let Some(desired) = &self.patch.sidebar_hover_preview {
            if self.expected_revision < current.revision
                && self.base.sidebar_hover_preview.as_ref() != Some(&current.sidebar_hover_preview)
                && desired != &current.sidebar_hover_preview
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.sidebar_hover_preview = *desired;
        }
        if let Some(desired) = &self.patch.files_auto_open {
            if self.expected_revision < current.revision
                && self.base.files_auto_open.as_ref() != Some(&current.files_auto_open)
                && desired != &current.files_auto_open
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.files_auto_open = *desired;
        }
        if let Some(desired) = &self.patch.remote_directory_mode {
            if self.expected_revision < current.revision
                && self.base.remote_directory_mode.as_ref() != Some(&current.remote_directory_mode)
                && desired != &current.remote_directory_mode
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.remote_directory_mode = desired.clone();
        }
        if let Some(desired) = &self.patch.monitor_display {
            if self.expected_revision < current.revision
                && self.base.monitor_display.as_ref() != Some(&current.monitor_display)
                && desired != &current.monitor_display
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.monitor_display = desired.clone();
        }
        if let Some(desired) = &self.patch.file_editor_application {
            if self.expected_revision < current.revision
                && self.base.file_editor_application.as_ref()
                    != Some(&current.file_editor_application)
                && desired != &current.file_editor_application
            {
                return Err(SettingsError::Conflict(Box::new(current.clone())));
            }
            next.file_editor_application = desired.clone();
        }
        next.validate()?;
        let applied = &next != current;
        if applied {
            if next.revision >= MAX_REVISION {
                return Err(SettingsError::RevisionLimit);
            }
            next.revision += 1;
        }
        Ok(PreferencesUpdate {
            preferences: next,
            applied,
        })
    }
}
#[cfg(test)]
mod preference_tests {
    use super::*;
    #[test]
    fn stale_disjoint_edits_merge_but_same_field_conflicts() {
        let original = Preferences::default();
        let a = UpdatePreferencesRequest {
            patch: PreferencesPatch {
                terminal_font_size: Some(18),
                ..Default::default()
            },
            base: PreferencesPatch {
                terminal_font_size: Some(12),
                ..Default::default()
            },
            expected_revision: 0,
        };
        let current = a.merge(&original).unwrap().preferences;
        let b = UpdatePreferencesRequest {
            patch: PreferencesPatch {
                monitor_sample_seconds: Some(15),
                monitor_full_seconds: Some(120),
                ..Default::default()
            },
            base: PreferencesPatch {
                monitor_sample_seconds: Some(5),
                monitor_full_seconds: Some(60),
                ..Default::default()
            },
            expected_revision: 0,
        };
        let result = b.merge(&current).unwrap().preferences;
        assert_eq!(result.terminal_font_size, 18);
        assert_eq!(result.monitor_sample_seconds, 15);
        assert_eq!(result.revision, 2);
        let mut conflict = a.clone();
        conflict.patch.terminal_font_size = Some(20);
        assert!(matches!(
            conflict.merge(&current),
            Err(SettingsError::Conflict(_))
        ));
        assert!(!a.merge(&current).unwrap().applied);
    }
    #[test]
    fn validates_new_fields_and_download_pair_after_merge() {
        let mut current = Preferences::default();
        let mut request = UpdatePreferencesRequest {
            patch: PreferencesPatch {
                terminal_font_size: Some(25),
                ..Default::default()
            },
            base: PreferencesPatch {
                terminal_font_size: Some(12),
                ..Default::default()
            },
            expected_revision: 0,
        };
        assert!(request.validate().is_err());
        request.patch = PreferencesPatch {
            download_mode: Some("directory".into()),
            ..Default::default()
        };
        request.base = PreferencesPatch {
            download_mode: Some("ask".into()),
            ..Default::default()
        };
        assert!(request.validate().is_ok());
        assert!(request.merge(&current).is_err());
        current.download_directory = "/tmp".into();
        assert!(request.merge(&current).is_ok());
        current.files_columns = vec!["size".into()];
        assert!(current.validate().is_err());
    }
}
pub mod agent;

fn valid_editor_application(value: &str) -> bool {
    value.is_empty()
        || (value.len() <= 4096
            && !value.contains(['\0', '\n', '\r'])
            && std::path::Path::new(value).is_absolute()
            && value.ends_with(".app"))
}
