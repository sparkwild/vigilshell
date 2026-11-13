//! 连接配置和凭据共享运行时主密钥，但使用独立密文格式和认证上下文。
use crate::vault::{EncryptedSecretStore, Inner, Unlock};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{AeadInOut, KeyInit},
};
use std::sync::{Arc, Weak};
use vigilshell_application_contracts::ports::{ConnectionConfigCipher, DbCommand, DbReply};
use vigilshell_domain::workspace::{ConnectionAccessPhase, ConnectionAccessState, WorkspaceError};
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"VSC1";
const MAX_CONFIG: usize = 1024 * 1024;
struct ConfigurationCipher(Weak<Inner>);
impl std::fmt::Debug for ConfigurationCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfigurationCipher")
    }
}
impl EncryptedSecretStore {
    pub fn access_state(&self) -> ConnectionAccessState {
        self.inner.access.borrow().clone()
    }
    pub fn subscribe_access(&self) -> tokio::sync::watch::Receiver<ConnectionAccessState> {
        self.inner.access.subscribe()
    }
    /// Starts one attempt and returns immediately. Only an explicit retry clears a failure.
    pub fn initialize(self: &Arc<Self>, retry: bool) -> ConnectionAccessState {
        let mut key = self.inner.state.lock().unwrap();
        if matches!(*key, Unlock::Closed) {
            return self.access_state();
        }
        let current = self.access_state();
        if !(matches!(current.phase, ConnectionAccessPhase::Locked)
            || retry && matches!(current.phase, ConnectionAccessPhase::Blocked))
        {
            return current;
        }
        if retry && matches!(*key, Unlock::Blocked(_)) {
            *key = Unlock::Locked;
        }
        let revision = current.revision + 1;
        self.inner.access.send_replace(ConnectionAccessState {
            phase: ConnectionAccessPhase::Unlocking,
            revision,
            code: None,
        });
        drop(key);
        let this = self.clone();
        tokio::spawn(async move {
            let result = async {
                let cipher = this.unlock_configuration().await?;
                this.inner
                    .database
                    .execute(DbCommand::WithConfigurationCipher {
                        cipher,
                        command: Box::new(DbCommand::ConfigurationReady),
                    })
                    .await?;
                Ok::<(), WorkspaceError>(())
            }
            .await;
            let mut key = this.inner.state.lock().unwrap();
            if matches!(*key, Unlock::Closed) || this.access_state().revision != revision {
                return;
            }
            if let Err(error) = result {
                *key = Unlock::Blocked(error);
            }
            this.inner.access.send_replace(ConnectionAccessState {
                phase: if result.is_ok() {
                    ConnectionAccessPhase::Ready
                } else {
                    ConnectionAccessPhase::Blocked
                },
                revision: revision + 1,
                code: result.err().map(|e| e.code().into()),
            });
            // Dev supervision also needs completion when initialization fails before a key read.
            tracing::info!(
                application = "vigilshell",
                pid = std::process::id(),
                stage = "vault_initialization",
                outcome = "finished"
            );
        });
        self.access_state()
    }
    pub fn configuration_cipher(&self) -> Result<Arc<dyn ConnectionConfigCipher>, WorkspaceError> {
        if self.access_state().phase == ConnectionAccessPhase::Ready {
            return Ok(Arc::new(ConfigurationCipher(Arc::downgrade(&self.inner))));
        }
        match *self.inner.state.lock().unwrap() {
            Unlock::Blocked(error) => Err(error),
            Unlock::Closed => Err(WorkspaceError::Cancelled),
            _ => Err(WorkspaceError::ConfigurationLocked),
        }
    }
    /// Failed authentication discards the key; only a subsequent user retry can reload it.
    pub fn block_configuration(&self, error: WorkspaceError) {
        let mut state = self.inner.state.lock().unwrap();
        if matches!(*state, Unlock::Ready(..)) {
            *state = Unlock::Blocked(error);
            self.inner.access.send_modify(|value| {
                value.phase = ConnectionAccessPhase::Blocked;
                value.revision += 1;
                value.code = Some(error.code().into());
            });
        }
    }
    pub async fn unlock_configuration(
        &self,
    ) -> Result<Arc<dyn ConnectionConfigCipher>, WorkspaceError> {
        let DbReply::VaultMetadata(meta) = self
            .inner
            .database
            .execute(DbCommand::VaultMetadata)
            .await?
        else {
            return Err(WorkspaceError::StorageUnavailable);
        };
        self.unlock(&meta.namespace).await?;
        Ok(Arc::new(ConfigurationCipher(Arc::downgrade(&self.inner))))
    }
}
fn aad(namespace: &str, id: &str, version: u32) -> Vec<u8> {
    let mut data = b"VigilShell connection configuration\0".to_vec();
    data.extend(MAGIC);
    data.extend(version.to_be_bytes());
    for field in [namespace, id] {
        data.extend((field.len() as u64).to_be_bytes());
        data.extend(field.as_bytes());
    }
    data
}
impl ConfigurationCipher {
    fn with_key<T>(
        &self,
        work: impl FnOnce(&str, u32, &[u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let inner = self
            .0
            .upgrade()
            .ok_or(WorkspaceError::ConfigurationLocked)?;
        let state = inner.state.lock().unwrap();
        match &*state {
            Unlock::Ready(meta, key) => work(&meta.namespace, meta.key_version, key),
            Unlock::Blocked(error) => Err(*error),
            _ => Err(WorkspaceError::ConfigurationLocked),
        }
    }
}
impl ConnectionConfigCipher for ConfigurationCipher {
    fn seal(&self, id: &str, plaintext: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
        if plaintext.len() > MAX_CONFIG {
            return Err(WorkspaceError::InvalidInput);
        }
        self.with_key(|namespace, version, key| {
            let cipher = XChaCha20Poly1305::new_from_slice(key)
                .map_err(|_| WorkspaceError::VaultKeyInvalid)?;
            let mut nonce = [0; 24];
            getrandom::fill(&mut nonce).map_err(|_| WorkspaceError::CredentialUnavailable)?;
            let mut body = Zeroizing::new(plaintext.to_vec());
            cipher
                .encrypt_in_place(
                    <&XNonce>::try_from(nonce.as_slice())
                        .map_err(|_| WorkspaceError::ConfigurationCorrupt)?,
                    &aad(namespace, id, version),
                    &mut *body,
                )
                .map_err(|_| WorkspaceError::ConfigurationCorrupt)?;
            let mut result = MAGIC.to_vec();
            result.extend(version.to_be_bytes());
            result.extend(nonce);
            result.extend_from_slice(&body);
            Ok(result)
        })
    }
    fn open(&self, id: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, WorkspaceError> {
        if ciphertext.len() < 48 || ciphertext.len() > MAX_CONFIG + 48 {
            return Err(WorkspaceError::ConfigurationCorrupt);
        }
        if &ciphertext[..4] != MAGIC || ciphertext[4..8] != 1u32.to_be_bytes() {
            return Err(WorkspaceError::ConfigurationFormatUnsupported);
        }
        self.with_key(|namespace, version, key| {
            let cipher = XChaCha20Poly1305::new_from_slice(key)
                .map_err(|_| WorkspaceError::VaultKeyInvalid)?;
            let mut body = Zeroizing::new(ciphertext[32..].to_vec());
            cipher
                .decrypt_in_place(
                    <&XNonce>::try_from(&ciphertext[8..32])
                        .map_err(|_| WorkspaceError::ConfigurationCorrupt)?,
                    &aad(namespace, id, version),
                    &mut *body,
                )
                .map_err(|_| WorkspaceError::ConfigurationCorrupt)?;
            Ok(body)
        })
    }
}
