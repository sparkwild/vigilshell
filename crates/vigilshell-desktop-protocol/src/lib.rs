//! 私有桌面协议；版本由 PROTOCOL_VERSION 定义。
mod v3;
pub use v3::*;

pub mod workspace;

pub mod terminal;

pub mod tools;

pub mod ui_state;

#[derive(schemars::JsonSchema)]
pub struct DesktopContract {
    pub file_editors: file_editing::FileEditorContract,
    pub agent: agent::AgentContract,
    pub system_notifications: notifications::SystemNotificationContract,
    pub runtime: RuntimeContract,
    pub workspace: workspace::WorkspaceContract,
    pub desktop: DesktopSnapshot,
    pub error_code: ErrorCode,
}
pub fn contract_schema() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<DesktopContract>()
}

mod schema;

pub mod agent;
pub mod notifications;

pub mod file_editing;
