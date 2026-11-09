//! Agent 的授权与执行模型；不授予人工终端权限，也不持有 SSH 秘密。
mod workspace_terminal;
pub use workspace_terminal::*;
mod activity;
mod history;
pub use history::*;
mod operation;
pub use operation::*;
mod native_execution;
pub use native_execution::*;
mod file_execution;
pub mod package_sources;
pub use file_execution::*;
mod execution_task;
pub use execution_task::*;
mod command_guard;
pub use command_guard::*;
mod execution;
mod execution_policy;
mod execution_state;
pub use execution::*;
pub use execution_policy::*;
pub use execution_state::*;
mod dns;
pub use dns::*;
mod environment;
pub use environment::*;
mod capability_snapshot;
pub use capability_snapshot::*;
mod authorization;
mod calls;
pub use calls::*;
mod limits;
pub use limits::*;
mod permission_types;
pub use permission_types::*;
mod permissions;
pub use permissions::*;
mod software;
pub use software::*;
mod inspection;
pub use inspection::*;
mod policy;
mod protection;
pub use protection::*;
mod native_operations;
mod policy_change;
pub use native_operations::*;
mod terminal_execution;
pub use terminal_execution::*;
mod script_execution;
mod scripts;
pub use script_execution::*;
pub use scripts::*;
mod scopes;
mod settings;
mod system_operations;
pub use activity::*;
pub use authorization::*;
pub use policy::*;
pub use scopes::*;
use serde::{Deserialize, Serialize};
pub use settings::*;
use std::collections::BTreeMap;

