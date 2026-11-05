//! 可独立序列化的业务动作；远端协议、隔离与分发信息不属于请求载荷。
use super::*;

pub const FILE_CHUNK_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    Status,
    Logs,
    Start,
    Stop,
    Restart,
    Reload,
}

impl ServiceAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Logs => "logs",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Reload => "reload",
        }
    }
}

pub fn valid_service_name(name: &str) -> bool {
    let name = name.strip_prefix("user:").unwrap_or(name);
    !name.is_empty()
        && name.len() <= 256
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._@-".contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextReplacement {
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FileWriteMode {
    Create,
    Replace,
    Append,
}

/// 修改类操作只接收业务参数；不能携带调用者声明的权限结论或任意维护命令。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeOperation {
    DnsUpdate {
        configuration: DnsConfiguration,
        expected_hash: Option<String>,
    },
    SystemSettingsUpdate {
        setting: SystemSetting,
        value: String,
    },
    PowerControl {
        action: PowerAction,
    },
    Script {
        source: ScriptSource,
        interpreter: ScriptInterpreter,
        arguments: Vec<String>,
        directory: ScopedPath,
    },
    TransfersUpload {
        local: ScopedPath,
        remote: ScopedPath,
        overwrite: bool,
    },
    TransfersDownload {
        remote: ScopedPath,
        local: ScopedPath,
        overwrite: bool,
    },
    Command {
        command: String,
        directory: ScopedPath,
    },
    Terminal {
        directory: ScopedPath,
        columns: u16,
        rows: u16,
    },
    FilesList {
        target: ScopedPath,
        after: Option<String>,
        limit: u16,
    },
    FilesStat {
        target: ScopedPath,
        hash: bool,
    },
    FilesRead {
        target: ScopedPath,
        offset: u64,
        length: u32,
    },
    FilesWrite {
        target: ScopedPath,
        data: String,
        mode: FileWriteMode,
        expected_hash: Option<String>,
    },
    FilesEdit {
        target: ScopedPath,
        edits: Vec<TextReplacement>,
        expected_hash: String,
    },
    FilesMkdir {
        target: ScopedPath,
        parents: bool,
    },
    FilesCopy {
        source: ScopedPath,
        target: ScopedPath,
        recursive: bool,
        overwrite: bool,
    },
    FilesMove {
        source: ScopedPath,
        target: ScopedPath,
        overwrite: bool,
    },
    FilesRemove {
        target: ScopedPath,
        recursive: bool,
    },
    FilesSetAttributes {
        target: ScopedPath,
        mode: Option<u32>,
        modified_seconds: Option<u64>,
    },
    FilesLink {
        target: ScopedPath,
        destination: ScopedPath,
    },
    SystemGet {},
    ProcessesList {},
    ServicesList {},
    ServicesStatus {
        name: String,
    },
    ServicesDefinition {
        name: String,
    },
    ServicesLogs {
        name: String,
        lines: u32,
    },
    ServicesControl {
        name: String,
        action: ServiceAction,
    },
    PackagesList {},
    PackagesInstall {
        packages: Vec<PackageTarget>,
    },
    PackagesUpdate {
        packages: Vec<PackageTarget>,
    },
    PackagesRemove {
        packages: Vec<PackageTarget>,
    },
    PackageSourcesCreate {
        definition: PackageSourceDefinition,
    },
    PackageSourcesUpdate {
        source_id: String,
        expected_hash: String,
        patch: PackageSourcePatch,
    },
    PackageSourcesRemove {
        source_id: String,
        expected_hash: String,
    },
    PackageSourcesList {},
    PackageSourcesManage {
        source_id: String,
        expected_hash: String,
        enabled: bool,
    },
    ServicesRegister {
        definition: NativeServiceDefinition,
    },
    ServicesUpdate {
        definition: NativeServiceDefinition,
    },
    ServicesSetEnabled {
        name: String,
        enabled: bool,
        expected_version: Option<String>,
    },
    ServicesUnregister {
        name: String,
        drop_in: Option<String>,
        expected_hash: String,
    },
}

