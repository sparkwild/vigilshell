//! 唯一公开 MCP 工具合同；每个合并工具由严格动作分支定义参数。
use super::*;
mod actions;
#[cfg(feature = "schema")]
mod schema;
pub use actions::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature="schema", schemars(transform=schema::close_tool_schema))]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCall {
    SessionManage(SessionManageRequest),
    ServerInspect(ServerInspectRequest),
    CommandRun(CommandRunRequest),
    TerminalManage(TerminalManageRequest),
    FilesRead(FilesReadRequest),
    FilesWrite(FilesWriteRequest),
    FilesManage(FilesManageRequest),
    FilesTransfer(FilesTransferRequest),
    ServicesQuery(ServicesQueryRequest),
    ServicesControl(ServicesControlRequest),
    ServicesDefine(ServicesDefineRequest),
    PackagesManage(PackagesManageRequest),
    PackageSourcesManage(PackageSourcesManageRequest),
    SystemSettingsManage(SystemSettingsManageRequest),
    TasksQuery(TasksQueryRequest),
    ConnectionsList {},
    CapabilitiesGet {
        session_id: String,
    },
    FilesRemove {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        recursive: bool,
    },
    PackagesQuery {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
    },
    PackageSourcesQuery {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
    },
    PowerControl {
        session_id: String,
        request_key: String,
        action: PowerAction,
    },
    TaskCancel {
        task_id: String,
        request_key: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionManageRequest {
    Open {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_id: Option<String>,
        connection_id: String,
        request_key: String,
    },
    Get {
        session_id: String,
    },
    Close {
        session_id: String,
        request_key: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerInspectRequest {
    Overview {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
    },
    Processes {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandRunRequest {
    Command {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        command: String,
        directory: String,
    },
    Script {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source: ScriptInput,
        interpreter: ScriptInterpreter,
        arguments: Vec<String>,
        directory: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalManageRequest {
    Workspace {
        session_id: String,
        command: WorkspaceTerminalAction,
    },
    Open {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        directory: String,
        columns: u16,
        rows: u16,
    },
    Write {
        task_id: String,
        text: String,
        request_key: String,
    },
    Resize {
        task_id: String,
        columns: u16,
        rows: u16,
        request_key: String,
    },
    Close {
        task_id: String,
        request_key: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilesReadRequest {
    List {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        after: Option<String>,
        limit: u16,
    },
    Stat {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        hash: bool,
    },
    Read {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        offset: u64,
        length: u32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilesWriteRequest {
    Write {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        data: String,
        mode: FileWriteMode,
        expected_hash: Option<String>,
    },
    Edit {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        edits: Vec<TextReplacement>,
        expected_hash: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilesManageRequest {
    Mkdir {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        parents: bool,
    },
    Copy {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source: String,
        target: String,
        recursive: bool,
        overwrite: bool,
    },
    Move {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source: String,
        target: String,
        overwrite: bool,
    },
    Link {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        destination: String,
    },
    SetAttributes {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: String,
        mode: Option<u32>,
        modified_seconds: Option<u64>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilesTransferRequest {
    Upload {
        session_id: String,
        local: ScopedPath,
        remote: String,
        overwrite: bool,
        request_key: String,
    },
    Download {
        session_id: String,
        remote: String,
        local: ScopedPath,
        overwrite: bool,
        request_key: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServicesQueryRequest {
    List {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
    },
    Status {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
    Logs {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
        lines: u32,
    },
    Definition {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServicesControlRequest {
    Start {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
    Stop {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
    Restart {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
    Reload {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
    },
    Enable {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
        expected_version: Option<String>,
    },
    Disable {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
        expected_version: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServicesDefineRequest {
    Create {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        definition: NativeServiceDefinition,
    },
    Update {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        definition: NativeServiceDefinition,
    },
    Remove {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        target: ServiceTarget,
        drop_in: Option<String>,
        expected_hash: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackagesManageRequest {
    Install {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        packages: Vec<PackageTarget>,
    },
    Upgrade {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        packages: Vec<PackageTarget>,
    },
    Remove {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        packages: Vec<PackageTarget>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageSourcesManageRequest {
    Create {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        definition: PackageSourceDefinition,
    },
    Update {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source_id: String,
        expected_hash: String,
        patch: PackageSourcePatch,
    },
    Remove {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source_id: String,
        expected_hash: String,
    },
    Enable {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source_id: String,
        expected_hash: String,
    },
    Disable {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        source_id: String,
        expected_hash: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum SystemSettingsManageRequest {
    Hostname {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        value: String,
    },
    Timezone {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        value: String,
    },
    Locale {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        value: String,
    },
    Dns {
        session_id: String,
        request_key: String,
        timeout_seconds: Option<u32>,
        configuration: DnsConfiguration,
        expected_hash: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TasksQueryRequest {
    List {
        session_id: Option<String>,
        connection_id: Option<String>,
        phase: Option<String>,
        offset: Option<u32>,
    },
    Get {
        task_id: String,
    },
    Output {
        task_id: String,
        cursor: Option<u64>,
        view: Option<OutputView>,
    },
}
fn remote_path(path: String) -> AgentResult<ScopedPath> {
    if !path.starts_with('/') || normalize_path("/", &path)? != path {
        return Err("invalid_path");
    }
    Ok(ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: path.trim_start_matches('/').into(),
    })
}
impl AgentCall {
    pub fn into_action(self) -> AgentResult<AgentAction> {
        Ok(match self {
            Self::SessionManage(request) => match request {
                SessionManageRequest::Open {
                    tab_id,
                    connection_id,
                    request_key,
                } => AgentAction::SessionOpen {
                    tab_id,
                    connection_id,
                    request_key,
                },
                SessionManageRequest::Get { session_id } => AgentAction::SessionGet { session_id },
                SessionManageRequest::Close {
                    session_id,
                    request_key,
                } => AgentAction::SessionClose {
                    session_id,
                    request_key,
                },
            },
            Self::ServerInspect(request) => match request {
                ServerInspectRequest::Overview {
                    session_id,
                    request_key,
                    timeout_seconds,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::SystemGet {},
                }),
                ServerInspectRequest::Processes {
                    session_id,
                    request_key,
                    timeout_seconds,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::ProcessesList {},
                }),
            },
            Self::CommandRun(request) => match request {
                CommandRunRequest::Command {
                    session_id,
                    request_key,
                    timeout_seconds,
                    command,
                    directory,
                } => {
                    let directory = remote_path(directory)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::Command { command, directory },
                    })
                }
                CommandRunRequest::Script {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source,
                    interpreter,
                    arguments,
                    directory,
                } => {
                    let source = source.into_runtime()?;
                    let directory = remote_path(directory)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::Script {
                            source,
                            interpreter,
                            arguments,
                            directory,
                        },
                    })
                }
            },
            Self::TerminalManage(request) => match request {
                TerminalManageRequest::Workspace {
                    session_id,
                    command,
                } => AgentAction::WorkspaceTerminal {
                    session_id,
                    command,
                },
                TerminalManageRequest::Open {
                    session_id,
                    request_key,
                    timeout_seconds,
                    directory,
                    columns,
                    rows,
                } => {
                    let directory = remote_path(directory)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::Terminal {
                            directory,
                            columns,
                            rows,
                        },
                    })
                }
                TerminalManageRequest::Write {
                    task_id,
                    text,
                    request_key,
                } => AgentAction::TerminalWrite {
                    task_id,
                    text,
                    request_key,
                },
                TerminalManageRequest::Resize {
                    task_id,
                    columns,
                    rows,
                    request_key,
                } => AgentAction::TerminalResize {
                    task_id,
                    columns,
                    rows,
                    request_key,
                },
                TerminalManageRequest::Close {
                    task_id,
                    request_key,
                } => AgentAction::TerminalClose {
                    task_id,
                    request_key,
                },
            },
            Self::FilesRead(request) => match request {
                FilesReadRequest::List {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    after,
                    limit,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesList {
                            target,
                            after,
                            limit,
                        },
                    })
                }
                FilesReadRequest::Stat {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    hash,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesStat { target, hash },
                    })
                }
                FilesReadRequest::Read {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    offset,
                    length,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesRead {
                            target,
                            offset,
                            length,
                        },
                    })
                }
            },
            Self::FilesWrite(request) => match request {
                FilesWriteRequest::Write {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    data,
                    mode,
                    expected_hash,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesWrite {
                            target,
                            data,
                            mode,
                            expected_hash,
                        },
                    })
                }
                FilesWriteRequest::Edit {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    edits,
                    expected_hash,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesEdit {
                            target,
                            edits,
                            expected_hash,
                        },
                    })
                }
            },
            Self::FilesManage(request) => match request {
                FilesManageRequest::Mkdir {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    parents,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesMkdir { target, parents },
                    })
                }
                FilesManageRequest::Copy {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source,
                    target,
                    recursive,
                    overwrite,
                } => {
                    let source = remote_path(source)?;
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesCopy {
                            source,
                            target,
                            recursive,
                            overwrite,
                        },
                    })
                }
                FilesManageRequest::Move {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source,
                    target,
                    overwrite,
                } => {
                    let source = remote_path(source)?;
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesMove {
                            source,
                            target,
                            overwrite,
                        },
                    })
                }
                FilesManageRequest::Link {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    destination,
                } => {
                    let target = remote_path(target)?;
                    let destination = remote_path(destination)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesLink {
                            target,
                            destination,
                        },
                    })
                }
                FilesManageRequest::SetAttributes {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    mode,
                    modified_seconds,
                } => {
                    let target = remote_path(target)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::FilesSetAttributes {
                            target,
                            mode,
                            modified_seconds,
                        },
                    })
                }
            },
            Self::FilesTransfer(request) => match request {
                FilesTransferRequest::Upload {
                    session_id,
                    local,
                    remote,
                    overwrite,
                    request_key,
                } => {
                    let remote = remote_path(remote)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds: None,
                        operation: RuntimeOperation::TransfersUpload {
                            local,
                            remote,
                            overwrite,
                        },
                    })
                }
                FilesTransferRequest::Download {
                    session_id,
                    remote,
                    local,
                    overwrite,
                    request_key,
                } => {
                    let remote = remote_path(remote)?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds: None,
                        operation: RuntimeOperation::TransfersDownload {
                            remote,
                            local,
                            overwrite,
                        },
                    })
                }
            },
            Self::ServicesQuery(request) => match request {
                ServicesQueryRequest::List {
                    session_id,
                    request_key,
                    timeout_seconds,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::ServicesList {},
                }),
                ServicesQueryRequest::Status {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesStatus { name },
                    })
                }
                ServicesQueryRequest::Logs {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    lines,
                } => {
                    let name = target.identifier()?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesLogs { name, lines },
                    })
                }
                ServicesQueryRequest::Definition {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesDefinition { name },
                    })
                }
            },
            Self::ServicesControl(request) => match request {
                ServicesControlRequest::Start {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    let service_action = ServiceAction::Start;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesControl {
                            name,
                            action: service_action,
                        },
                    })
                }
                ServicesControlRequest::Stop {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    let service_action = ServiceAction::Stop;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesControl {
                            name,
                            action: service_action,
                        },
                    })
                }
                ServicesControlRequest::Restart {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    let service_action = ServiceAction::Restart;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesControl {
                            name,
                            action: service_action,
                        },
                    })
                }
                ServicesControlRequest::Reload {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                } => {
                    let name = target.identifier()?;
                    let service_action = ServiceAction::Reload;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesControl {
                            name,
                            action: service_action,
                        },
                    })
                }
                ServicesControlRequest::Enable {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    expected_version,
                } => {
                    let name = target.identifier()?;
                    let enabled = true;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesSetEnabled {
                            name,
                            enabled,
                            expected_version,
                        },
                    })
                }
                ServicesControlRequest::Disable {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    expected_version,
                } => {
                    let name = target.identifier()?;
                    let enabled = false;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesSetEnabled {
                            name,
                            enabled,
                            expected_version,
                        },
                    })
                }
            },
            Self::ServicesDefine(request) => match request {
                ServicesDefineRequest::Create {
                    session_id,
                    request_key,
                    timeout_seconds,
                    definition,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::ServicesRegister { definition },
                }),
                ServicesDefineRequest::Update {
                    session_id,
                    request_key,
                    timeout_seconds,
                    definition,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::ServicesUpdate { definition },
                }),
                ServicesDefineRequest::Remove {
                    session_id,
                    request_key,
                    timeout_seconds,
                    target,
                    drop_in,
                    expected_hash,
                } => {
                    let name = target.identifier()?;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::ServicesUnregister {
                            name,
                            drop_in,
                            expected_hash,
                        },
                    })
                }
            },
            Self::PackagesManage(request) => match request {
                PackagesManageRequest::Install {
                    session_id,
                    request_key,
                    timeout_seconds,
                    packages,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackagesInstall { packages },
                }),
                PackagesManageRequest::Upgrade {
                    session_id,
                    request_key,
                    timeout_seconds,
                    packages,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackagesUpdate { packages },
                }),
                PackagesManageRequest::Remove {
                    session_id,
                    request_key,
                    timeout_seconds,
                    packages,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackagesRemove { packages },
                }),
            },
            Self::PackageSourcesManage(request) => match request {
                PackageSourcesManageRequest::Create {
                    session_id,
                    request_key,
                    timeout_seconds,
                    definition,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackageSourcesCreate { definition },
                }),
                PackageSourcesManageRequest::Update {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source_id,
                    expected_hash,
                    patch,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackageSourcesUpdate {
                        source_id,
                        expected_hash,
                        patch,
                    },
                }),
                PackageSourcesManageRequest::Remove {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source_id,
                    expected_hash,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::PackageSourcesRemove {
                        source_id,
                        expected_hash,
                    },
                }),
                PackageSourcesManageRequest::Enable {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source_id,
                    expected_hash,
                } => {
                    let enabled = true;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::PackageSourcesManage {
                            source_id,
                            expected_hash,
                            enabled,
                        },
                    })
                }
                PackageSourcesManageRequest::Disable {
                    session_id,
                    request_key,
                    timeout_seconds,
                    source_id,
                    expected_hash,
                } => {
                    let enabled = false;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::PackageSourcesManage {
                            source_id,
                            expected_hash,
                            enabled,
                        },
                    })
                }
            },
            Self::SystemSettingsManage(request) => match request {
                SystemSettingsManageRequest::Hostname {
                    session_id,
                    request_key,
                    timeout_seconds,
                    value,
                } => {
                    let setting = SystemSetting::Hostname;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::SystemSettingsUpdate { setting, value },
                    })
                }
                SystemSettingsManageRequest::Timezone {
                    session_id,
                    request_key,
                    timeout_seconds,
                    value,
                } => {
                    let setting = SystemSetting::Timezone;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::SystemSettingsUpdate { setting, value },
                    })
                }
                SystemSettingsManageRequest::Locale {
                    session_id,
                    request_key,
                    timeout_seconds,
                    value,
                } => {
                    let setting = SystemSetting::Locale;
                    AgentAction::Runtime(RuntimeCall {
                        parent_task_id: None,
                        session_id,
                        request_key,
                        timeout_seconds,
                        operation: RuntimeOperation::SystemSettingsUpdate { setting, value },
                    })
                }
                SystemSettingsManageRequest::Dns {
                    session_id,
                    request_key,
                    timeout_seconds,
                    configuration,
                    expected_hash,
                } => AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::DnsUpdate {
                        configuration,
                        expected_hash,
                    },
                }),
            },
            Self::TasksQuery(request) => match request {
                TasksQueryRequest::List {
                    session_id,
                    connection_id,
                    phase,
                    offset,
                } => AgentAction::TasksList {
                    session_id,
                    connection_id,
                    phase,
                    offset,
                },
                TasksQueryRequest::Get { task_id } => AgentAction::TaskGet { task_id },
                TasksQueryRequest::Output {
                    task_id,
                    cursor,
                    view,
                } => AgentAction::TaskReadOutput {
                    task_id,
                    cursor,
                    view,
                },
            },
            Self::ConnectionsList {} => AgentAction::ConnectionsList {},
            Self::CapabilitiesGet { session_id } => AgentAction::CapabilitiesGet { session_id },
            Self::FilesRemove {
                session_id,
                request_key,
                timeout_seconds,
                target,
                recursive,
            } => {
                let target = remote_path(target)?;
                AgentAction::Runtime(RuntimeCall {
                    parent_task_id: None,
                    session_id,
                    request_key,
                    timeout_seconds,
                    operation: RuntimeOperation::FilesRemove { target, recursive },
                })
            }
            Self::PackagesQuery {
                session_id,
                request_key,
                timeout_seconds,
            } => AgentAction::Runtime(RuntimeCall {
                parent_task_id: None,
                session_id,
                request_key,
                timeout_seconds,
                operation: RuntimeOperation::PackagesList {},
            }),
            Self::PackageSourcesQuery {
                session_id,
                request_key,
                timeout_seconds,
            } => AgentAction::Runtime(RuntimeCall {
                parent_task_id: None,
                session_id,
                request_key,
                timeout_seconds,
                operation: RuntimeOperation::PackageSourcesList {},
            }),
            Self::PowerControl {
                session_id,
                request_key,
                action,
            } => AgentAction::Runtime(RuntimeCall {
                parent_task_id: None,
                session_id,
                request_key,
                timeout_seconds: None,
                operation: RuntimeOperation::PowerControl { action },
            }),
            Self::TaskCancel {
                task_id,
                request_key,
            } => AgentAction::TaskCancel {
                task_id,
                request_key,
            },
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScriptInput {
    Inline {
        text: String,
    },
    RemoteFile {
        path: String,
        expected_hash: Option<String>,
    },
}
impl ScriptInput {
    fn into_runtime(self) -> AgentResult<ScriptSource> {
        Ok(match self {
            Self::Inline { text } => ScriptSource::Inline { text },
            Self::RemoteFile {
                path,
                expected_hash,
            } => ScriptSource::RemoteFile {
                target: remote_path(path)?,
                expected_hash,
            },
        })
    }
}

