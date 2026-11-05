//! MCP 管理接口和工具输入共用 Rust 生成的合同；管理指令不向 MCP 发布。
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vigilshell_domain::agent::*;
#[derive(JsonSchema)]
pub struct AgentContract {
    pub ui_request: AgentUiRequest,
    pub snapshot: AgentSnapshot,
    pub call: AgentCall,
    pub event: AgentEvent,
    pub execution: ExecutionContract,
}

/// 当前执行与只读历史合同，与管理请求和 MCP 输入共同生成。
#[derive(JsonSchema)]
pub struct ExecutionContract {
    pub policy: ConnectionPolicy,
    pub request: ExecutionRequest,
    pub authorization: ExecutionAuthorization,
    pub state: ExecutionState,
    pub outcome: ExecutionOutcome,
    pub task_history_filter: TaskHistoryFilter,
    pub task_history: TaskHistoryPage,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSnapshot {
    #[serde(default)]
    pub protections: Vec<SystemProtection>,
    pub views: Vec<AgentSessionView>,
    pub settings: AgentSettings,
    pub entries: Vec<AccessEntry>,
    pub policies: Vec<ConnectionPolicySettings>,
    pub contexts: Vec<AgentContext>,
    pub clients: Vec<AgentClientDetection>,
    pub observations: Vec<AccessObservation>,
    pub endpoint: String,
    pub running: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionView {
    pub context: AgentContext,
    pub running_tasks: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentUiRequest {
    ActivitySources {},
    ActivitySummaries {
        filter: ActivitySummaryFilter,
    },
    ApproveScript {
        task_id: String,
    },
    RejectScript {
        task_id: String,
    },
    PreviewPolicy {
        policy: Box<ConnectionPolicy>,
    },
    OpenPolicyBrowser {
        connection_id: String,
        connection_revision: u64,
    },
    ReadPolicyResources {
        browser_id: String,
        query: PolicyResourceQuery,
    },
    ClosePolicyBrowser {
        browser_id: String,
    },
    RetryRecording {},
    SelectLocalDirectory {},
    HideSessionView {
        session_id: String,
    },
    CloseSession {
        session_id: String,
    },
    Recordings {
        filter: RecordingFilter,
    },
    Snapshot {},
    DetectClients {},
    UpdateSettings {
        patch: AgentSettingsPatch,
    },
    PrepareAccess {
        client: AgentClient,
        name: String,
        id: Option<String>,
        delivery: AgentDelivery,
        request_key: String,
    },
    DeliverAccess {
        id: String,
        client: AgentClient,
        delivery: AgentDelivery,
    },
    UpdatePolicySettings {
        connection_id: String,
        expected_revision: u64,
        connection_revision: u64,
        patch: PolicySettingsPatch,
    },
    SetPolicyAccess {
        connection_id: String,
        expected_revision: u64,
        connection_revision: u64,
        enabled: bool,
    },
    RenameAccess {
        id: String,
        name: String,
    },
    RotateAccess {
        id: String,
    },
    RevokeAccess {
        id: String,
    },
    DeleteAccess {
        id: String,
        generation: u64,
    },
    ApproveConnection {
        context_id: String,
    },
    Task {
        task_id: String,
    },
    Tasks {
        session_id: Option<String>,
        offset: u32,
        connection_id: Option<String>,
        entry_id: Option<String>,
        phase: Option<String>,
    },
}
