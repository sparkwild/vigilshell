use super::FutureResult;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
};
use vigilshell_domain::tools::*;
pub struct TransferIo {
    pub local: PathBuf,
    pub remote: String,
    pub upload: bool,
    pub overwrite: bool,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
}
pub trait ServerTools: Send + Sync {
    fn identities(&self, _uids: Vec<u32>, _gids: Vec<u32>) -> FutureResult<'_, Vec<IdentityName>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn sample(&self, full: bool) -> FutureResult<'_, RawSample>;
    fn list(
        &self,
        path: String,
        cancel: Arc<AtomicBool>,
    ) -> FutureResult<'_, (String, Vec<RemoteEntry>)>;
    fn stat(&self, _path: String) -> FutureResult<'_, RemoteEntry> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn create_file(&self, _path: String) -> FutureResult<'_, ()> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn permissions(
        &self,
        _path: String,
        _set: u32,
        _clear: u32,
        _expected: RemoteEntry,
    ) -> FutureResult<'_, ()> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn mkdir(&self, path: String) -> FutureResult<'_, ()>;
    fn rename(&self, from: String, to: String) -> FutureResult<'_, ()>;
    fn remove(&self, path: String, directory: bool) -> FutureResult<'_, ()>;
    fn transfer(&self, io: TransferIo) -> FutureResult<'_, ()>;
    fn transfer_tree(&self, tree: TransferTreeIo) -> FutureResult<'_, ()> {
        self.transfer(tree.io)
    }
    fn archive(&self, _io: ArchiveIo) -> FutureResult<'_, ()> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn edit_version(&self, _path: String) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn edit_download(&self, _path: String, _local: PathBuf) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn edit_upload(
        &self,
        _path: String,
        _local: PathBuf,
        _expected: Option<RemoteFileVersion>,
    ) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async { Err(vigilshell_domain::workspace::WorkspaceError::FilesUnsupported) })
    }
    fn close(&self) -> FutureResult<'_, ()>;
}

#[derive(Clone, Debug)]
pub struct TransferTreeEntry {
    pub local: PathBuf,
    pub remote: String,
    pub source: RemoteEntry,
    pub phase: String,
    pub error: Option<String>,
    pub transferred: u64,
}
#[derive(Default)]
pub struct TransferTreeState {
    pub entries: Vec<TransferTreeEntry>,
    pub enumerated: bool,
    pub stage: String,
    pub conflict_decision: Option<bool>,
}
pub struct TransferTreeIo {
    pub io: TransferIo,
    pub state: Arc<Mutex<TransferTreeState>>,
    pub batch_policy: Arc<Mutex<Option<bool>>>,
}
pub struct ArchiveIo {
    pub io: TransferIo,
    pub paths: Vec<String>,
    pub state: Arc<Mutex<TransferTreeState>>,
}
