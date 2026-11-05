//! 用例命令、结果和事件；不包含桌面报文编码。
use crate::ports::{DbCommand, DbReply};
use vigilshell_domain::workspace::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialIntent {
    Keep,
    Replace,
    Remove,
}
#[derive(Clone, Debug)]
pub enum CredentialChange {
    Keep,
    Replace { reference: String, label: String },
    Remove,
}

#[derive(Clone, Debug)]
pub struct DraftView {
    pub intent: CredentialIntent,
    pub proxy_id: Option<String>,
    pub id: String,
    pub connection_id: Option<String>,
    pub prepared: bool,
    pub saved: bool,
    pub auth: Option<AuthKind>,
    pub label: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ChallengeView {
    pub id: String,
    pub input: ConnectionInput,
    pub key: HostKey,
    pub previous: Option<String>,
    pub temporary: bool,
}
#[derive(Clone, Debug)]
pub struct OperationView {
    pub file_summary: Option<FileOperationSummary>,
    pub proxy: Option<ProxyRecord>,
    pub id: String,
    pub kind: String,
    pub phase: String,
    pub stage: String,
    pub error: Option<WorkspaceError>,
    pub draft: Option<DraftView>,
    pub connection: Option<ConnectionRecord>,
    pub challenge: Option<ChallengeView>,
    pub session_id: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEndKind {
    UserDisconnected,
    Cancelled,
    RemoteExit,
    RemoteClosed,
    TransportError,
}
#[derive(Clone, Debug)]
pub struct SessionEndReason {
    pub kind: SessionEndKind,
    pub message: Option<String>,
}
#[derive(Clone, Debug)]
pub struct SessionView {
    pub agent_control: Option<vigilshell_domain::agent::WorkspaceAgentControl>,
    pub group_id: String,
    pub id: String,
    pub operation_id: String,
    pub connection: ConnectionRecord,
    pub phase: String,
    pub error: Option<WorkspaceError>,
    pub exit_code: Option<u32>,
    pub end_reason: Option<SessionEndReason>,
}
#[derive(Clone, Debug)]
pub enum WorkspaceNotice {
    Tools(Box<vigilshell_domain::tools::ToolSnapshot>),
    Transfer(Box<vigilshell_domain::tools::TransferView>),
    InputRejected {
        session_id: String,
        code: String,
    },
    Tunnels {
        connection_id: String,
        states: Vec<TunnelStatus>,
    },
    Catalogue,
    Operation(Box<OperationView>),
    Session(Box<SessionView>),
    Detached,
}
#[derive(Clone, Debug)]
pub struct SaveRequest {
    pub connection_id: Option<String>,
    pub expected_version: Option<u64>,
    pub group_id: Option<String>,
    pub input: ConnectionInput,
    pub draft_id: Option<String>,
}
#[derive(Clone, Debug)]
pub struct TestRequest {
    pub connection_id: Option<String>,
    pub input: Option<ConnectionInput>,
    pub draft_id: Option<String>,
}
#[derive(Clone, Debug)]
pub enum WorkspaceAction {
    Tools(crate::tools::ToolAction),
    BeginProxyDraft(Option<String>),
    SaveProxy(ProxySave),
    DeleteProxy {
        id: String,
        version: u64,
    },
    TunnelStatuses(String),
    RetryTunnel {
        connection: String,
        rule: String,
    },
    ValidateInput {
        session: String,
        text: String,
    },

    ReconcileCredentials(String),
    Database(DbCommand),
    Get(String),
    BeginDraft(Option<String>),
    ConfigureDraft {
        draft: String,
        kind: AuthKind,
    },
    ClearDraft(String),
    DiscardDraft(String),
    Save(SaveRequest),
    Test(TestRequest),
    Confirm {
        challenge: String,
        accepted: bool,
    },
    ForgetTrust {
        id: String,
        version: u64,
    },
    Open {
        id: String,
        columns: u16,
        rows: u16,
    },
    Sessions,
    Resize {
        id: String,
        columns: u16,
        rows: u16,
    },
    Close(String),
    Delete {
        id: String,
        version: u64,
        disconnect: bool,
        sessions: Vec<String>,
    },
    Operation(String),
    Cancel(String),
}
#[derive(Clone, Debug)]
pub enum WorkspaceResult {
    Tools(crate::tools::ToolResult),
    Tunnels(Vec<TunnelStatus>),
    Database(DbReply),
    Connection(ConnectionRecord, Option<HostTrust>),
    Draft(DraftView),
    Operation(Box<OperationView>),
    Sessions(Vec<SessionView>),
    Done,
}

#[derive(Clone, Debug)]
pub struct FileOperationSummary {
    pub connection_id: String,
    pub attempt: u32,
    pub elapsed_ms: u64,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
}
