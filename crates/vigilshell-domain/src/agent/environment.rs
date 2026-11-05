//! SSH/SFTP 的只读探测结果；配置授权和每个目标的实际权限仍独立判定。
use super::*;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionEnvironment {
    pub operating_system: String,
    pub architecture: String,
    pub account_uid: Option<u32>,
    pub supported_tools: Vec<String>,
    pub programs: Vec<String>,
    pub package_managers: Vec<PackageManagerSupport>,
    pub system_service_manager: bool,
    pub user_service_manager: bool,
    pub sftp: bool,
    pub atomic_rename: bool,
    pub reason: Option<String>,
}