pub const MAX_TASK_SECONDS: u32 = 1800;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSettings {
    #[serde(default)]
    pub endpoint_revision: u64,
    pub revision: u64,
    pub enabled: bool,
    pub port: u16,
    pub show_tray: bool,
    pub retention_days: u32,
    pub storage_mib: u32,
}
impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            endpoint_revision: 0,
            revision: 0,
            enabled: false,
            port: 51602,
            show_tray: true,
            retention_days: 30,
            storage_mib: 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessEntry {
    #[serde(default)]
    pub client: Option<AgentClient>,
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub generation: u64,
}
/// 加密令牌只通过 Main/Backend 私有通道传递，不属于 Renderer 快照。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessSecret {
    pub entry: AccessEntry,
    pub digest: String,
    pub sealed: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfiguration {
    pub settings: AgentSettings,
    pub entries: Vec<AccessSecret>,
    pub policies: Vec<ConnectionPolicySettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ContextPhase {
    WaitingUser,
    Connecting,
    Ready,
    Closing,
    Disconnected,
    Closed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<WorkspaceTerminalTab>,
    #[serde(default)]
    pub support: Option<ExecutionEnvironment>,
    pub id: String,
    pub entry_id: String,
    pub entry_name: String,
    pub connection_id: String,
    pub connection_name: String,
    #[serde(default)]
    pub connection_revision: u64,
    pub phase: ContextPhase,
    pub cwd: String,
    pub environment: BTreeMap<String, String>,
    pub revision: u64,
    pub created_at: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TaskPhase {
    Queued,
    WaitingReview,
    Running,
    Stopping,
    Succeeded,
    RequestAccepted,
    Failed,
    Denied,
    Cancelled,
    TimedOut,
    InterruptedUnknown,
}
impl TaskPhase {
    pub fn active(&self) -> bool {
        matches!(
            self,
            Self::Queued | Self::WaitingReview | Self::Running | Self::Stopping
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTask {
    /// 业务执行的简化授权快照；仅查询或尚未接纳的审计记录可以没有快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_snapshot: Option<Box<ExecutionPolicy>>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub execution_state: Option<Box<ExecutionState>>,
    #[serde(default)]
    pub script_snapshot: Option<Box<ScriptSnapshot>>,
    #[serde(default)]
    pub historical: Option<Box<HistoricalTask>>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub recovery_paths: Vec<String>,
    #[serde(default)]
    pub file_effects: Option<FileCommitState>,
    #[serde(default)]
    pub entry_name: String,
    pub id: String,
    pub entry_id: String,
    pub context_id: String,
    pub connection_id: String,
    pub connection_name: String,
    #[serde(default)]
    pub connection_revision: u64,
    pub command: String,
    pub cwd: String,
    pub policy_revision: u64,
    pub targets: Vec<OperationTarget>,
    pub phase: TaskPhase,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
    pub termination_confirmed: bool,
    pub operation: Option<Box<RuntimeOperation>>,
    pub tool: String,
    pub terminal: bool,
    pub parent_task_id: Option<String>,
    pub output_start: u64,
    pub output_end: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputChunk {
    pub sequence: u64,
    pub stream: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskOutput {
    pub screen: Option<String>,
    pub chunks: Vec<OutputChunk>,
    pub next_cursor: u64,
    pub incomplete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConnectionView {
    pub tabs: Vec<WorkspaceTerminalTab>,
    pub id: String,
    pub name: String,
    pub environment: String,
    pub policy: ConnectionPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentMutation {
    ApproveScript {
        task_id: String,
    },
    Bootstrap {
        settings: AgentSettings,
        secret: AccessSecret,
    },
    Settings {
        settings: AgentSettings,
    },
    SettingsPatch {
        patch: AgentSettingsPatch,
    },
    Provision {
        secret: AccessSecret,
        port: u16,
        delivery: AgentDelivery,
    },
    Export {
        id: String,
        client: AgentClient,
        delivery: AgentDelivery,
        generation: u64,
        port: u16,
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
    Access {
        secret: AccessSecret,
    },
    Revoke {
        id: String,
    },
    DeleteAccess {
        id: String,
        generation: u64,
    },
    ApproveConnection {
        context_id: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentReply {
    WorkspaceTerminal {
        result: WorkspaceTerminalReply,
    },
    ActivitySources {
        sources: Vec<ActivitySource>,
    },
    ActivitySummaries {
        items: Vec<ActivitySummary>,
        more: bool,
    },
    PolicyCapabilities {
        tools: Vec<ToolCapability>,
        environment_known: bool,
    },
    PolicyBrowser {
        id: String,
    },
    PolicyResources {
        page: PolicyResourcePage,
    },
    Screen {
        checkpoint: Option<ScreenCheckpoint>,
    },
    Capabilities {
        tools: Vec<ToolCapability>,
        protections: Vec<SystemProtection>,
        policy: Box<ConnectionPolicy>,
        support: Option<ExecutionEnvironment>,
    },
    Activity {
        page: ActivityPage,
    },
    Recordings {
        sessions: Vec<RecordedSession>,
        #[serde(default)]
        has_more: bool,
    },
    Input {
        request_key: String,
        state: String,
    },
    Observations {
        observations: Vec<AccessObservation>,
    },
    Authenticated {
        endpoint_revision: u64,
        entry: AccessEntry,
    },
    Error {
        code: String,
    },
    Done {},
    Configuration {
        #[serde(default)]
        protections: Vec<SystemProtection>,
        configuration: AgentConfiguration,
        recording_error: Option<String>,
    },
    Challenge {
        id: String,
        revision: u64,
    },
    Connections {
        connections: Vec<AgentConnectionView>,
    },
    Context {
        context: AgentContext,
    },
    Contexts {
        contexts: Vec<AgentContext>,
    },
    Task {
        task: AgentTask,
    },
    Tasks {
        tasks: Vec<AgentTask>,
    },
    Output {
        output: TaskOutput,
    },
}

/// 只用于受信 Main 与 Backend 的私有 IPC，不能通过通用 Renderer 工作区入口转发。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentOperation {
    ActivitySources {},
    ActivitySummaries {
        filter: ActivitySummaryFilter,
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
    SaveScreen {
        task_id: String,
        checkpoint: ScreenCheckpoint,
    },
    LoadScreen {
        task_id: String,
    },
    RetryRecording {},
    Activity {
        session_id: String,
        task_id: Option<String>,
        cursor: u64,
        include_output: Option<bool>,
    },
    Recordings {
        filter: RecordingFilter,
    },
    CloseSession {
        session_id: String,
    },
    Reject {
        digest: String,
        tool: String,
        code: String,
    },
    Authenticate {
        digest: String,
    },
    Configuration {},
    Observations {},
    Observe {
        entry_id: String,
        generation: u64,
        port: u16,
        endpoint_revision: u64,
    },
    Prepare {
        change: AgentMutation,
    },
    Commit {
        challenge_id: String,
    },
    Abort {
        challenge_id: String,
    },
    Invoke {
        digest: String,
        call: AgentCall,
    },
    Contexts {},
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
    Output {
        task_id: String,
        cursor: u64,
    },
    Cancel {
        task_id: String,
    },
    Stop {},
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentEvent {
    ViewsChanged {
        selected_view: Option<String>,
    },
    Activity {
        record: ActivityRecord,
    },
    Task {
        task: AgentTask,
    },
    Context {
        context: AgentContext,
    },
    Attention {
        id: String,
        connection_id: String,
        code: String,
    },
    Configuration {},
    AccessChanged {},
}

pub type AgentResult<T> = Result<T, &'static str>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum OutputView {
    Stream,
    Screen,
}

mod policy_settings;
pub use policy_settings::*;
