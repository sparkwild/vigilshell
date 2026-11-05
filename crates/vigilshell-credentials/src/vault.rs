//! A single unlock belongs to the Host, not to the request that first needed it.
use crate::MAX_SECRET;
use crate::key_store::VaultKeyStore;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{AeadInOut, KeyInit},
};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, oneshot};

use vigilshell_application_contracts::ports::{
    DbCommand, DbReply, EncryptedCredential, FutureResult, SecretDiagnostic, SecretMaterial,
    SecretStore, VaultMetadata, WorkspaceDatabase,
};
use vigilshell_domain::workspace::*;
use zeroize::Zeroizing;

pub(super) enum Unlock {
    Locked,
    Reading,
    Ready(VaultMetadata, Zeroizing<Vec<u8>>),
    Blocked(WorkspaceError),
    Closed,
}
pub(super) struct Inner {
    pub(super) database: Arc<dyn WorkspaceDatabase>,
    keys: Arc<dyn VaultKeyStore>,
    pub(super) state: Mutex<Unlock>,
    changed: Notify,
    pub(super) access: tokio::sync::watch::Sender<ConnectionAccessState>,
    diagnostics: Mutex<std::collections::VecDeque<(String, Arc<dyn SecretDiagnostic>)>>,
}
pub struct EncryptedSecretStore {
    pub(super) inner: Arc<Inner>,
}
impl EncryptedSecretStore {
    pub fn new(database: Arc<dyn WorkspaceDatabase>, keys: Arc<dyn VaultKeyStore>) -> Self {
        Self {
            inner: Arc::new(Inner {
                database,
                keys,
                state: Mutex::new(Unlock::Locked),
                changed: Notify::new(),
                access: tokio::sync::watch::channel(ConnectionAccessState::default()).0,
                diagnostics: Mutex::new(std::collections::VecDeque::new()),
            }),
        }
    }
    pub(super) async fn unlock(&self, namespace: &str) -> Result<(), WorkspaceError> {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.inner.state.lock().unwrap();
                match &*state {
                    Unlock::Ready(meta, _) => {
                        return if meta.namespace == namespace {
                            Ok(())
                        } else {
                            Err(WorkspaceError::CredentialCorrupt)
                        };
                    }
                    Unlock::Blocked(e) => return Err(*e),
                    Unlock::Closed => return Err(WorkspaceError::Cancelled),
                    Unlock::Reading => {}
                    Unlock::Locked => {
                        *state = Unlock::Reading;
                        let inner = self.inner.clone();
                        tokio::spawn(async move {
                            let result = load_key(&inner).await;
                            let mut state = inner.state.lock().unwrap();
                            if matches!(*state, Unlock::Reading) {
                                *state = match result {
                                    Ok((m, k)) => Unlock::Ready(m, k),
                                    Err(e) => Unlock::Blocked(e),
                                };
                            }
                            drop(state);
                            inner.changed.notify_waiters();
                        });
                    }
                }
            }
            changed.await;
        }
    }
    fn require_ready(&self, namespace: &str) -> Result<(), WorkspaceError> {
        self.configuration_cipher()?;
        let state = self.inner.state.lock().unwrap();
        match &*state {
            Unlock::Ready(meta, _) if meta.namespace == namespace => Ok(()),
            Unlock::Ready(..) => Err(WorkspaceError::CredentialCorrupt),
            Unlock::Blocked(error) => Err(*error),
            Unlock::Closed => Err(WorkspaceError::Cancelled),
            _ => Err(WorkspaceError::ConfigurationLocked),
        }
    }
    fn with_key<T>(
        &self,
        work: impl FnOnce(&VaultMetadata, &[u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let state = self.inner.state.lock().unwrap();
        match &*state {
            Unlock::Ready(meta, key) => work(meta, key),
            _ => Err(WorkspaceError::Cancelled),
        }
    }
}

async fn load_key(
    inner: &Arc<Inner>,
) -> Result<(VaultMetadata, Zeroizing<Vec<u8>>), WorkspaceError> {
    let DbReply::VaultMetadata(mut meta) = inner.database.execute(DbCommand::VaultMetadata).await?
    else {
        return Err(WorkspaceError::StorageUnavailable);
    };
    if meta.key_version != 1 {
        return Err(WorkspaceError::CredentialFormatUnsupported);
    }
    if matches!(*inner.state.lock().unwrap(), Unlock::Closed) {
        return Err(WorkspaceError::Cancelled);
    }
    let keys = inner.keys.clone();
    let metadata = meta.clone();
    let (tx, rx) = oneshot::channel();
    // System authorization can outlive its waiter. Never put this call on Tokio's
    // shutdown-waited blocking pool, or wait while holding the database transaction.
    std::thread::Builder::new()
        .name("vault-keychain".into())
        .spawn(move || {
            let result = (|| {
                let key = match keys.read(&metadata.namespace, metadata.key_version) {
                    Ok(key) => key,
                    Err(WorkspaceError::VaultKeyMissing) if !metadata.initialized => {
                        let mut key = Zeroizing::new(vec![0u8; 32]);
                        getrandom::fill(&mut key)
                            .map_err(|_| WorkspaceError::CredentialUnavailable)?;
                        if keys.create(&metadata.namespace, metadata.key_version, &key)? {
                            key
                        } else {
                            keys.read(&metadata.namespace, metadata.key_version)?
                        }
                    }
                    Err(e) => return Err(e),
                };
                if key.len() != 32 {
                    return Err(WorkspaceError::VaultKeyInvalid);
                }
                Ok(key)
            })();
            let _ = tx.send(result);
        })
        .map_err(|_| WorkspaceError::CredentialUnavailable)?;
    let key = rx
        .await
        .map_err(|_| WorkspaceError::CredentialUnavailable)??;
    if matches!(*inner.state.lock().unwrap(), Unlock::Closed) {
        return Err(WorkspaceError::Cancelled);
    }
    if !meta.initialized {
        inner
            .database
            .execute(DbCommand::InitializeVault {
                namespace: meta.namespace.clone(),
                key_version: meta.key_version,
            })
            .await?;
        meta.initialized = true;
    }
    Ok((meta, key))
}

// Length-prefixed fields prevent ambiguous concatenation. Context always comes
// from the caller's frozen connection/proxy identity, not the ciphertext row.
fn aad(
    namespace: &str,
    reference: &str,
    context: &CredentialContext,
    format: u32,
    key_version: u32,
) -> Vec<u8> {
    let mut value = b"VigilShell credential\0".to_vec();
    value.extend(format.to_be_bytes());
    value.extend(key_version.to_be_bytes());
    for field in [
        namespace,
        reference,
        &context.owner_id,
        context.purpose.code(),
    ] {
        value.extend((field.len() as u64).to_be_bytes());
        value.extend(field.as_bytes());
    }
    value
}
impl SecretStore for EncryptedSecretStore {
    fn read<'a>(
        &'a self,
        namespace: &'a str,
        reference: &'a str,
        context: CredentialContext,
    ) -> FutureResult<'a, SecretMaterial> {
        Box::pin(async move {
            let DbReply::EncryptedCredential(record) = self
                .inner
                .database
                .execute(DbCommand::ReadEncryptedCredential(reference.into()))
                .await?
            else {
                return Err(WorkspaceError::StorageUnavailable);
            };
            if record.format_version != 1 || record.key_version != 1 {
                return Err(WorkspaceError::CredentialFormatUnsupported);
            }
            if record.reference != reference
                || record.context != context
                || record.nonce.len() != 24
                || record.ciphertext.len() < 17
                || record.ciphertext.len() > MAX_SECRET + 16
            {
                return Err(WorkspaceError::CredentialCorrupt);
            }
            self.require_ready(namespace)?;
            let mut secret = self.with_key(|meta, key| {
                if record.key_version != meta.key_version {
                    return Err(WorkspaceError::CredentialFormatUnsupported);
                }
                let cipher = XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| WorkspaceError::VaultKeyInvalid)?;
                let mut bytes = Zeroizing::new(record.ciphertext);
                cipher
                    .decrypt_in_place(
                        <&XNonce>::try_from(record.nonce.as_slice())
                            .map_err(|_| WorkspaceError::CredentialCorrupt)?,
                        &aad(
                            namespace,
                            reference,
                            &context,
                            record.format_version,
                            record.key_version,
                        ),
                        &mut *bytes,
                    )
                    .map_err(|_| WorkspaceError::CredentialCorrupt)?;
                Ok(SecretMaterial {
                    diagnostic: None,
                    kind: context.purpose.kind(),
                    bytes,
                    label: String::new(),
                })
            })?;
            secret.diagnostic = self
                .inner
                .diagnostics
                .lock()
                .unwrap()
                .iter()
                .find(|(id, _)| id == reference)
                .map(|(_, trace)| trace.clone());
            if let Some(trace) = &secret.diagnostic {
                trace.check("vault_read", &secret.bytes);
            }
            Ok(secret)
        })
    }
    fn write<'a>(
        &'a self,
        namespace: &'a str,
        reference: &'a str,
        secret: &'a SecretMaterial,
        context: CredentialContext,
    ) -> FutureResult<'a, ()> {
        Box::pin(async move {
            if secret.bytes.is_empty()
                || secret.bytes.len() > MAX_SECRET
                || secret.kind != context.purpose.kind()
            {
                return Err(WorkspaceError::InvalidInput);
            }
            self.require_ready(namespace)?;
            if let Some(trace) = &secret.diagnostic {
                trace.check("vault_write_input", &secret.bytes);
            }
            let record = self.with_key(|meta, key| {
                let mut nonce = vec![0u8; 24];
                getrandom::fill(&mut nonce).map_err(|_| WorkspaceError::CredentialUnavailable)?;
                let cipher = XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| WorkspaceError::VaultKeyInvalid)?;
                let mut bytes = Zeroizing::new(secret.bytes.to_vec());
                cipher
                    .encrypt_in_place(
                        <&XNonce>::try_from(nonce.as_slice())
                            .map_err(|_| WorkspaceError::CredentialCorrupt)?,
                        &aad(namespace, reference, &context, 1, meta.key_version),
                        &mut *bytes,
                    )
                    .map_err(|_| WorkspaceError::CredentialUnavailable)?;
                Ok(EncryptedCredential {
                    reference: reference.into(),
                    context: context.clone(),
                    format_version: 1,
                    key_version: meta.key_version,
                    nonce,
                    ciphertext: std::mem::take(&mut *bytes),
                })
            })?;
            self.inner
                .database
                .execute(DbCommand::WriteEncryptedCredential(record))
                .await?;
            if let Some(trace) = &secret.diagnostic {
                {
                    let state = self.inner.state.lock().unwrap();
                    if matches!(*state, Unlock::Ready(..)) {
                        let mut markers = self.inner.diagnostics.lock().unwrap();
                        markers.retain(|(id, _)| id != reference);
                        markers.push_back((reference.into(), trace.clone()));
                        while markers.len() > 64 {
                            markers.pop_front();
                        }
                    }
                }
                // Diagnostic reads neither rewrite the credential nor change the save result.
                match self.read(namespace, reference, context).await {
                    Ok(restored) => {
                        trace.record("vault_write_readback", Some(restored.bytes == secret.bytes))
                    }
                    Err(error) => {
                        trace.record("vault_write_readback", None);
                        tracing::info!(target: "vigilshell_auth_diagnostics", stage="credential_readback", trace_id=trace.id(), code=error.code());
                    }
                }
            }
            Ok(())
        })
    }
    fn remove<'a>(&'a self, namespace: &'a str, reference: &'a str) -> FutureResult<'a, ()> {
        Box::pin(async move {
            let DbReply::VaultMetadata(meta) = self
                .inner
                .database
                .execute(DbCommand::VaultMetadata)
                .await?
            else {
                return Err(WorkspaceError::StorageUnavailable);
            };
            if meta.namespace != namespace {
                return Err(WorkspaceError::CredentialCorrupt);
            }
            self.inner
                .database
                .execute(DbCommand::RemoveEncryptedCredential(reference.into()))
                .await?;
            self.inner
                .diagnostics
                .lock()
                .unwrap()
                .retain(|(id, _)| id != reference);
            Ok(())
        })
    }
    fn close(&self) {
        *self.inner.state.lock().unwrap() = Unlock::Closed;
        self.inner.diagnostics.lock().unwrap().clear();
        self.inner.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests;
