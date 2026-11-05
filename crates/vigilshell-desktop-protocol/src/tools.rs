use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vigilshell_domain::tools as domain;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiskUsage {
    #[schemars(extend("maxLength" = 16384))]
    pub mount: String,
    #[schemars(extend("minimum" = 0))]
    pub total: u64,
    #[schemars(extend("minimum" = 0))]
    pub available: u64,
}
impl From<domain::DiskUsage> for DiskUsage {
    fn from(v: domain::DiskUsage) -> Self {
        Self {
            mount: v.mount,
            total: v.total,
            available: v.available,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerSample {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 256))]
    pub hostname: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 256))]
    pub distribution: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 256))]
    pub kernel: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 64))]
    pub architecture: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<f64>")]
    #[schemars(extend("minimum" = 0))]
    pub uptime: Option<f64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<f64>")]
    #[schemars(extend("maximum" = 100, "minimum" = 0))]
    pub cpu_percent: Option<f64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<f64>")]
    #[schemars(extend("minimum" = 0))]
    pub network_receive: Option<f64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<f64>")]
    #[schemars(extend("minimum" = 0))]
    pub network_send: Option<f64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("minimum" = 0))]
    pub memory_total: Option<u64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("minimum" = 0))]
    pub memory_available: Option<u64>,
    #[schemars(extend("maxItems" = 10000))]
    pub disks: Vec<DiskUsage>,
    #[schemars(extend("minimum" = 0))]
    pub collected_at: u64,
    #[schemars(extend("enum" = ["loading", "ready", "stale", "unavailable"]))]
    pub phase: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 128))]
    pub error: Option<String>,
}
impl From<domain::ServerSample> for ServerSample {
    fn from(v: domain::ServerSample) -> Self {
        Self {
            hostname: v.hostname,
            distribution: v.distribution,
            kernel: v.kernel,
            architecture: v.architecture,
            uptime: v.uptime,
            cpu_percent: v.cpu_percent,
            network_receive: v.network_receive,
            network_send: v.network_send,
            memory_total: v.memory_total,
            memory_available: v.memory_available,
            disks: v.disks.into_iter().map(Into::into).collect(),
            collected_at: v.collected_at,
            phase: v.phase,
            error: v.error,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteEntry {
    #[schemars(extend("maxLength" = 16384))]
    pub name: String,
    #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
    pub path: String,
    #[schemars(extend("enum" = ["file", "directory", "symlink", "other"]))]
    pub kind: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("minimum" = 0))]
    pub size: Option<u64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("minimum" = 0))]
    pub modified: Option<u64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub permissions: Option<u32>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub uid: Option<u32>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub gid: Option<u32>,
}
impl From<domain::RemoteEntry> for RemoteEntry {
    fn from(v: domain::RemoteEntry) -> Self {
        Self {
            name: v.name,
            path: v.path,
            kind: v.kind,
            size: v.size,
            modified: v.modified,
            permissions: v.permissions,
            uid: v.uid,
            gid: v.gid,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryPage {
    #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
    pub path: String,
    #[schemars(extend("maxItems" = 100))]
    pub entries: Vec<RemoteEntry>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 16))]
    pub next: Option<String>,
}
impl From<domain::DirectoryPage> for DirectoryPage {
    fn from(v: domain::DirectoryPage) -> Self {
        Self {
            path: v.path,
            entries: v.entries.into_iter().map(Into::into).collect(),
            next: v.next,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolSnapshot {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: String,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: String,
    pub sample: ServerSample,
}
impl From<domain::ToolSnapshot> for ToolSnapshot {
    fn from(v: domain::ToolSnapshot) -> Self {
        Self {
            connection_id: v.connection_id,
            group_id: v.group_id,
            sample: v.sample.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferConflict {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 16384))]
    pub source_path: Option<String>,
    #[schemars(extend("maxLength" = 16384))]
    pub target_path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferView {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<TransferConflict>")]
    pub conflict: Option<TransferConflict>,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<TransferSummary>,
    #[schemars(range(min = 1, max = 100))]
    pub batch_size: u32,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub batch_id: String,
    #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
    pub remote_path: String,
    #[schemars(extend("maxLength" = 2048))]
    pub remote_identity: String,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub attempt: u32,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u32,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub conflict_version: u32,
    pub can_retry: bool,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: String,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: String,
    #[schemars(extend("maxLength" = 16384))]
    pub name: String,
    #[schemars(extend("enum" = ["upload", "download"]))]
    pub direction: String,
    #[schemars(extend("enum" = ["queued", "running", "cancelling", "awaitingDecision", "complete", "cancelled", "failed", "skipped"]))]
    pub phase: String,
    #[schemars(extend("minimum" = 0))]
    pub bytes: u64,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("minimum" = 0))]
    pub total: Option<u64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 128))]
    pub error: Option<String>,
}
impl From<domain::TransferView> for TransferView {
    fn from(v: domain::TransferView) -> Self {
        Self {
            conflict: v.conflict.map(|c| TransferConflict {
                source_path: c.source_path,
                target_path: c.target_path,
            }),
            created_at: v.created_at,
            details: v.details.map(Into::into),
            batch_size: v.batch_size,
            batch_id: v.batch_id,
            remote_path: v.remote_path,
            remote_identity: v.remote_identity,
            attempt: v.attempt,
            revision: v.revision,
            conflict_version: v.conflict_version,
            can_retry: v.can_retry,
            id: v.id,
            connection_id: v.connection_id,
            group_id: v.group_id,
            name: v.name,
            direction: v.direction,
            phase: v.phase,
            bytes: v.bytes,
            total: v.total,
            error: v.error,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileGrantView {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[schemars(extend("maxItems" = 100))]
    pub names: Vec<String>,
}
impl From<domain::FileGrantView> for FileGrantView {
    fn from(v: domain::FileGrantView) -> Self {
        Self {
            id: v.id,
            names: v.names,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityName {
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub id: u32,
    pub group: bool,
    #[schemars(extend("maxLength" = 256))]
    pub name: String,
}
impl From<domain::IdentityName> for IdentityName {
    fn from(v: domain::IdentityName) -> Self {
        Self {
            id: v.id,
            group: v.group,
            name: v.name,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferSummary {
    #[schemars(extend("enum" = ["enumerating", "transferring", "archiving", "verifying", "finished"]))]
    pub stage: String,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub pending: u32,
}
impl From<domain::TransferSummary> for TransferSummary {
    fn from(v: domain::TransferSummary) -> Self {
        Self {
            stage: v.stage,
            completed: v.completed,
            failed: v.failed,
            skipped: v.skipped,
            pending: v.pending,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferItem {
    #[schemars(extend("maxLength" = 16384))]
    pub path: String,
    #[schemars(extend("enum" = ["pending", "complete", "failed", "skipped", "conflict"]))]
    pub phase: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("maxLength" = 128))]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferItemsPage {
    #[schemars(extend("maxItems" = 100))]
    pub items: Vec<TransferItem>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    pub next: Option<u32>,
}
impl From<domain::TransferItemsPage> for TransferItemsPage {
    fn from(v: domain::TransferItemsPage) -> Self {
        Self {
            items: v
                .items
                .into_iter()
                .map(|i| TransferItem {
                    path: i.path,
                    phase: i.phase,
                    error: i.error,
                })
                .collect(),
            next: v.next,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileOperationItem {
    pub entry: RemoteEntry,
    #[schemars(extend("enum" = ["pending", "complete", "failed", "skipped"]))]
    pub status: String,
    #[schemars(extend("maxLength" = 128))]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileOperationPage {
    #[schemars(extend("maxLength" = 64))]
    pub id: String,
    #[schemars(extend("enum" = ["working", "success", "failed", "cancelled"]))]
    pub phase: String,
    #[schemars(extend("enum" = ["scanning", "applying", "finished"]))]
    pub stage: String,
    #[schemars(extend("maxLength" = 128))]
    pub error: Option<String>,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub pending: u32,
    #[schemars(extend("maxItems" = 100))]
    pub items: Vec<FileOperationItem>,
    pub next: Option<u32>,
}
impl From<domain::FileOperationPage> for FileOperationPage {
    fn from(v: domain::FileOperationPage) -> Self {
        Self {
            id: v.id,
            phase: v.phase,
            stage: v.stage,
            error: v.error.map(|e| e.code().into()),
            completed: v.completed,
            failed: v.failed,
            skipped: v.skipped,
            pending: v.pending,
            items: v
                .items
                .into_iter()
                .map(|i| FileOperationItem {
                    entry: i.entry.into(),
                    status: i.status,
                    error: i.error.map(|e| e.code().into()),
                })
                .collect(),
            next: v.next,
        }
    }
}
