//! 会话和任务管理分派；业务动作直接进入 RuntimeCall，不再复制一套工具参数。
use super::*;
#[derive(Clone, Debug)]
pub enum AgentAction {
    WorkspaceTerminal {
        session_id: String,
        command: WorkspaceTerminalAction,
    },
    Runtime(RuntimeCall),
    SessionOpen {
        tab_id: Option<String>,
        connection_id: String,
        request_key: String,
    },
    SessionGet {
        session_id: String,
    },
    SessionClose {
        session_id: String,
        request_key: String,
    },
    CapabilitiesGet {
        session_id: String,
    },
    TerminalWrite {
        task_id: String,
        text: String,
        request_key: String,
    },
    TerminalResize {
        task_id: String,
        columns: u16,
        rows: u16,
        request_key: String,
    },
    TerminalClose {
        task_id: String,
        request_key: String,
    },
    TasksList {
        session_id: Option<String>,
        connection_id: Option<String>,
        phase: Option<String>,
        offset: Option<u32>,
    },
    TaskGet {
        task_id: String,
    },
    TaskReadOutput {
        task_id: String,
        cursor: Option<u64>,
        view: Option<OutputView>,
    },
    TaskCancel {
        task_id: String,
        request_key: String,
    },
    ConnectionsList {},
}
#[derive(Clone, Debug)]
pub struct RuntimeCall {
    pub parent_task_id: Option<String>,
    pub session_id: String,
    pub request_key: String,
    pub timeout_seconds: Option<u32>,
    pub operation: RuntimeOperation,
}
impl AgentAction {
    pub fn into_runtime(self) -> Result<RuntimeCall, Box<Self>> {
        match self {
            Self::Runtime(call) => Ok(call),
            other => Err(Box::new(other)),
        }
    }
}
