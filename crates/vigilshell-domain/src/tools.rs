use crate::workspace::WorkspaceError;
#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub mount: String,
    pub total: u64,
    pub available: u64,
}
#[derive(Clone, Debug, Default)]
pub struct ServerSample {
    pub hostname: Option<String>,
    pub distribution: Option<String>,
    pub kernel: Option<String>,
    pub architecture: Option<String>,
    pub uptime: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub network_receive: Option<f64>,
    pub network_send: Option<f64>,
    pub memory_total: Option<u64>,
    pub memory_available: Option<u64>,
    pub disks: Vec<DiskUsage>,
    pub collected_at: u64,
    pub phase: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct RawSample {
    /// 网卡计数读取前取得的服务器启动秒数，用于计算速率时间差。
    pub network_time: Option<f64>,
    pub network: Option<std::collections::BTreeMap<String, (u64, u64)>>,
    pub view: ServerSample,
    pub cpu_total: Option<u64>,
    pub cpu_idle: Option<u64>,
    pub load: Option<[f64; 3]>,
    pub memory_free: Option<u64>,
    pub memory_shared: Option<u64>,
    pub memory_buffers: Option<u64>,
    pub swap_total: Option<u64>,
    pub swap_free: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: Option<u64>,
    pub modified: Option<u64>,
    pub permissions: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct DirectoryPage {
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    pub next: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ToolSnapshot {
    pub connection_id: String,
    pub group_id: String,
    pub sample: ServerSample,
}
#[derive(Clone, Debug)]
pub struct TransferConflict {
    pub source_path: Option<String>,
    pub target_path: String,
}
#[derive(Clone, Debug)]
pub struct TransferView {
    pub conflict: Option<TransferConflict>,
    pub created_at: u64,
    pub details: Option<TransferSummary>,
    pub batch_size: u32,
    pub batch_id: String,
    pub remote_path: String,
    pub remote_identity: String,
    pub attempt: u32,
    pub revision: u32,
    pub conflict_version: u32,
    pub can_retry: bool,
    pub id: String,
    pub connection_id: String,
    pub group_id: String,
    pub name: String,
    pub direction: String,
    pub phase: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct FileGrantView {
    pub id: String,
    pub names: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct IdentityName {
    pub id: u32,
    pub group: bool,
    pub name: String,
}

pub fn valid_remote_path(path: &str) -> Result<(), WorkspaceError> {
    if path.is_empty() || path.len() > 16384 || path.contains('\0') {
        Err(WorkspaceError::InvalidInput)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct TransferSummary {
    pub stage: String,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub pending: u32,
}
#[derive(Clone, Debug)]
pub struct TransferItem {
    pub path: String,
    pub phase: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct TransferItemsPage {
    pub items: Vec<TransferItem>,
    pub next: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct FileOperationItem {
    pub entry: RemoteEntry,
    pub status: String,
    pub error: Option<WorkspaceError>,
}
#[derive(Clone, Debug)]
pub struct FileOperationPage {
    pub id: String,
    pub phase: String,
    pub stage: String,
    pub error: Option<WorkspaceError>,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub pending: u32,
    pub items: Vec<FileOperationItem>,
    pub next: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFileVersion {
    pub hash: String,
    pub size: u64,
    pub modified: u64,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
}
#[derive(Clone, Debug)]
pub struct FileEditRemote {
    pub id: String,
    pub connection_id: String,
    pub group_id: String,
    pub path: String,
    pub version: RemoteFileVersion,
}
