//! 运行任务保留接纳时授权；权限收紧阻止新的执行，不改变系统服务生命周期。
use super::*;
impl ConnectionPolicy {
    pub fn permits_existing(&self, old: &Self) -> bool {
        self.enabled()
            && self.connection_id == old.connection_id
            && self.connection_revision == old.connection_revision
            && self.execution.permits_existing(&old.execution)
    }
}
