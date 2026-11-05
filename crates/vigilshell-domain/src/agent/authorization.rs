//! 所有执行途径共用的操作授权；调用方不能用工具名称或说明文字替代效果检查。
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationTarget {
    pub path: String,
    pub effect: FileEffect,
    pub recursive: bool,
}

pub fn valid_content_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
