//! Main 系统通知边界的安全 DTO；不授予 Renderer 任意发送能力。
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vigilshell_domain as domain;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationMode {
    Off,
    Unfocused,
    Always,
}
impl From<domain::notifications::NotificationMode> for NotificationMode {
    fn from(value: domain::notifications::NotificationMode) -> Self {
        match value {
            domain::notifications::NotificationMode::Off => Self::Off,
            domain::notifications::NotificationMode::Unfocused => Self::Unfocused,
            domain::notifications::NotificationMode::Always => Self::Always,
        }
    }
}
impl From<NotificationMode> for domain::notifications::NotificationMode {
    fn from(value: NotificationMode) -> Self {
        match value {
            NotificationMode::Off => Self::Off,
            NotificationMode::Unfocused => Self::Unfocused,
            NotificationMode::Always => Self::Always,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationAuthorization {
    NotDetermined,
    Authorized,
    Denied,
    Provisional,
    Unknown,
    Unsupported,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemNotificationStatus {
    pub authorization: NotificationAuthorization,
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    pub version: u64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SystemNotificationCommand {
    Status,
    OpenSettings,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationOutcome {
    Submitted,
    Suppressed,
    Duplicate,
    Failed,
    Unknown,
    Opened,
    Stopped,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemNotificationResult {
    pub outcome: NotificationOutcome,
    pub status: SystemNotificationStatus,
}
#[derive(JsonSchema)]
pub struct SystemNotificationContract {
    pub command: SystemNotificationCommand,
    pub feedback: NotificationFeedback,
    pub feedback_request: NotificationFeedbackRequest,
    pub transfer_feedback: TransferFeedbackReport,
    pub transfer_target: TransferNotificationTarget,
    pub status: SystemNotificationStatus,
    pub result: SystemNotificationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferFeedbackReport {
    pub runtime_id: String,
    #[schemars(length(max = 100))]
    pub keys: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationFeedbackDisposition {
    Pending,
    Local,
    Submitted,
    Unknown,
    Presented,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferNotificationTarget {
    pub runtime_id: String,
    pub connection_id: String,
    pub batch_id: String,
    pub phase: TransferNotificationPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransferNotificationPhase {
    Failed,
    AwaitingDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NotificationTarget {
    Transfer {
        runtime_id: String,
        connection_id: String,
        batch_id: String,
        phase: TransferNotificationPhase,
    },
    AgentTask {
        context_id: String,
        task_id: String,
        review: bool,
    },
    AgentContext {
        context_id: String,
    },
    AgentActivity {
        context_id: String,
    },
    FileEditor {
        id: String,
    },
    FileOperation {
        runtime_id: String,
        session_id: String,
        operation_id: String,
    },
    Connection {
        runtime_id: String,
        connection_id: String,
        session_id: Option<String>,
        tunnel_id: Option<String>,
    },
    Runtime {},
    Mcp {},
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationCategory {
    AttentionRequired,
    ExecutionIssue,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationFeedback {
    pub id: String,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub target: NotificationTarget,
    pub route: NotificationFeedbackDisposition,
    pub relevant: bool,
    pub related_targets: Vec<NotificationTarget>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum NotificationFeedbackRequest {
    List {},
    Presented {
        #[schemars(length(max = 100))]
        ids: Vec<String>,
    },
    Activate {
        #[schemars(length(max = 256))]
        id: String,
    },
}