#[cfg(test)]
mod action_matrix_tests {
    use super::*;
    #[test]
    fn every_public_action_maps_without_broadening_readonly_authority() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../../tests/fixtures/mcp-actions.json"))
                .unwrap();
        for case in cases {
            let value = case["call"].clone();
            let call: AgentCall = serde_json::from_value(value.clone()).unwrap();
            let action = call.into_action().unwrap();
            let mut injected = value.clone();
            injected["uid"] = serde_json::json!(0);
            assert!(
                serde_json::from_value::<AgentCall>(injected).is_err(),
                "{value}"
            );
            if let AgentAction::Runtime(runtime) = action {
                let expected = case["readOnly"].as_bool().unwrap();
                let mut policy = ExecutionPolicy::default();
                assert!(policy.authorize(&runtime.operation).is_err(), "{value}");
                policy.file_transfer = true;
                let grant = LocalDirectoryGrant {
                    id: "local".into(),
                    path: "/tmp/local".into(),
                };
                policy.upload_roots = vec![grant.clone()];
                policy.download_roots = vec![grant];
                policy.level = OperationLevel::ReadOnly;
                assert_eq!(
                    policy.authorize(&runtime.operation).is_ok(),
                    expected,
                    "{value}"
                );
                policy.level = OperationLevel::Operate;
                assert!(
                    policy.authorize(&runtime.operation).is_ok(),
                    "{value}: {:?}",
                    policy.authorize(&runtime.operation)
                );
            } else {
                assert!(case["readOnly"].is_null(), "{value}");
            }
        }
    }
}
