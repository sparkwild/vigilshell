//! 具名原生请求只携带业务目标；程序、参数及维护文件许可由应用固定构造。
use super::*;
#[derive(Clone, Debug)]
pub struct CheckedNativeOperation {
    operation: RuntimeOperation,
    id: String,
}
impl CheckedNativeOperation {
    pub fn operation(&self) -> &RuntimeOperation {
        &self.operation
    }
    pub fn backup_root(&self, staging: &str) -> AgentResult<String> {
        let parent = staging
            .strip_suffix(&format!("/native-staging/{}", self.id))
            .ok_or("invalid_native_staging")?;
        Ok(format!(
            "{parent}/{}/{}",
            if self.operation.is_package_operation() {
                "source-backups"
            } else {
                "configuration-backups"
            },
            self.id
        ))
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}
pub fn check_native_operation(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<CheckedNativeOperation> {
    request.authorize(policy)?;
    if !request.operation.is_native_operation() {
        return Err("native_operation_required");
    }
    Ok(CheckedNativeOperation {
        operation: request.operation.clone(),
        id: request.identity.task_id.clone(),
    })
}
impl RuntimeOperation {
    pub fn is_native_operation(&self) -> bool {
        self.is_package_operation()
            || self.is_service_operation()
            || matches!(
                self,
                Self::SystemGet {}
                    | Self::ProcessesList {}
                    | Self::SystemSettingsUpdate { .. }
                    | Self::DnsUpdate { .. }
                    | Self::PowerControl { .. }
            )
    }
    pub fn is_service_operation(&self) -> bool {
        matches!(
            self,
            Self::ServicesList {}
                | Self::ServicesStatus { .. }
                | Self::ServicesLogs { .. }
                | Self::ServicesDefinition { .. }
                | Self::ServicesRegister { .. }
                | Self::ServicesUpdate { .. }
                | Self::ServicesUnregister { .. }
                | Self::ServicesControl { .. }
                | Self::ServicesSetEnabled { .. }
        )
    }

    pub fn is_package_operation(&self) -> bool {
        matches!(
            self,
            Self::PackagesList {}
                | Self::PackagesInstall { .. }
                | Self::PackagesUpdate { .. }
                | Self::PackagesRemove { .. }
                | Self::PackageSourcesList {}
                | Self::PackageSourcesCreate { .. }
                | Self::PackageSourcesUpdate { .. }
                | Self::PackageSourcesRemove { .. }
                | Self::PackageSourcesManage { .. }
        )
    }
    pub fn is_package_source(&self) -> bool {
        matches!(
            self,
            Self::PackageSourcesList {}
                | Self::PackageSourcesCreate { .. }
                | Self::PackageSourcesUpdate { .. }
                | Self::PackageSourcesRemove { .. }
                | Self::PackageSourcesManage { .. }
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeExecutionReport {
    pub outcome: Option<ExecutionOutcome>,
    pub effects: FileCommitState,
    pub error: Option<String>,
    pub json: Option<String>,
    pub recovery_paths: Vec<String>,
}

#[cfg(test)]
#[path = "native_execution_tests.rs"]
mod tests;
