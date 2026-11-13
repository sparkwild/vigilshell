use crate::ui_state::{UiStateRecord, UiStateUpdate};
// 私有桌面协议 DTO；从这些类型生成 TypeScript。

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use vigilshell_domain as domain;

pub const PROTOCOL_VERSION: u8 = 17;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_AGENT_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_REQUEST_ID_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum LocaleMode {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en")]
    En,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "preferences"))]
pub struct Preferences {
    pub system_notification_mode: crate::notifications::NotificationMode,
    pub attention_required_notification_enabled: bool,
    pub execution_issue_notification_enabled: bool,
    pub auto_check_updates: bool,
    #[schemars(extend("enum" = ["system", "light", "dark"]))]
    pub theme_mode: ThemeMode,
    #[schemars(extend("enum" = ["system", "en", "zh-CN"]))]
    pub locale_mode: LocaleMode,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u64,
    #[schemars(extend("maximum" = 24, "minimum" = 10))]
    pub terminal_font_size: u16,
    #[schemars(extend("maximum" = 20, "minimum" = 10))]
    pub terminal_line_height: u16,
    #[schemars(extend("enum" = [1000, 5000, 10000, 20000, 50000]))]
    pub terminal_scrollback: u32,
    #[schemars(extend("enum" = ["block", "bar", "underline"]))]
    pub terminal_cursor_style: String,
    pub terminal_cursor_blink: bool,
    #[schemars(extend("enum" = ["ask", "reuse", "new"]))]
    pub connection_open_behavior: String,
    pub files_show_hidden: bool,
    pub files_show_tree: bool,
    #[schemars(extend("maxItems" = 6, "minItems" = 1))]
    pub files_columns: Vec<String>,
    #[schemars(extend("enum" = ["ask", "directory"]))]
    pub download_mode: String,
    #[schemars(extend("maxLength" = 4096))]
    pub download_directory: String,
    #[schemars(extend("enum" = [1, 5, 15]))]
    pub monitor_sample_seconds: u16,
    #[schemars(extend("enum" = [60, 120]))]
    pub monitor_full_seconds: u16,
    pub terminal_copy_on_select: bool,
    #[schemars(extend("enum" = ["inherit", "light", "dark"]))]
    pub terminal_theme_mode: String,
    #[schemars(extend("enum" = ["jetbrains-mono", "menlo", "monaco", "consolas", "cascadia-mono", "cascadia-code", "dejavu-sans-mono", "liberation-mono", "courier-new"]))]
    pub terminal_font_family: String,
    pub sidebar_hover_preview: bool,
    pub files_auto_open: bool,
    #[schemars(extend("enum" = ["last", "initial"]))]
    pub remote_directory_mode: String,
    #[schemars(extend("maxLength" = 4096))]
    pub file_editor_application: String,
    #[schemars(extend("enum" = ["bar", "chart"]))]
    pub monitor_display: String,
}

/// 字段缺省表示保持原值；显式 null 不是合法偏好。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "preferences"))]
pub struct PreferencesPatch {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "crate::notifications::NotificationMode")]
    pub system_notification_mode: Option<crate::notifications::NotificationMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub attention_required_notification_enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub execution_issue_notification_enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub auto_check_updates: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "ThemeMode")]
    #[schemars(extend("enum" = ["system", "light", "dark"]))]
    pub theme_mode: Option<ThemeMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "LocaleMode")]
    #[schemars(extend("enum" = ["system", "en", "zh-CN"]))]
    pub locale_mode: Option<LocaleMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "u16")]
    #[schemars(extend("maximum" = 24, "minimum" = 10))]
    pub terminal_font_size: Option<u16>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "u16")]
    #[schemars(extend("maximum" = 20, "minimum" = 10))]
    pub terminal_line_height: Option<u16>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "u32")]
    #[schemars(extend("enum" = [1000, 5000, 10000, 20000, 50000]))]
    pub terminal_scrollback: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["block", "bar", "underline"]))]
    pub terminal_cursor_style: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub terminal_cursor_blink: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["ask", "reuse", "new"]))]
    pub connection_open_behavior: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub files_show_hidden: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub files_show_tree: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(extend("maxItems" = 6, "minItems" = 1))]
    #[schemars(with = "Vec<String>")]
    pub files_columns: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["ask", "directory"]))]
    pub download_mode: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("maxLength" = 4096))]
    pub download_directory: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "u16")]
    #[schemars(extend("enum" = [1, 5, 15]))]
    pub monitor_sample_seconds: Option<u16>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "u16")]
    #[schemars(extend("enum" = [60, 120]))]
    pub monitor_full_seconds: Option<u16>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub terminal_copy_on_select: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["inherit", "light", "dark"]))]
    pub terminal_theme_mode: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["jetbrains-mono", "menlo", "monaco", "consolas", "cascadia-mono", "cascadia-code", "dejavu-sans-mono", "liberation-mono", "courier-new"]))]
    pub terminal_font_family: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub sidebar_hover_preview: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "bool")]
    pub files_auto_open: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["last", "initial"]))]
    pub remote_directory_mode: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("maxLength" = 4096))]
    pub file_editor_application: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    #[schemars(with = "String")]
    #[schemars(extend("enum" = ["bar", "chart"]))]
    pub monitor_display: Option<String>,
}

