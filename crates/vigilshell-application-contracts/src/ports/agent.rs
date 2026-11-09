//! Agent 执行及工作区接管端口；接管不接触 Renderer credit，只由工作区控制权授权 PTY 输入。
use super::FutureResult;
use std::sync::Arc;
use vigilshell_domain::{agent::*, workspace::WorkspaceError};

/// 独立只读历史端口，不初始化 Agent、不恢复任务，也不触发历史清理。
#[derive(Clone, Debug)]
pub enum AgentHistoryCommand {
    ActivitySources,
    Screen {
        session_id: String,
        task_id: String,
    },
    ActivitySummaries {
        filter: ActivitySummaryFilter,
    },
    Task {
        id: String,
    },
    Tasks {
        filter: TaskHistoryFilter,
    },
    RequestReceipt {
        entry: String,
        key: String,
    },
    Recordings {
        filter: RecordingFilter,
    },
    Activity {
        session_id: String,
        task_id: Option<String>,
        cursor: u64,
        include_output: bool,
    },
}

#[derive(Clone, Debug)]
pub enum AgentHistoryReply {
    ActivitySources(Vec<ActivitySource>),
    Screen(Option<ScreenCheckpoint>),
    ActivitySummaries(Vec<ActivitySummary>),
    Task(Option<Box<HistoricalTask>>),
    Tasks(TaskHistoryPage),
    RequestReceipt(Option<HistoricalRequestReceipt>),
    Recordings(RecordingHistoryPage),
    Activity(ActivityHistoryPage),
}

#[derive(Clone, Debug)]
pub enum AgentDbCommand {
    Recording(Box<RecordingCommand>),
    Configuration,
    OutputRange {
        session: String,
        task: String,
    },
    Observations,
    Observe {
        observation: AccessObservation,
    },
    SaveConfiguration {
        configuration: AgentConfiguration,
        expected: u64,
    },
    SaveAuditTask {
        task: Box<AgentTask>,
    },
}
impl AgentDbCommand {
    pub fn recording(command: RecordingCommand) -> Self {
        Self::Recording(Box::new(command))
    }
}

#[derive(Clone, Debug)]
pub enum AgentDbReply {
    RecordedSession(RecordedSession),
    Activity(ActivityPage),
    Recorded(ActivityRecord),
    Configuration(AgentConfiguration),
    Observations(Vec<AccessObservation>),
    OutputRange { start: u64, end: u64 },
    Done,
}

#[derive(Clone, Debug)]
pub enum RecordingCommand {
    AppendScreen {
        record: ActivityRecord,
        limit_bytes: u64,
    },
    Probe,
    Create {
        session: RecordedSession,
    },
    Append {
        record: ActivityRecord,
        limit_bytes: u64,
    },
    Read {
        include_output: bool,
        session_id: String,
        task_id: Option<String>,
        cursor: u64,
    },
    End {
        session_id: String,
        ended_at: u64,
        result: String,
        incomplete: bool,
    },
    Recover {
        now: u64,
    },
    Prune {
        before: u64,
        limit_bytes: u64,
    },
}

pub enum AgentProcessEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(u32),
    ExitSignal { signal: String, core_dumped: bool },
    Eof,
    Closed,
}
pub trait AgentProcess: Send {
    /// 只请求停止，必须继续读取退出证据；发送成功不表示业务已终止。
    fn signal_stop(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }
    fn eof(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }
    fn resize(&mut self, _cols: u32, _rows: u32) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    fn read(&mut self) -> FutureResult<'_, AgentProcessEvent>;
    fn input<'a>(&'a mut self, bytes: &'a [u8]) -> FutureResult<'a, ()>;
    fn stop(&mut self) -> FutureResult<'_, ()>;
}
pub trait AgentConnection: Send + Sync {
    fn execute_terminal(
        &self,
        _operation: CheckedTerminal,
    ) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    fn probe_environment(&self) -> FutureResult<'_, ExecutionEnvironment> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    fn probe_script(&self, _path: String) -> FutureResult<'_, Option<ScriptInterpreter>> {
        Box::pin(async { Ok(None) })
    }

    fn prepare_script(
        &self,
        _operation: CheckedScriptOperation,
    ) -> FutureResult<'_, ScriptPreparation> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }
    /// 复核审阅过的原文件或准备 Inline 文件；失败时尚未分派业务解释器。
    fn prepare_script_launch(
        self: Arc<Self>,
        _operation: CheckedScriptLaunch,
    ) -> FutureResult<'static, Box<dyn PreparedScript>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    /// 准备阶段只查询环境；返回的具名操作必须在分派事实持久化后执行。
    fn prepare_native(
        self: Arc<Self>,
        _operation: CheckedNativeOperation,
    ) -> FutureResult<'static, Box<dyn PreparedNativeOperation>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    fn execute_files(
        &self,
        _operation: CheckedFileOperation,
        _io: FileTransferHandles,
        _control: FileExecutionControl,
    ) -> FutureResult<'_, FileExecutionReport> {
        Box::pin(async { Err(WorkspaceError::FilesUnsupported) })
    }

    fn execute_command(&self, _command: CheckedCommand) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }

    /// 仅供桌面策略编辑器调用；禁止从 AgentCall 或业务文件工具转发到此入口。
    fn inspect_resources(
        &self,
        _query: PolicyResourceQuery,
    ) -> FutureResult<'_, PolicyResourcePage> {
        Box::pin(async { Err(WorkspaceError::FilesUnsupported) })
    }
    fn encoding(&self) -> &str {
        "utf-8"
    }
    fn is_closed(&self) -> bool;
    fn close(&self) -> FutureResult<'_, ()>;
}