impl RuntimeOperation {
    /// 校验业务参数，不执行命令、不证明任意命令或脚本正文安全。
    pub fn validate_business(&self) -> AgentResult<()> {
        let service = |name: &str| {
            if valid_service_name(name) && name.ends_with(".service") {
                Ok(())
            } else {
                Err("invalid_service")
            }
        };
        let hash = |value: &str| {
            if valid_content_hash(value) {
                Ok(())
            } else {
                Err("invalid_content_hash")
            }
        };
        match self {
            Self::Command { command, .. } => {
                if command.trim().is_empty() || command.len() > 64 * 1024 || command.contains('\0')
                {
                    return Err("invalid_command");
                }
            }
            Self::Script {
                source, arguments, ..
            } => {
                if arguments.len() > 128
                    || arguments.iter().any(|value| value.contains('\0'))
                    || arguments.iter().map(String::len).sum::<usize>() > 64 * 1024
                {
                    return Err("invalid_script_arguments");
                }
                match source {
                    ScriptSource::Inline { text } => {
                        if text.trim().is_empty()
                            || text.len() > SCRIPT_LIMIT
                            || text.contains('\0')
                        {
                            return Err("invalid_script");
                        }
                    }
                    ScriptSource::RemoteFile { expected_hash, .. } => {
                        if let Some(value) = expected_hash {
                            hash(value)?;
                        }
                    }
                }
            }
            Self::Terminal { columns, rows, .. } => {
                if !(1..=500).contains(columns) || !(1..=300).contains(rows) {
                    return Err("invalid_terminal_size");
                }
            }
            Self::TransfersUpload { local, .. } | Self::TransfersDownload { local, .. } => {
                super::execution::validate_identity(&local.scope_id)?;
                if local.scope_id == ACCOUNT_SCOPE_ID || local.path.starts_with('/') {
                    return Err("invalid_local_scope");
                }
                normalize_path(
                    "/",
                    if local.path.is_empty() {
                        "."
                    } else {
                        &local.path
                    },
                )?;
            }
            Self::FilesList { after, limit, .. } => {
                if !(1..=500).contains(limit)
                    || after.as_ref().is_some_and(|value| {
                        value.len() > 255 || value.contains('/') || value.contains('\0')
                    })
                {
                    return Err("invalid_directory_page");
                }
            }
            Self::FilesRead { offset, length, .. } => {
                super::execution::validate_revision(*offset)?;
                if *length == 0 || *length as usize > FILE_CHUNK_LIMIT {
                    return Err("invalid_read_length");
                }
            }
            Self::FilesWrite {
                data,
                expected_hash,
                ..
            } => {
                if data.len() > FILE_CHUNK_LIMIT * 2 {
                    return Err("file_chunk_too_large");
                }
                if let Some(value) = expected_hash {
                    hash(value)?;
                }
            }
            Self::FilesEdit {
                edits,
                expected_hash,
                ..
            } => {
                hash(expected_hash)?;
                if edits.is_empty()
                    || edits.len() > 128
                    || edits.iter().any(|edit| edit.before.is_empty())
                    || edits
                        .iter()
                        .map(|edit| edit.before.len() + edit.after.len())
                        .sum::<usize>()
                        > 1024 * 1024
                {
                    return Err("invalid_edits");
                }
            }
            Self::FilesSetAttributes {
                mode,
                modified_seconds,
                ..
            } => {
                if mode.is_none() && modified_seconds.is_none()
                    || mode.is_some_and(|value| value & !0o777 != 0)
                {
                    return Err("invalid_file_attributes");
                }
                if let Some(value) = modified_seconds {
                    super::execution::validate_revision(*value)?;
                }
            }
            Self::ServicesStatus { name } | Self::ServicesDefinition { name } => service(name)?,
            Self::ServicesLogs { name, lines } => {
                service(name)?;
                if *lines == 0 || *lines > 10_000 {
                    return Err("invalid_log_limit");
                }
            }
            Self::ServicesControl { name, action } => {
                service(name)?;
                if matches!(action, ServiceAction::Status | ServiceAction::Logs) {
                    return Err("invalid_service_action");
                }
            }
            Self::ServicesSetEnabled {
                name,
                expected_version,
                ..
            } => {
                service(name)?;
                if let Some(value) = expected_version {
                    hash(value)?;
                }
            }
            Self::ServicesRegister { definition } => definition.validate(false)?,
            Self::ServicesUpdate { definition } => definition.validate(true)?,
            Self::ServicesUnregister {
                name,
                drop_in,
                expected_hash,
            } => {
                service(name)?;
                hash(expected_hash)?;
                if let Some(name) = drop_in {
                    validate_drop_in(name)?;
                }
            }
            Self::PackagesInstall { packages }
            | Self::PackagesUpdate { packages }
            | Self::PackagesRemove { packages } => {
                if packages.is_empty() || packages.len() > 128 {
                    return Err("invalid_package");
                }
                for package in packages {
                    package.validate(matches!(self, Self::PackagesRemove { .. }))?;
                }
            }
            Self::PackageSourcesCreate { definition } => definition.validate()?,
            Self::PackageSourcesUpdate {
                source_id,
                expected_hash,
                patch,
            } => {
                hash(source_id)?;
                hash(expected_hash)?;
                patch.validate()?;
            }
            Self::PackageSourcesRemove {
                source_id,
                expected_hash,
            }
            | Self::PackageSourcesManage {
                source_id,
                expected_hash,
                ..
            } => {
                hash(source_id)?;
                hash(expected_hash)?;
            }
            Self::SystemSettingsUpdate { setting, value } => setting.validate_value(value)?,
            Self::DnsUpdate {
                configuration,
                expected_hash,
            } => {
                configuration.validate()?;
                if let Some(value) = expected_hash {
                    hash(value)?;
                }
            }
            _ => {}
        }
        self.business_targets()?;
        Ok(())
    }

