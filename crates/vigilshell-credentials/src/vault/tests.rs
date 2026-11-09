use super::*;
use crate::key_store::VaultKeyStore;
use std::sync::{
    Condvar,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use vigilshell_application_contracts::ports::WorkspaceRepository;
use vigilshell_storage_sqlite::SqlitePreferences;

struct Database {
    repository: Mutex<SqlitePreferences>,
    fail_initialize: AtomicBool,
}
impl WorkspaceDatabase for Database {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if matches!(command, DbCommand::InitializeVault { .. })
                && self.fail_initialize.swap(false, Ordering::SeqCst)
            {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.repository.lock().unwrap().execute(command)
        })
    }
}
#[derive(Default)]
struct Keys {
    values: Mutex<std::collections::HashMap<String, Zeroizing<Vec<u8>>>>,
    reads: AtomicUsize,
    creates: AtomicUsize,
    error: Mutex<Option<WorkspaceError>>,
    gate: (Mutex<bool>, Condvar),
}
impl Keys {
    fn block(&self) {
        *self.gate.0.lock().unwrap() = true;
    }
    fn unblock(&self) {
        *self.gate.0.lock().unwrap() = false;
        self.gate.1.notify_all();
    }
}
impl VaultKeyStore for Keys {
    fn read(&self, ns: &str, _: u32) -> Result<Zeroizing<Vec<u8>>, WorkspaceError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let mut gate = self.gate.0.lock().unwrap();
        while *gate {
            gate = self.gate.1.wait(gate).unwrap();
        }
        if let Some(error) = *self.error.lock().unwrap() {
            return Err(error);
        }
        self.values
            .lock()
            .unwrap()
            .get(ns)
            .map(|k| Zeroizing::new(k.to_vec()))
            .ok_or(WorkspaceError::VaultKeyMissing)
    }
    fn create(&self, ns: &str, _: u32, key: &[u8]) -> Result<bool, WorkspaceError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        let mut values = self.values.lock().unwrap();
        if values.contains_key(ns) {
            return Ok(false);
        }
        values.insert(ns.into(), Zeroizing::new(key.to_vec()));
        Ok(true)
    }
}
struct Fixture {
    _directory: tempfile::TempDir,
    db: Arc<Database>,
    keys: Arc<Keys>,
    ns: String,
    store: Arc<EncryptedSecretStore>,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut repository = SqlitePreferences::open(directory.path()).unwrap();
        let DbReply::Namespace(ns) = repository.execute(DbCommand::Namespace).unwrap() else {
            panic!()
        };
        let db = Arc::new(Database {
            repository: Mutex::new(repository),
            fail_initialize: AtomicBool::new(false),
        });
        let keys = Arc::new(Keys::default());
        let store = Arc::new(EncryptedSecretStore::new(db.clone(), keys.clone()));
        Self {
            _directory: directory,
            db,
            keys,
            ns,
            store,
        }
    }
    fn restart(&mut self) {
        self.store.close();
        self.store = Arc::new(EncryptedSecretStore::new(
            self.db.clone(),
            self.keys.clone(),
        ));
    }
    async fn save(&self, r: &str, ctx: CredentialContext) {
        initialize(&self.store, false).await.unwrap();
        self.store
            .write(&self.ns, r, &secret(ctx.purpose.kind()), ctx)
            .await
            .unwrap();
    }
    async fn row(&self, r: &str) -> EncryptedCredential {
        let DbReply::EncryptedCredential(row) = self
            .db
            .execute(DbCommand::ReadEncryptedCredential(r.into()))
            .await
            .unwrap()
        else {
            panic!()
        };
        row
    }
    async fn replace(&self, row: EncryptedCredential) {
        self.db
            .execute(DbCommand::RemoveEncryptedCredential(row.reference.clone()))
            .await
            .unwrap();
        self.db
            .execute(DbCommand::WriteEncryptedCredential(row))
            .await
            .unwrap();
    }
}
async fn initialize(store: &Arc<EncryptedSecretStore>, retry: bool) -> Result<(), WorkspaceError> {
    let mut events = store.subscribe_access();
    store.initialize(retry);
    loop {
        if !matches!(store.access_state().phase, ConnectionAccessPhase::Unlocking) {
            return store.configuration_cipher().map(|_| ());
        }
        if matches!(*store.inner.state.lock().unwrap(), Unlock::Closed) {
            return Err(WorkspaceError::Cancelled);
        }
        events.changed().await.unwrap();
    }
}
fn context() -> CredentialContext {
    CredentialContext::login("connection", AuthKind::Password)
}
fn secret(kind: AuthKind) -> SecretMaterial {
    SecretMaterial {
        diagnostic: None,
        kind,
        bytes: b"synthetic-secret-that-must-not-be-stored".to_vec().into(),
        label: String::new(),
    }
}
async fn wait_read(keys: &Keys, count: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while keys.reads.load(Ordering::SeqCst) < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn all_purposes_roundtrip_and_context_is_authenticated() {
    let f = Fixture::new();
    for (i, ctx) in [
        context(),
        CredentialContext::login("key-connection", AuthKind::PrivateKey),
        CredentialContext::proxy("proxy"),
    ]
    .into_iter()
    .enumerate()
    {
        let reference = format!("reference-{i}");
        f.save(&reference, ctx.clone()).await;
        let value = f.store.read(&f.ns, &reference, ctx.clone()).await.unwrap();
        assert_eq!(*value.bytes, *secret(ctx.purpose.kind()).bytes);
        let row = f.row(&reference).await;
        assert_ne!(row.ciphertext, *value.bytes);
        assert_eq!(
            f.store
                .read(&f.ns, &reference, CredentialContext::proxy("wrong"))
                .await
                .unwrap_err(),
            WorkspaceError::CredentialCorrupt
        );
        let mut swapped = row.clone();
        swapped.context = CredentialContext::proxy("wrong");
        f.replace(swapped).await;
        assert_eq!(
            f.store
                .read(&f.ns, &reference, CredentialContext::proxy("wrong"))
                .await
                .unwrap_err(),
            WorkspaceError::CredentialCorrupt
        );
        f.replace(row).await;
    }
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), 1);
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 1);
    f.save("another", context()).await;
    assert_ne!(
        f.row("reference-0").await.nonce,
        f.row("another").await.nonce
    );
    let file = std::fs::read(f._directory.path().join("app.sqlite3")).unwrap();
    assert!(
        !file
            .windows(secret(AuthKind::Password).bytes.len())
            .any(|w| w == *secret(AuthKind::Password).bytes)
    );
}
#[tokio::test]
async fn tampering_is_per_record_and_wrong_key_never_replaces_the_key() {
    let mut f = Fixture::new();
    f.save("one", context()).await;
    f.save("two", context()).await;
    let original = f.row("one").await;
    for nonce in [false, true] {
        let mut row = original.clone();
        if nonce {
            row.nonce[0] ^= 1;
        } else {
            row.ciphertext[0] ^= 1;
        }
        f.replace(row).await;
        assert_eq!(
            f.store.read(&f.ns, "one", context()).await.unwrap_err(),
            WorkspaceError::CredentialCorrupt
        );
        f.store.read(&f.ns, "two", context()).await.unwrap();
    }
    let mut row = original.clone();
    row.format_version = 2;
    f.replace(row).await;
    assert_eq!(
        f.store.read(&f.ns, "one", context()).await.unwrap_err(),
        WorkspaceError::CredentialFormatUnsupported
    );
    f.replace(original).await;
    f.restart();
    f.keys
        .values
        .lock()
        .unwrap()
        .insert(f.ns.clone(), vec![9; 32].into());
    assert_eq!(
        initialize(&f.store, false).await.unwrap_err(),
        WorkspaceError::ConfigurationCorrupt
    );
    assert_eq!(
        f.store.read(&f.ns, "one", context()).await.unwrap_err(),
        WorkspaceError::ConfigurationCorrupt
    );
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn concurrent_business_calls_fail_fast_while_single_startup_unlock_continues() {
    let mut f = Fixture::new();
    f.save("saved", context()).await;
    f.restart();
    f.keys.block();
    f.store.initialize(false);
    let mut tasks = Vec::new();
    for i in 0..20 {
        let store = f.store.clone();
        let ns = f.ns.clone();
        tasks.push(tokio::spawn(async move {
            if i % 2 == 0 {
                store.read(&ns, "saved", context()).await.map(|_| ())
            } else {
                store
                    .write(
                        &ns,
                        &format!("new-{i}"),
                        &secret(AuthKind::Password),
                        context(),
                    )
                    .await
            }
        }));
    }
    wait_read(&f.keys, 2).await;
    tasks.remove(0).abort();
    for task in tasks {
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            WorkspaceError::ConfigurationLocked
        );
    }
    f.keys.unblock();
    initialize(&f.store, false).await.unwrap();
    f.store.read(&f.ns, "saved", context()).await.unwrap();
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 2);
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn failures_latch_until_explicit_retry_and_missing_keys_are_not_recreated() {
    let mut f = Fixture::new();
    f.save("one", context()).await;
    f.restart();
    *f.keys.error.lock().unwrap() = Some(WorkspaceError::CredentialDenied);
    assert_eq!(
        initialize(&f.store, false).await.unwrap_err(),
        WorkspaceError::CredentialDenied
    );
    for _ in 0..4 {
        assert_eq!(
            f.store.read(&f.ns, "one", context()).await.unwrap_err(),
            WorkspaceError::CredentialDenied
        );
    }
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 2);
    *f.keys.error.lock().unwrap() = None;
    initialize(&f.store, true).await.unwrap();
    f.store.read(&f.ns, "one", context()).await.unwrap();
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 3);
    f.restart();
    f.keys.values.lock().unwrap().clear();
    assert_eq!(
        initialize(&f.store, false).await.unwrap_err(),
        WorkspaceError::VaultKeyMissing
    );
    assert_eq!(
        f.store.read(&f.ns, "one", context()).await.unwrap_err(),
        WorkspaceError::VaultKeyMissing
    );
    assert_eq!(
        f.store
            .write(&f.ns, "new", &secret(AuthKind::Password), context())
            .await
            .unwrap_err(),
        WorkspaceError::VaultKeyMissing
    );
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), 1);
    f.keys
        .values
        .lock()
        .unwrap()
        .insert(f.ns.clone(), vec![0; 31].into());
    assert_eq!(
        initialize(&f.store, true).await.unwrap_err(),
        WorkspaceError::VaultKeyInvalid
    );
    assert_eq!(
        f.store.read(&f.ns, "one", context()).await.unwrap_err(),
        WorkspaceError::VaultKeyInvalid
    );
}
#[tokio::test]
async fn interrupted_initialization_reuses_created_key_and_deletion_never_unlocks() {
    let mut f = Fixture::new();
    f.db.fail_initialize.store(true, Ordering::SeqCst);
    assert_eq!(
        initialize(&f.store, false).await.unwrap_err(),
        WorkspaceError::StorageUnavailable
    );
    assert_eq!(
        f.store
            .write(&f.ns, "one", &secret(AuthKind::Password), context())
            .await
            .unwrap_err(),
        WorkspaceError::StorageUnavailable
    );
    let key = f.keys.values.lock().unwrap().get(&f.ns).unwrap().to_vec();
    f.restart();
    f.save("one", context()).await;
    assert_eq!(
        f.keys.values.lock().unwrap().get(&f.ns).unwrap().as_slice(),
        key.as_slice()
    );
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), 1);
    let reads = f.keys.reads.load(Ordering::SeqCst);
    f.restart();
    f.store.remove(&f.ns, "one").await.unwrap();
    f.store.remove(&f.ns, "one").await.unwrap();
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), reads);
    assert_eq!(
        f.store.read(&f.ns, "absent", context()).await.unwrap_err(),
        WorkspaceError::CredentialMissing
    );
    assert_eq!(f.keys.values.lock().unwrap().len(), 1);
    f.keys.values.lock().unwrap().clear();
    assert_eq!(
        initialize(&f.store, false).await.unwrap_err(),
        WorkspaceError::VaultKeyMissing
    );
    assert_eq!(
        f.store
            .write(&f.ns, "new", &secret(AuthKind::Password), context())
            .await
            .unwrap_err(),
        WorkspaceError::VaultKeyMissing
    );
}
#[tokio::test]
async fn closed_store_rejects_late_unlock_and_cannot_be_retried() {
    let mut f = Fixture::new();
    f.save("one", context()).await;
    f.restart();
    f.keys.block();
    f.store.initialize(false);
    wait_read(&f.keys, 2).await;
    f.store.close();
    f.keys.unblock();
    f.store.initialize(true);
    assert_eq!(
        f.store.read(&f.ns, "one", context()).await.unwrap_err(),
        WorkspaceError::Cancelled
    );
    assert!(matches!(
        *f.store.inner.state.lock().unwrap(),
        Unlock::Closed
    ));
}
#[tokio::test]
async fn separate_vaults_use_distinct_keys_and_cannot_swap_records() {
    let a = Fixture::new();
    let mut b = Fixture::new();
    b.keys = a.keys.clone();
    b.store = Arc::new(EncryptedSecretStore::new(b.db.clone(), b.keys.clone()));
    a.save("same", context()).await;
    b.save("same", context()).await;
    assert_ne!(a.ns, b.ns);
    b.replace(a.row("same").await).await;
    assert_eq!(
        b.store.read(&b.ns, "same", context()).await.unwrap_err(),
        WorkspaceError::CredentialCorrupt
    );
    a.store.read(&a.ns, "same", context()).await.unwrap();
    assert_eq!(a.keys.values.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn configuration_cipher_binds_identity_namespace_and_runtime_and_uses_fresh_nonces() {
    use vigilshell_application_contracts::ports::ConnectionConfigCipher;
    let f = Fixture::new();
    let cipher: Arc<dyn ConnectionConfigCipher> = f.store.unlock_configuration().await.unwrap();
    let id = new_id();
    let body = b"synthetic configuration body";
    let one = cipher.seal(&id, body).unwrap();
    let two = cipher.seal(&id, body).unwrap();
    assert_ne!(&one[8..32], &two[8..32]);
    assert_eq!(cipher.open(&id, &one).unwrap().as_slice(), body);
    assert!(cipher.open(&new_id(), &one).is_err());
    let mut altered = one.clone();
    *altered.last_mut().unwrap() ^= 1;
    assert!(cipher.open(&id, &altered).is_err());
    let other = Fixture::new();
    // Same key in another vault still cannot authenticate the configuration.
    other.keys.values.lock().unwrap().insert(
        other.ns.clone(),
        f.keys.values.lock().unwrap()[&f.ns].to_vec().into(),
    );
    let other_cipher = other.store.unlock_configuration().await.unwrap();
    assert!(other_cipher.open(&id, &one).is_err());
    f.store.close();
    assert!(cipher.open(&id, &one).is_err());
    assert!(cipher.seal(&id, body).is_err());
}

#[tokio::test]
async fn configuration_denial_is_shared_and_missing_key_is_not_replaced() {
    let f = Fixture::new();
    *f.keys.error.lock().unwrap() = Some(WorkspaceError::CredentialDenied);
    for _ in 0..3 {
        assert!(matches!(
            f.store.unlock_configuration().await,
            Err(WorkspaceError::CredentialDenied)
        ));
    }
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 1);
    *f.keys.error.lock().unwrap() = None;
    initialize(&f.store, true).await.unwrap();
    f.store.unlock_configuration().await.unwrap();
    let creates = f.keys.creates.load(Ordering::SeqCst);
    f.store.close();
    f.keys.values.lock().unwrap().clear();
    let restarted = EncryptedSecretStore::new(f.db.clone(), f.keys.clone());
    assert!(matches!(
        restarted.unlock_configuration().await,
        Err(WorkspaceError::VaultKeyMissing)
    ));
    assert_eq!(f.keys.creates.load(Ordering::SeqCst), creates);
}

#[tokio::test]
async fn input_diagnostic_survives_write_read_and_copy_but_is_not_persisted() {
    let mut f = Fixture::new();
    let mut input = secret(AuthKind::Password);
    input.diagnostic = Some(crate::diagnostics::test_trace(&input.bytes));
    let original_id = input.diagnostic.as_ref().unwrap().id().to_owned();
    initialize(&f.store, false).await.unwrap();
    f.store
        .write(&f.ns, "diagnostic", &input, context())
        .await
        .unwrap();
    let read = f.store.read(&f.ns, "diagnostic", context()).await.unwrap();
    let mut copy = read.duplicate();
    assert!(read.bytes == input.bytes);
    assert_eq!(copy.diagnostic.as_ref().unwrap().id(), original_id);
    assert!(
        copy.diagnostic
            .as_ref()
            .unwrap()
            .check("fixture_read", &copy.bytes)
    );
    copy.bytes[0] ^= 1;
    assert!(
        !copy
            .diagnostic
            .as_ref()
            .unwrap()
            .check("fixture_changed", &copy.bytes)
    );
    f.restart();
    initialize(&f.store, false).await.unwrap();
    let reopened = f.store.read(&f.ns, "diagnostic", context()).await.unwrap();
    assert!(reopened.bytes == input.bytes);
    assert!(reopened.diagnostic.is_none());
    let database = std::fs::read(f._directory.path().join("app.sqlite3")).unwrap();
    assert!(
        !database
            .windows(original_id.len())
            .any(|bytes| bytes == original_id.as_bytes())
    );
}

#[tokio::test]
async fn pending_authorization_is_single_flight_and_does_not_queue_business_writes() {
    let f = Fixture::new();
    f.keys.block();
    let state = f.store.initialize(false);
    assert_eq!(state.phase, ConnectionAccessPhase::Unlocking);
    wait_read(&f.keys, 1).await;
    for _ in 0..10 {
        assert_eq!(f.store.initialize(false), state);
        assert_eq!(f.store.initialize(true), state);
    }
    assert_eq!(
        f.store
            .write(&f.ns, "not-written", &secret(AuthKind::Password), context())
            .await
            .unwrap_err(),
        WorkspaceError::ConfigurationLocked
    );
    assert!(matches!(
        f.db.execute(DbCommand::Preferences).await,
        Ok(DbReply::Preferences(_))
    ));
    assert!(matches!(
        f.db.execute(DbCommand::ReadEncryptedCredential("not-written".into()))
            .await,
        Err(WorkspaceError::CredentialMissing)
    ));
    f.keys.unblock();
    initialize(&f.store, false).await.unwrap();
    assert!(matches!(
        f.db.execute(DbCommand::ReadEncryptedCredential("not-written".into()))
            .await,
        Err(WorkspaceError::CredentialMissing)
    ));
    assert_eq!(f.keys.reads.load(Ordering::SeqCst), 1);
}
