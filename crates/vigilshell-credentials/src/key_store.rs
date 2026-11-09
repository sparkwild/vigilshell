//! 系统主密钥存储是密码库组件内部的技术端口。
use vigilshell_domain::workspace::WorkspaceError;
use zeroize::Zeroizing;
pub type VaultKeyMaterial = Zeroizing<Vec<u8>>;
/// 同步系统调用由密码库在独立线程执行，不占用数据库线程或 Tokio 阻塞池。
pub trait VaultKeyStore: Send + Sync {
    fn read(&self, namespace: &str, version: u32) -> Result<VaultKeyMaterial, WorkspaceError>;
    /// 返回 false 表示同名项已存在，调用方必须读取它，不能覆盖。
    fn create(&self, namespace: &str, version: u32, key: &[u8]) -> Result<bool, WorkspaceError>;
}
