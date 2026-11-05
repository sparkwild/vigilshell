//! 脚本入口的审阅记录；远端文件执行前复核，不提供不可变文件隔离。
use super::*;
pub const SCRIPT_LIMIT: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ScriptInterpreter {
    Sh,
    Bash,
    Python3,
    Node,
}
impl ScriptInterpreter {
    pub fn program(self) -> &'static str {
        match self {
            Self::Sh => "/bin/sh",
            Self::Bash => "/bin/bash",
            Self::Python3 => "/usr/bin/python3",
            Self::Node => "/usr/bin/node",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScriptSource {
    Inline {
        text: String,
    },
    RemoteFile {
        target: ScopedPath,
        expected_hash: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptSnapshot {
    /// 仅保留既有记录的读取；当前执行不生成或复核文件属性版本。
    #[serde(default)]
    pub source_version: Option<String>,
    pub text: String,
    pub hash: String,
    pub path: String,
    pub directory: String,
    pub interpreter: ScriptInterpreter,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptOrigin {
    pub task_id: String,
    pub session_id: String,
    pub tool: String,
    /// 仅在已记录的完整内容摘要与执行快照一致时成立，不从相同路径推断。
    pub content_matches: bool,
}