    /// 仅列出请求中明确的远端目标，不宣称枚举了命令、安装程序或服务的间接效果。
    pub fn business_targets(&self) -> AgentResult<Vec<OperationTarget>> {
        let mut targets = Vec::new();
        let mut add = |target: &ScopedPath, effect, recursive| -> AgentResult<()> {
            if target.scope_id != ACCOUNT_SCOPE_ID || target.path.starts_with('/') {
                return Err("invalid_remote_path");
            }
            let path = normalize_path(
                "/",
                if target.path.is_empty() {
                    "."
                } else {
                    &target.path
                },
            )?;
            targets.push(OperationTarget {
                path,
                effect,
                recursive,
            });
            Ok(())
        };
        match self {
            Self::Command { directory, .. } | Self::Terminal { directory, .. } => {
                add(directory, FileEffect::Read, false)?
            }
            Self::Script {
                source, directory, ..
            } => {
                add(directory, FileEffect::Read, false)?;
                if let ScriptSource::RemoteFile { target, .. } = source {
                    add(target, FileEffect::Read, false)?;
                }
            }
            Self::TransfersUpload { remote, .. } => add(remote, FileEffect::Write, false)?,
            Self::TransfersDownload { remote, .. } => add(remote, FileEffect::Read, false)?,
            Self::FilesList { target, .. }
            | Self::FilesStat { target, .. }
            | Self::FilesRead { target, .. } => add(target, FileEffect::Read, false)?,
            Self::FilesWrite { target, .. }
            | Self::FilesEdit { target, .. }
            | Self::FilesMkdir { target, .. }
            | Self::FilesSetAttributes { target, .. } => add(target, FileEffect::Write, false)?,
            Self::FilesCopy {
                source,
                target,
                recursive,
                ..
            } => {
                add(source, FileEffect::Read, *recursive)?;
                add(target, FileEffect::Write, *recursive)?;
            }
            Self::FilesMove { source, target, .. } => {
                add(source, FileEffect::Remove, true)?;
                add(target, FileEffect::Write, true)?;
            }
            Self::FilesRemove { target, recursive } => add(target, FileEffect::Remove, *recursive)?,
            Self::FilesLink {
                target,
                destination,
            } => {
                add(target, FileEffect::Write, false)?;
                add(destination, FileEffect::Read, false)?;
            }
            _ => {}
        }
        if matches!(self, Self::FilesCopy { .. } | Self::FilesMove { .. })
            && (targets[0].path == targets[1].path
                || targets[0].recursive && beneath(&targets[0].path, &targets[1].path))
        {
            return Err("invalid_destination");
        }
        Ok(targets)
    }