fn present_value<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "preferencesUpdate"))]
pub struct UpdatePreferencesRequest {
    pub patch: PreferencesPatch,
    pub base: PreferencesPatch,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub expected_revision: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", deny_unknown_fields)]
pub enum RpcCommand {
    #[serde(rename = "uiState.get")]
    UiGet { scope: String },
    #[serde(rename = "uiState.update")]
    UiUpdate(UiStateUpdate),
    #[serde(rename = "uiState.reconcile")]
    UiReconcile {
        scope: String,
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "runtimeId")]
        runtime_id: String,
    },

    #[serde(rename = "runtime.hello")]
    Hello {
        application: String,
        #[serde(rename = "contractVersion")]
        #[schemars(extend("maximum" = 255, "minimum" = 0))]
        contract_version: u8,
    },
    #[serde(rename = "preferences.get")]
    Get {},
    #[serde(rename = "preferences.update")]
    Update(Box<UpdatePreferencesRequest>),
    #[serde(rename = "preferences.reconcile")]
    Reconcile {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "runtimeId")]
        runtime_id: String,
    },
    #[serde(rename = "requests.getStatus")]
    Status {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "requests.cancel")]
    Cancel {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "runtime.shutdown")]
    Shutdown {},
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeRequest {
    pub jsonrpc: String,
    pub id: String,
    #[serde(flatten)]
    pub command: RpcCommand,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorData {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Box<Preferences>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub code: i32,
    pub message: String,
    pub data: ErrorData,
}
impl RpcError {
    pub fn app(code: &str) -> Self {
        Self::standard(-32000, code)
    }
    pub fn standard(number: i32, code: &str) -> Self {
        Self {
            code: number,
            message: code.into(),
            data: ErrorData {
                code: code.into(),
                preferences: None,
            },
        }
    }
}
/// 仅作为近期写入的确认凭证；不用于持久任务或崩溃恢复。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "writeOutcome"))]
pub struct WriteOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_state: Option<UiStateRecord>,
    #[schemars(extend("enum" = ["applied", "unchanged", "rejected"]))]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Preferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RpcValue {
    UiState {
        state: UiStateRecord,
    },
    UiReconciled {
        state: UiStateRecord,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<WriteOutcome>")]
        outcome: Option<WriteOutcome>,
    },

    Ready {
        application: String,
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
        contract_version: u8,
        #[schemars(extend("minLength" = 1))]
        runtime_id: String,
    },
    Preferences {
        preferences: Preferences,
    },
    Updated {
        #[schemars(extend("enum" = ["applied", "unchanged"]))]
        outcome: String,
        preferences: Preferences,
    },
    Reconciled {
        preferences: Preferences,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<WriteOutcome>")]
        outcome: Option<WriteOutcome>,
    },
    RequestStatus {
        #[schemars(extend("enum" = ["queued", "running", "completed", "unknown"]))]
        state: String,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<WriteOutcome>")]
        outcome: Option<WriteOutcome>,
    },
    Cancelled {},
    Stopped {},
    Workspace {
        result: Box<crate::workspace::WorkspaceReply>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuntimeResponse {
    Success {
        #[schemars(extend("enum" = ["2.0"]))]
        jsonrpc: String,
        #[schemars(extend("minLength" = 1))]
        id: String,
        result: Box<RpcValue>,
    },
    Failure {
        #[schemars(extend("enum" = ["2.0"]))]
        jsonrpc: String,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        #[schemars(extend("minLength" = 1))]
        id: Option<String>,
        error: Box<RpcError>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangedParams {
    #[schemars(extend("minLength" = 1))]
    pub runtime_id: String,
    pub preferences: Preferences,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeNotification {
    #[schemars(extend("enum" = ["2.0"]))]
    pub jsonrpc: String,
    #[schemars(extend("enum" = ["preferences.changed"]))]
    pub method: String,
    pub params: ChangedParams,
}
#[derive(JsonSchema)]
pub struct RuntimeContract {
    pub command: RpcCommand,
    pub request: RuntimeRequest,
    pub response: RuntimeResponse,
    pub notification: RuntimeNotification,
    pub preferences: Preferences,
    pub patch: PreferencesPatch,
    pub update: UpdatePreferencesRequest,
}
impl From<UpdatePreferencesRequest> for domain::UpdatePreferencesRequest {
    fn from(v: UpdatePreferencesRequest) -> Self {
        Self {
            patch: v.patch.into(),
            base: v.base.into(),
            expected_revision: v.expected_revision,
        }
    }
}
impl From<domain::Preferences> for Preferences {
    fn from(value: domain::Preferences) -> Self {
        Self {
            system_notification_mode: value.system_notification_mode.into(),
            attention_required_notification_enabled: value.attention_required_notification_enabled,
            execution_issue_notification_enabled: value.execution_issue_notification_enabled,
            auto_check_updates: value.auto_check_updates,

            theme_mode: match value.theme {
                domain::ThemeMode::System => ThemeMode::System,
                domain::ThemeMode::Light => ThemeMode::Light,
                domain::ThemeMode::Dark => ThemeMode::Dark,
            },
            locale_mode: match value.locale {
                domain::LocaleMode::System => LocaleMode::System,
                domain::LocaleMode::Chinese => LocaleMode::ZhCn,
                domain::LocaleMode::English => LocaleMode::En,
            },
            revision: value.revision,
            terminal_font_size: value.terminal_font_size,
            terminal_line_height: value.terminal_line_height,
            terminal_scrollback: value.terminal_scrollback,
            terminal_cursor_style: value.terminal_cursor_style,
            terminal_cursor_blink: value.terminal_cursor_blink,
            connection_open_behavior: value.connection_open_behavior,
            files_show_hidden: value.files_show_hidden,
            files_show_tree: value.files_show_tree,
            files_columns: value.files_columns,
            download_mode: value.download_mode,
            download_directory: value.download_directory,
            monitor_sample_seconds: value.monitor_sample_seconds,
            monitor_full_seconds: value.monitor_full_seconds,
            terminal_copy_on_select: value.terminal_copy_on_select,
            terminal_theme_mode: value.terminal_theme_mode,
            terminal_font_family: value.terminal_font_family,
            sidebar_hover_preview: value.sidebar_hover_preview,
            files_auto_open: value.files_auto_open,
            remote_directory_mode: value.remote_directory_mode,
            file_editor_application: value.file_editor_application,
            monitor_display: value.monitor_display,
        }
    }
}

impl From<PreferencesPatch> for domain::PreferencesPatch {
    fn from(value: PreferencesPatch) -> Self {
        Self {
            system_notification_mode: value.system_notification_mode.map(Into::into),
            attention_required_notification_enabled: value.attention_required_notification_enabled,
            execution_issue_notification_enabled: value.execution_issue_notification_enabled,
            auto_check_updates: value.auto_check_updates,

            theme_mode: value.theme_mode.map(|mode| match mode {
                ThemeMode::System => domain::ThemeMode::System,
                ThemeMode::Light => domain::ThemeMode::Light,
                ThemeMode::Dark => domain::ThemeMode::Dark,
            }),
            locale_mode: value.locale_mode.map(|mode| match mode {
                LocaleMode::System => domain::LocaleMode::System,
                LocaleMode::ZhCn => domain::LocaleMode::Chinese,
                LocaleMode::En => domain::LocaleMode::English,
            }),
            terminal_font_size: value.terminal_font_size,
            terminal_line_height: value.terminal_line_height,
            terminal_scrollback: value.terminal_scrollback,
            terminal_cursor_style: value.terminal_cursor_style,
            terminal_cursor_blink: value.terminal_cursor_blink,
            connection_open_behavior: value.connection_open_behavior,
            files_show_hidden: value.files_show_hidden,
            files_show_tree: value.files_show_tree,
            files_columns: value.files_columns,
            download_mode: value.download_mode,
            download_directory: value.download_directory,
            monitor_sample_seconds: value.monitor_sample_seconds,
            monitor_full_seconds: value.monitor_full_seconds,
            terminal_copy_on_select: value.terminal_copy_on_select,
            terminal_theme_mode: value.terminal_theme_mode,
            terminal_font_family: value.terminal_font_family,
            sidebar_hover_preview: value.sidebar_hover_preview,
            files_auto_open: value.files_auto_open,
            remote_directory_mode: value.remote_directory_mode,
            file_editor_application: value.file_editor_application,
            monitor_display: value.monitor_display,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedTheme {
    Light,
    Dark,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ResolvedLocale {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    Chinese,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SavePhase {
    Idle,
    Saving,
    Confirming,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveState {
    pub phase: SavePhase,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BackendStatus {
    Starting,
    Ready,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub status: BackendStatus,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub instance_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: ResolvedTheme,
    pub locale: ResolvedLocale,
    pub reduced_transparency: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DesktopSnapshot {
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    pub version: u64,
    pub save: SaveState,
    pub runtime: RuntimeStatus,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<Preferences>")]
    pub preferences: Option<Preferences>,
    pub appearance: Appearance,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Conflict,
    StorageUnavailable,
    ProtocolMismatch,
    HostUnavailable,
    Timeout,
    Cancelled,
    TooLate,
    Busy,
    ShuttingDown,
    RevisionLimit,
    RuntimeUnavailable,
    ResultUnknown,
    InvalidParams,
}
