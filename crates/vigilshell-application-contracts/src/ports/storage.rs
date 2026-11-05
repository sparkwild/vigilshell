//! 应用用例需要的存储操作，由 SQLite 适配器实现。
use super::FutureResult;
use crate::workspace::CredentialChange;
use vigilshell_domain::{Preferences, workspace::*};

#[derive(Clone, Debug)]
pub struct VaultMetadata {
    pub namespace: String,
    pub key_version: u32,
    pub initialized: bool,
}

#[derive(Clone, Debug)]
pub struct EncryptedCredential {
    pub reference: String,
    pub context: CredentialContext,
    pub format_version: u32,
    pub key_version: u32,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CredentialJob {
    pub id: String,
    pub connection_id: String,
    pub new_ref: Option<String>,
    pub old_ref: Option<String>,
    pub action: String,
}

#[derive(Clone, Debug)]
pub enum DbCommand {
    ConfigurationReady,
    WithConfigurationCipher {
        cipher: std::sync::Arc<dyn ConnectionConfigCipher>,
        command: Box<DbCommand>,
    },
    Execution(super::ExecutionDbCommand),
    Agent(super::AgentDbCommand),
    AgentHistory(super::AgentHistoryCommand),
    VaultMetadata,
    InitializeVault {
        namespace: String,
        key_version: u32,
    },
    ReadEncryptedCredential(String),
    WriteEncryptedCredential(EncryptedCredential),
    RemoveEncryptedCredential(String),
    Preferences,
    ListProxies {
        offset: u32,
        version: Option<u64>,
    },
    GetProxy(String),
    SaveProxy {
        proxy: ProxyRecord,
        expected_version: Option<u64>,
    },
    DeleteProxy {
        id: String,
        version: u64,
    },
    ProxyReferences {
        id: String,
        offset: u32,
        version: Option<u64>,
    },

    UpdateLayout(GroupLayout),
    Namespace,
    List {
        query: String,
        offset: u32,
        limit: u32,
        kind: String,
        version: Option<u64>,
    },
    Get(String),
    Trust(String),
    CreateGroup(String),
    UpdateGroup {
        id: String,
        version: u64,
        name: String,
    },
    DeleteGroup {
        id: String,
        version: u64,
    },
    Save {
        id: String,
        expected_version: Option<u64>,
        group_id: Option<String>,
        input: ConnectionInput,
        credential: CredentialChange,
        trust: Option<HostKey>,
    },
    SetTrust {
        id: String,
        revision: u64,
        key: HostKey,
    },
    ForgetTrust {
        id: String,
        version: u64,
    },
    PrepareCredential(CredentialJob),
    FinishCredential(String),
    CredentialJobs,
    BeginDeletion {
        id: String,
        version: u64,
        job: String,
    },
    Delete(String),
}
#[derive(Clone, Debug)]
pub enum DbReply {
    Execution(super::ExecutionDbReply),
    Agent(super::AgentDbReply),
    AgentHistory(super::AgentHistoryReply),
    VaultMetadata(VaultMetadata),
    EncryptedCredential(EncryptedCredential),
    Preferences(Preferences),
    Proxies {
        proxies: Vec<ProxyRecord>,
        version: u64,
        next: Option<u32>,
    },
    Proxy(ProxyRecord),
    References {
        names: Vec<String>,
        version: u64,
        next: Option<u32>,
    },
    Namespace(String),
    Catalogue(Catalogue),
    Group(ConnectionGroup),
    LayoutRevision(u64),
    Connection(ConnectionRecord),
    Trust(Option<HostTrust>),
    Jobs(Vec<CredentialJob>),
    Done,
}
pub trait WorkspaceRepository: Send {
    fn execute(&mut self, command: DbCommand) -> Result<DbReply, WorkspaceError>;
}
pub trait WorkspaceDatabase: Send + Sync {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply>;
}

/// 已解锁的配置加密能力；数据库线程只调用内存密码运算，不调用系统授权。
/// 密文必须绑定存储命名空间、用途和连接 ID；实现不得保留明文副本。
pub trait ConnectionConfigCipher: Send + Sync + std::fmt::Debug {
    fn seal(&self, id: &str, plaintext: &[u8]) -> Result<Vec<u8>, WorkspaceError>;
    fn open(
        &self,
        id: &str,
        ciphertext: &[u8],
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, WorkspaceError>;
}

impl DbCommand {
    /// 仅实际读取或写入加密连接正文的命令依赖配置密钥；明文元数据和历史独立可用。
    pub fn needs_connection_configuration(&self) -> bool {
        match self {
            Self::ConfigurationReady
            | Self::Get(_)
            | Self::Save { .. }
            | Self::SetTrust { .. }
            | Self::ForgetTrust { .. }
            | Self::BeginDeletion { .. }
            | Self::Delete(_)
            | Self::DeleteProxy { .. }
            | Self::ProxyReferences { .. } => true,
            Self::List { kind, .. } => kind != "groups",
            Self::Execution(_)
            | Self::Agent(_)
            | Self::AgentHistory(_)
            | Self::VaultMetadata
            | Self::InitializeVault { .. }
            | Self::ReadEncryptedCredential(_)
            | Self::WriteEncryptedCredential(_)
            | Self::RemoveEncryptedCredential(_)
            | Self::Preferences
            | Self::Namespace
            | Self::WithConfigurationCipher { .. }
            | Self::ListProxies { .. }
            | Self::GetProxy(_)
            | Self::SaveProxy { .. }
            | Self::UpdateLayout(_)
            | Self::Trust(_)
            | Self::CreateGroup(_)
            | Self::UpdateGroup { .. }
            | Self::DeleteGroup { .. }
            | Self::PrepareCredential(_)
            | Self::FinishCredential(_)
            | Self::CredentialJobs => false,
        }
    }
}
