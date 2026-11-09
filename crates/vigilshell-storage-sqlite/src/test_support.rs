//! 仅供存储语义测试的非密码学替身；加密行为由 credentials 合成夹具验证。
use super::*;
use std::sync::Arc;
use vigilshell_application_contracts::ports::{
    ConnectionConfigCipher, DbCommand, WorkspaceRepository,
};
use vigilshell_domain::workspace::WorkspaceError;
#[derive(Debug)]
pub(super) struct Cipher;
impl ConnectionConfigCipher for Cipher {
    fn seal(&self, id: &str, bytes: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
        let mut result = vec![0; 48];
        result.extend_from_slice(id.as_bytes());
        result.push(0);
        result.extend_from_slice(bytes);
        Ok(result)
    }
    fn open(&self, id: &str, bytes: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, WorkspaceError> {
        let prefix = self.seal(id, &[])?;
        let bytes = bytes
            .strip_prefix(prefix.as_slice())
            .ok_or(WorkspaceError::ConfigurationCorrupt)?;
        Ok(zeroize::Zeroizing::new(bytes.to_vec()))
    }
}
pub(super) fn open(path: &Path) -> Result<SqlitePreferences, SettingsError> {
    let mut store = SqlitePreferences::open(path)?;
    unlock(&mut store);
    Ok(store)
}
pub(super) fn unlock(store: &mut SqlitePreferences) {
    store
        .connection
        .execute("UPDATE credential_vault SET initialized=1", [])
        .unwrap();
    store
        .execute(DbCommand::WithConfigurationCipher {
            cipher: Arc::new(Cipher),
            command: Box::new(DbCommand::ConfigurationReady),
        })
        .unwrap();
}
