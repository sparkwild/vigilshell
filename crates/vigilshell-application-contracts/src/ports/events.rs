use super::FutureResult;
use crate::workspace::WorkspaceNotice;
pub trait WorkspaceSink: Send + Sync {
    fn end(&self, owner: String, session: String, sequence: u64) -> FutureResult<'_, ()>;
    fn event(&self, owner: String, event: WorkspaceNotice) -> FutureResult<'_, ()>;
    fn output(
        &self,
        owner: String,
        session: String,
        sequence: u64,
        data: Vec<u8>,
    ) -> FutureResult<'_, ()>;
    fn input_ack(
        &self,
        owner: String,
        session: String,
        sequence: u64,
        length: u32,
    ) -> FutureResult<'_, ()>;
}