    pub fn execution_class(&self) -> ExecutionClass {
        if self.is_terminal() {
            ExecutionClass::Terminal
        } else if self.is_transfer() {
            ExecutionClass::Transfer
        } else {
            ExecutionClass::Task
        }
    }
    pub fn is_maintenance(&self) -> bool {
        matches!(
            self,
            Self::DnsUpdate { .. }
                | Self::SystemSettingsUpdate { .. }
                | Self::PowerControl { .. }
                | Self::PackagesInstall { .. }
                | Self::PackagesUpdate { .. }
                | Self::PackagesRemove { .. }
                | Self::PackageSourcesCreate { .. }
                | Self::PackageSourcesUpdate { .. }
                | Self::PackageSourcesRemove { .. }
                | Self::PackageSourcesManage { .. }
                | Self::ServicesRegister { .. }
                | Self::ServicesUpdate { .. }
                | Self::ServicesUnregister { .. }
                | Self::ServicesSetEnabled { .. }
                | Self::ServicesControl { .. }
        )
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::DnsUpdate { .. } => "system_settings_manage.dns",
            Self::SystemSettingsUpdate { setting, .. } => match setting {
                SystemSetting::Hostname => "system_settings_manage.hostname",
                SystemSetting::Timezone => "system_settings_manage.timezone",
                SystemSetting::Locale => "system_settings_manage.locale",
            },
            Self::PowerControl { action } => match action {
                PowerAction::Reboot => "power_control.reboot",
                PowerAction::PowerOff => "power_control.powerOff",
            },
            Self::TransfersUpload { .. } => "files_transfer.upload",
            Self::TransfersDownload { .. } => "files_transfer.download",
            Self::Command { .. } => "command_run.command",
            Self::Script { .. } => "command_run.script",
            Self::Terminal { .. } => "terminal_manage.open",
            Self::FilesList { .. } => "files_read.list",
            Self::FilesStat { .. } => "files_read.stat",
            Self::FilesRead { .. } => "files_read.read",
            Self::FilesWrite { .. } => "files_write.write",
            Self::FilesEdit { .. } => "files_write.edit",
            Self::FilesMkdir { .. } => "files_manage.mkdir",
            Self::FilesCopy { .. } => "files_manage.copy",
            Self::FilesMove { .. } => "files_manage.move",
            Self::FilesRemove { .. } => "files_remove",
            Self::FilesSetAttributes { .. } => "files_manage.set_attributes",
            Self::FilesLink { .. } => "files_manage.link",
            Self::SystemGet { .. } => "server_inspect.overview",
            Self::ProcessesList { .. } => "server_inspect.processes",
            Self::ServicesList { .. } => "services_query.list",
            Self::ServicesStatus { .. } => "services_query.status",
            Self::ServicesDefinition { .. } => "services_query.definition",
            Self::ServicesLogs { .. } => "services_query.logs",
            Self::ServicesControl { action, .. } => match action {
                ServiceAction::Start => "services_control.start",
                ServiceAction::Stop => "services_control.stop",
                ServiceAction::Restart => "services_control.restart",
                ServiceAction::Reload => "services_control.reload",
                ServiceAction::Status => "services_query.status",
                ServiceAction::Logs => "services_query.logs",
            },
            Self::PackagesList { .. } => "packages_query",
            Self::PackagesInstall { .. } => "packages_manage.install",
            Self::PackagesUpdate { .. } => "packages_manage.upgrade",
            Self::PackagesRemove { .. } => "packages_manage.remove",
            Self::PackageSourcesUpdate { .. } => "package_sources_manage.update",
            Self::PackageSourcesRemove { .. } => "package_sources_manage.remove",
            Self::PackageSourcesList { .. } => "package_sources_query",
            Self::PackageSourcesCreate { .. } => "package_sources_manage.create",
            Self::PackageSourcesManage { enabled, .. } => {
                if *enabled {
                    "package_sources_manage.enable"
                } else {
                    "package_sources_manage.disable"
                }
            }
            Self::ServicesRegister { .. } => "services_define.create",
            Self::ServicesUpdate { .. } => "services_define.update",
            Self::ServicesUnregister { .. } => "services_define.remove",
            Self::ServicesSetEnabled { enabled, .. } => {
                if *enabled {
                    "services_control.enable"
                } else {
                    "services_control.disable"
                }
            }
        }
    }
    pub fn mutating(&self) -> bool {
        !matches!(
            self,
            Self::TransfersDownload { .. }
                | Self::FilesList { .. }
                | Self::FilesStat { .. }
                | Self::FilesRead { .. }
                | Self::SystemGet {}
                | Self::ProcessesList {}
                | Self::ServicesList {}
                | Self::ServicesStatus { .. }
                | Self::ServicesDefinition { .. }
                | Self::ServicesLogs { .. }
                | Self::PackageSourcesList {}
                | Self::PackagesList {}
        )
    }
    pub fn is_transfer(&self) -> bool {
        matches!(
            self,
            Self::TransfersUpload { .. } | Self::TransfersDownload { .. }
        )
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
}
