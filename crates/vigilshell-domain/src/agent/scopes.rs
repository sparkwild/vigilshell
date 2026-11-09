//! 文件能力按实际对象统一授权。词法判断与执行端的路径、链接和版本核对共同使用。
use serde::{Deserialize, Serialize};

pub const ACCOUNT_SCOPE_ID: &str = "__account__";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ScopeKind {
    Directory,
    File,
}

/// 本地传输引用已登记目录；远端内部仅使用固定账号标识，MCP 远端输入为绝对路径。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScopedPath {
    pub scope_id: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FileEffect {
    Read,
    Write,
    Remove,
}
