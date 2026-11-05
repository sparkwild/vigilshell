//! 历史只保存事实和展示投影；原授权、请求和回执正文不能转为当前执行许可。
use super::RecordingPosition;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskHistoryFilter {
    pub session_id: Option<String>,
    pub entry_id: Option<String>,
    pub connection_id: Option<String>,
    pub phase: Option<String>,
    pub offset: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HistoricalTaskDisplay {
    pub context_id: Option<String>,
    pub connection_name: Option<String>,
    pub entry_name: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub tool: Option<String>,
    pub phase: Option<String>,
    pub error: Option<String>,
    pub finished_at: Option<u64>,
    pub exit_code: Option<u32>,
    pub output_start: Option<u64>,
    pub output_end: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalTask {
    pub id: String,
    pub entry_id: String,
    pub connection_id: String,
    pub created_at: u64,
    pub active: bool,
    pub request_key: Option<String>,
    pub fingerprint: Option<String>,
    /// SQLite 保存的原正文；展示解析不重编码或修补这段内容。
    pub raw_body: String,
    pub display: HistoricalTaskDisplay,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskHistoryPage {
    pub tasks: Vec<HistoricalTask>,
    pub next_offset: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalRequestReceipt {
    pub entry_id: String,
    pub request_key: String,
    pub task_id: String,
    pub fingerprint: String,
    pub created_at: u64,
    /// false 表示结果已过期的防重放墓碑，不表示该请求可以再次执行。
    pub task_present: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalRecording {
    pub id: String,
    pub connection_id: String,
    pub entry_id: String,
    pub created_at: u64,
    pub ended_at: Option<u64>,
    pub result: Option<String>,
    pub incomplete: bool,
    pub last_sequence: u64,
    pub bytes: u64,
    pub connection_name: Option<String>,
    pub entry_name: Option<String>,
    pub raw_body: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingHistoryPage {
    pub sessions: Vec<HistoricalRecording>,
    pub next_offset: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoricalActivity {
    pub session_id: String,
    pub sequence: u64,
    pub task_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub timestamp_ms: u64,
    pub actor: Option<String>,
    pub policy_revision: Option<u64>,
    pub kind: String,
    pub position: RecordingPosition,
    pub previous_digest: String,
    pub digest: String,
    /// 已验证摘要的原 UTF-8 JSON；未知历史正文只供展示，不能提交为执行输入。
    pub raw_json: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityHistoryPage {
    pub records: Vec<HistoricalActivity>,
    pub next_cursor: u64,
    pub has_more: bool,
    pub incomplete: bool,
}
