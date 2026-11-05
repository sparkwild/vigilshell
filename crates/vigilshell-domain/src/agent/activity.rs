//! 可视会话及持久记录共用事件，不从终端文本猜测操作或执行成功。
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ActivityActor {
    Agent,
    User,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityRecord {
    pub session_id: String,
    pub task_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub actor: ActivityActor,
    pub policy_revision: u64,
    pub body: ActivityBody,
}

/// 区分真实进程输出与工具结果，避免将命令本身输出的 JSON 当作界面数据改写。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ActivityOutputFormat {
    #[default]
    Text,
    Structured,
}
impl ActivityOutputFormat {
    pub fn is_text(&self) -> bool {
        *self == Self::Text
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ActivityBody {
    Historical {
        original_kind: String,
        raw: String,
    },
    Script {
        snapshot: ScriptSnapshot,
        origin: Option<ScriptOrigin>,
    },
    Screen {
        checkpoint: ScreenCheckpoint,
    },
    Transfer {
        bytes: u64,
        total: Option<u64>,
        hash: Option<String>,
    },
    Policy {
        policy: Box<ExecutionPolicy>,
    },
    Session {
        phase: ContextPhase,
        connection_id: String,
        entry_id: String,
        entry_name: String,
    },
    Operation {
        tool: String,
        phase: TaskPhase,
        command: Option<String>,
        targets: Vec<OperationTarget>,
        error: Option<String>,
        exit_code: Option<u32>,
    },
    Input {
        request_key: String,
        text: String,
        delivered: bool,
    },
    Output {
        #[serde(default, skip_serializing_if = "ActivityOutputFormat::is_text")]
        format: ActivityOutputFormat,
        stream: String,
        text: String,
        data: Option<String>,
    },
    Resize {
        columns: u16,
        rows: u16,
    },
    FileChanged {
        paths: Vec<String>,
    },
    Decision {
        allowed: bool,
        reason: Option<String>,
    },
    Control {
        action: String,
    },
    Gap {
        reason: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingPosition {
    pub session_id: String,
    pub segment: u32,
    pub offset: u64,
    pub length: u32,
    pub sequence: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityPage {
    pub records: Vec<ActivityRecord>,
    pub next_cursor: u64,
    pub incomplete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordedSession {
    pub id: String,
    pub connection_id: String,
    pub connection_name: String,
    pub entry_id: String,
    pub entry_name: String,
    pub created_at: u64,
    pub ended_at: Option<u64>,
    pub result: Option<String>,
    pub incomplete: bool,
    pub last_sequence: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingFilter {
    pub created_after: Option<u64>,
    pub before_created_at: Option<u64>,
    pub before_id: Option<String>,
    pub active: Option<bool>,
    pub query: Option<String>,
    pub connection_id: Option<String>,
    pub entry_id: Option<String>,
    pub session_id: Option<String>,
    pub result: Option<String>,
    pub offset: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFileEntry {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub modified_seconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFilesPage {
    pub entries: Vec<AgentFileEntry>,
    pub next_cursor: Option<String>,
}

/// Main 的派生终端画面缓存；丢失后仍可从完整输入输出恢复，不代表执行状态。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenCheckpoint {
    pub version: u32,
    pub sequence: u64,
    pub columns: u16,
    pub rows: u16,
    pub gzip: String,
}

/// Read-only cross-session activity projection. Cursor ordering is timestamp then identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivitySummaryFilter {
    pub session_id: Option<String>,
    pub query: Option<String>,
    pub active: Option<bool>,
    pub before: Option<String>,
    pub before_time: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivitySummary {
    #[serde(default)]
    pub entry_id: String,
    pub id: String,
    pub session_id: String,
    pub task_id: Option<String>,
    pub sequence: Option<u64>,
    pub created_at: u64,
    pub connection_name: String,
    pub entry_name: String,
    pub kind: String,
    pub summary: String,
    pub phase: Option<TaskPhase>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub uncertain: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivitySource {
    pub id: String,
    pub client: Option<AgentClient>,
}
