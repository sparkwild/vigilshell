//! 配置表达用户授权，远端探针表达已实现且依赖可用的工具；两者不能互相替代。
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCapability {
    pub tool: String,
    pub configured: bool,
    pub supported: bool,
    pub usable: bool,
    pub reason: Option<String>,
    /// 目标路径、系统账号权限与资源变更必须在执行时复核，不作预先成功保证。
    pub requires_execution_check: bool,
}
impl ConnectionPolicy {
    pub fn tool_capabilities(
        &self,
        support: Option<&ExecutionEnvironment>,
        ready: bool,
    ) -> Vec<ToolCapability> {
        let p = &self.execution;
        let read = p.level != OperationLevel::Disabled;
        let write = p.level == OperationLevel::Operate;
        let queries = [
            "server_inspect.overview",
            "server_inspect.processes",
            "services_query.list",
            "services_query.status",
            "services_query.logs",
            "services_query.definition",
            "packages_query",
            "package_sources_query",
        ];
        let reads = ["files_read.list", "files_read.stat", "files_read.read"];
        let commands = [
            "command_run.command",
            "command_run.script",
            "terminal_manage.open",
            "terminal_manage.workspace",
        ];
        let writes = [
            "files_write.write",
            "files_write.edit",
            "files_manage.mkdir",
            "files_manage.copy",
            "files_manage.move",
            "files_remove",
            "files_manage.set_attributes",
            "files_manage.link",
        ];
        let native = [
            "services_control.start",
            "services_control.stop",
            "services_control.restart",
            "services_control.reload",
            "services_define.create",
            "services_define.update",
            "services_define.remove",
            "services_control.enable",
            "services_control.disable",
            "packages_manage.install",
            "packages_manage.upgrade",
            "packages_manage.remove",
            "package_sources_manage.create",
            "package_sources_manage.update",
            "package_sources_manage.remove",
            "package_sources_manage.enable",
            "package_sources_manage.disable",
            "system_settings_manage.hostname",
            "system_settings_manage.timezone",
            "system_settings_manage.locale",
            "system_settings_manage.dns",
            "power_control.reboot",
            "power_control.powerOff",
        ];
        queries
            .iter()
            .chain(&reads)
            .chain(&commands)
            .chain(&writes)
            .chain(&native)
            .copied()
            .chain(["files_transfer.upload", "files_transfer.download"])
            .map(|tool| {
                let maintenance = native.contains(&tool);
                let configured = if tool == "files_transfer.upload" {
                    write
                        && p.file_transfer
                        && (p.unrestricted_local_transfers() || !p.upload_roots.is_empty())
                } else if tool == "files_transfer.download" {
                    read && p.file_transfer
                        && (p.unrestricted_local_transfers() || !p.download_roots.is_empty())
                } else if maintenance
                    || writes.contains(&tool)
                    || matches!(
                        tool,
                        "command_run.script" | "terminal_manage.open" | "terminal_manage.workspace"
                    )
                {
                    write
                } else {
                    read
                };
                let supported = support.is_some_and(|s| {
                    s.supported_tools.iter().any(|t| {
                        t == if tool == "terminal_manage.workspace" {
                            "terminal_manage.open"
                        } else {
                            tool
                        }
                    })
                });
                ToolCapability {
                    tool: tool.into(),
                    configured,
                    supported,
                    usable: configured && supported && ready,
                    reason: if !configured {
                        Some("policy_denied")
                    } else if !ready {
                        Some("session_not_ready")
                    } else if !supported {
                        Some("operation_unsupported")
                    } else {
                        None
                    }
                    .map(str::to_owned),
                    requires_execution_check: true,
                }
            })
            .collect()
    }
}