pub trait AgentUpload: Send {
    fn size(&self) -> u64;
    fn read(&mut self) -> FutureResult<'_, Vec<u8>>;
}
pub trait AgentDownload: Send {
    fn write(&mut self, bytes: Vec<u8>) -> FutureResult<'_, ()>;
    fn finish(&mut self) -> FutureResult<'_, ()>;
}
pub trait AgentLocalFiles: Send + Sync {
    fn upload(&self, grant: LocalTransferGrant) -> FutureResult<'_, Box<dyn AgentUpload>>;
    fn download(
        &self,
        grant: LocalTransferGrant,
        overwrite: bool,
    ) -> FutureResult<'_, Box<dyn AgentDownload>>;
}

#[derive(Clone, Debug)]
pub struct WorkspaceAgentIdentity {
    pub terminal_seconds: Option<u32>,
    pub idle_seconds: Option<u32>,
    pub entry_id: String,
    pub entry_name: String,
    pub context_id: String,
    pub connection_id: String,
    pub connection_revision: u64,
}
pub type AgentFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = AgentResult<T>> + Send + 'a>>;

pub trait AgentConnector: Send + Sync {
    /// 在授权完成后创建普通终端或接管明确指定的标签；返回时终端已可用且人工写入已锁定。
    fn open_workspace(
        &self,
        _identity: WorkspaceAgentIdentity,
        _tab_id: Option<String>,
    ) -> AgentFuture<'_, WorkspaceTerminalTab> {
        Box::pin(async { Err("workspace_unavailable") })
    }
    fn workspace_tabs(
        &self,
        _id: String,
        _revision: u64,
    ) -> AgentFuture<'_, Vec<WorkspaceTerminalTab>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn workspace_control_live(&self, _context_id: String) -> AgentFuture<'_, bool> {
        Box::pin(async { Ok(false) })
    }

    fn workspace_terminal(
        &self,
        _identity: WorkspaceAgentIdentity,
        _command: WorkspaceTerminalAction,
    ) -> AgentFuture<'_, WorkspaceTerminalReply> {
        Box::pin(async { Err("workspace_unavailable") })
    }
    fn release_workspace(&self, _context_id: String, _finish: bool) -> AgentFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn connect(&self, id: String, revision: u64) -> FutureResult<'_, Arc<dyn AgentConnection>>;
}
pub trait AgentSink: Send + Sync {
    fn publish(&self, event: AgentEvent) -> FutureResult<'_, ()>;
}
pub fn agent_storage_error(error: WorkspaceError) -> &'static str {
    match error {
        WorkspaceError::PolicyUpgradeRequired => "policy_upgrade_required",
        WorkspaceError::AgentConfigurationUnavailable => "agent_configuration_unavailable",
        _ => "agent_storage_unavailable",
    }
}

#[derive(Default)]
pub struct FileTransferHandles {
    pub upload: Option<Box<dyn AgentUpload>>,
    pub download: Option<Box<dyn AgentDownload>>,
}
#[derive(Clone)]
pub struct FileExecutionControl {
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
    pub bytes: Arc<std::sync::atomic::AtomicU64>,
    /// u64::MAX 表示大小尚未取得。
    pub total: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for FileExecutionControl {
    fn default() -> Self {
        Self {
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total: Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX)),
        }
    }
}

/// 固定探测之后的执行计划；只由适配器创建，不能从 MCP 报文反序列化。
pub trait PreparedNativeOperation: Send {
    fn recovery_paths(&self) -> Vec<String>;
    fn execute(
        self: Box<Self>,
        output: Arc<dyn NativeOperationOutput>,
        control: FileExecutionControl,
    ) -> FutureResult<'static, NativeExecutionReport>;
}
pub trait NativeOperationOutput: Send + Sync {
    fn append(&self, stream: &'static str, bytes: Vec<u8>) -> FutureResult<'_, ()>;
}

pub struct ScriptPreparation {
    pub snapshot: ScriptSnapshot,
    pub staging_path: Option<String>,
}

/// 准备后仍存在外部文件并发窗口；launch 只能在持久分派登记和授权复核后调用。
pub trait PreparedScript: Send {
    fn launch(self: Box<Self>) -> FutureResult<'static, Box<dyn AgentProcess>>;
}
