//! 本地 SSH 命令任务端口；不接受客户端提供的已检查命令、权限或连接对象。
use super::*;
use std::sync::Arc;
use vigilshell_domain::agent::*;

#[derive(Clone, Debug)]
pub enum ExecutionDbCommand {
    Register(Box<ExecutionTask>),
    FindRequest(Box<ExecutionRequest>),
    GetAny {
        id: String,
    },
    Get {
        entry: String,
        id: String,
    },
    Save {
        task: Box<ExecutionTask>,
        expected: u64,
    },
    ReserveTerminalInput {
        entry: String,
        key: String,
        task_id: String,
        fingerprint: String,
        created_at: u64,
    },
    Recover,
}
#[derive(Clone, Debug)]
pub enum ExecutionDbReply {
    Registered {
        task: Box<ExecutionTask>,
        inserted: bool,
    },
    Task(Option<Box<ExecutionTask>>),
    InputReserved {
        inserted: bool,
    },
    Done,
}

pub struct ExecutionAccess {
    pub policy: ConnectionPolicy,
    pub connection: Arc<dyn AgentConnection>,
}
/// 由可信应用会话提供：核对接入代次、会话所有权、本人认证、当前连接/策略版本及撤销。
/// 每次调用必须读当前状态，不得只返回接单快照；不创建/准备自有远端运行器。
pub trait ExecutionAuthority: Send + Sync {
    fn current(&self, identity: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess>;
}
