use super::*;
use vigilshell_credentials::{VaultKeyMaterial, VaultKeyStore};
use vigilshell_storage_sqlite::SqlitePreferences;
struct Database(
    Mutex<SqlitePreferences>,
    Mutex<Option<std::sync::Weak<vigilshell_credentials::EncryptedSecretStore>>>,
);
#[derive(Debug)]
struct ConfigurationFixture;
impl ConnectionConfigCipher for ConfigurationFixture {
    fn seal(&self, id: &str, bytes: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
        let mut result = vec![0; 48];
        result.extend_from_slice(id.as_bytes());
        result.push(0);
        result.extend_from_slice(bytes);
        Ok(result)
    }
    fn open(&self, id: &str, bytes: &[u8]) -> Result<VaultKeyMaterial, WorkspaceError> {
        let prefix = self.seal(id, &[])?;
        Ok(bytes
            .strip_prefix(prefix.as_slice())
            .ok_or(WorkspaceError::ConfigurationCorrupt)?
            .to_vec()
            .into())
    }
}
impl WorkspaceDatabase for Database {
    fn execute(&self, c: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if !c.needs_connection_configuration() {
                return self.0.lock().unwrap().execute(c);
            }
            let secrets = self
                .1
                .lock()
                .unwrap()
                .as_ref()
                .and_then(std::sync::Weak::upgrade);
            let cipher: Arc<dyn ConnectionConfigCipher> = if let Some(secrets) = secrets {
                let mut states = secrets.subscribe_access();
                secrets.initialize(false);
                while secrets.access_state().phase == ConnectionAccessPhase::Unlocking {
                    states
                        .changed()
                        .await
                        .map_err(|_| WorkspaceError::Cancelled)?;
                }
                secrets.configuration_cipher()?
            } else {
                let mut db = self.0.lock().unwrap();
                let DbReply::VaultMetadata(meta) = db.execute(DbCommand::VaultMetadata)? else {
                    panic!()
                };
                db.execute(DbCommand::InitializeVault {
                    namespace: meta.namespace,
                    key_version: meta.key_version,
                })?;
                Arc::new(ConfigurationFixture)
            };
            self.0
                .lock()
                .unwrap()
                .execute(DbCommand::WithConfigurationCipher {
                    cipher,
                    command: Box::new(c),
                })
        })
    }
}
#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<String, SecretMaterial>>,
    deny_remove: AtomicBool,
    deny_read: AtomicBool,
    reads: std::sync::atomic::AtomicUsize,
}
impl SecretStore for Secrets {
    fn write<'a>(
        &'a self,
        n: &'a str,
        r: &'a str,
        s: &'a SecretMaterial,
        _: CredentialContext,
    ) -> FutureResult<'a, ()> {
        Box::pin(async move {
            self.values
                .lock()
                .unwrap()
                .insert(format!("{n}/{r}"), s.duplicate());
            Ok(())
        })
    }
    fn read<'a>(
        &'a self,
        n: &'a str,
        r: &'a str,
        _: CredentialContext,
    ) -> FutureResult<'a, SecretMaterial> {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.deny_read.load(Ordering::SeqCst) {
                return Err(WorkspaceError::CredentialDenied);
            }
            self.values
                .lock()
                .unwrap()
                .get(&format!("{n}/{r}"))
                .map(SecretMaterial::duplicate)
                .ok_or(WorkspaceError::CredentialMissing)
        })
    }
    fn remove<'a>(&'a self, n: &'a str, r: &'a str) -> FutureResult<'a, ()> {
        Box::pin(async move {
            if self.deny_remove.load(Ordering::SeqCst) {
                return Err(WorkspaceError::CredentialDenied);
            }
            self.values.lock().unwrap().remove(&format!("{n}/{r}"));
            Ok(())
        })
    }
}

struct Prompt;

#[tokio::test]
async fn encrypted_store_integrates_saved_login_proxy_and_host_restart() {
    #[derive(Default)]
    struct Keys {
        value: Mutex<Option<VaultKeyMaterial>>,
        reads: std::sync::atomic::AtomicUsize,
    }
    impl VaultKeyStore for Keys {
        fn read(&self, _: &str, _: u32) -> Result<VaultKeyMaterial, WorkspaceError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.value
                .lock()
                .unwrap()
                .as_ref()
                .map(|v| v.to_vec().into())
                .ok_or(WorkspaceError::VaultKeyMissing)
        }
        fn create(&self, _: &str, _: u32, key: &[u8]) -> Result<bool, WorkspaceError> {
            let mut value = self.value.lock().unwrap();
            if value.is_some() {
                return Ok(false);
            }
            *value = Some(key.to_vec().into());
            Ok(true)
        }
    }
    #[derive(Default)]
    struct Diagnostic(Mutex<Vec<(&'static str, Option<bool>)>>);
    impl SecretDiagnostic for Diagnostic {
        fn id(&self) -> &str {
            "synthetic-trace"
        }
        fn check(&self, stage: &'static str, bytes: &[u8]) -> bool {
            let matches = bytes == b"synthetic-only";
            self.record(stage, Some(matches));
            matches
        }
        fn record(&self, stage: &'static str, matches: Option<bool>) {
            self.0.lock().unwrap().push((stage, matches));
        }
    }
    struct DiagnosticPrompt(Arc<Diagnostic>);
    impl NativePrompt for DiagnosticPrompt {
        fn capture(
            &self,
            kind: AuthKind,
            cancelled: Arc<AtomicBool>,
        ) -> FutureResult<'_, SecretMaterial> {
            Box::pin(async move {
                let mut secret = Prompt.capture(kind, cancelled).await?;
                secret.diagnostic = Some(self.0.clone());
                Ok(secret)
            })
        }
        fn update_context(&self, _: DisplayContext) {}
    }
    let diagnostic = Arc::new(Diagnostic::default());
    let mut f = Fixture::new();
    let db = f.database.upgrade().unwrap();
    let keys = Arc::new(Keys::default());
    f.service.shutdown().await.unwrap();
    let service = || {
        let secrets = Arc::new(vigilshell_credentials::EncryptedSecretStore::new(
            db.clone(),
            keys.clone(),
        ));
        *db.1.lock().unwrap() = Some(Arc::downgrade(&secrets));
        WorkspaceService::new(
            db.clone(),
            secrets,
            Arc::new(DiagnosticPrompt(diagnostic.clone())),
            Arc::new(Transport),
            Arc::new(Sink),
        )
    };
    f.service = service();
    db.execute(DbCommand::ConfigurationReady).await.unwrap();
    let login = f.create().await;
    for checkpoint in [
        "vault_write_input",
        "vault_write_readback",
        "vault_read",
        "connection_config_readback",
        "connection_credential_reference",
    ] {
        assert!(
            diagnostic
                .0
                .lock()
                .unwrap()
                .contains(&(checkpoint, Some(true))),
            "missing checkpoint: {checkpoint}"
        );
    }
    assert!(
        !diagnostic
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(_, value)| *value == Some(false))
    );
    let WorkspaceResult::Draft(d) = f.run(WorkspaceAction::BeginProxyDraft(None)).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(
        f.done(
            f.run(WorkspaceAction::ConfigureDraft {
                draft: d.id.clone(),
                kind: AuthKind::Password
            })
            .await
            .unwrap()
        )
        .await
        .phase,
        "success"
    );
    let proxy = f
        .done(
            f.run(WorkspaceAction::SaveProxy(ProxySave {
                proxy: ProxyRecord {
                    id: new_id(),
                    name: "encrypted proxy".into(),
                    kind: "http".into(),
                    host: "127.0.0.1".into(),
                    port: 8080,
                    username: "user".into(),
                    credential: None,
                    version: 0,
                },
                expected_version: None,
                draft_id: Some(d.id),
                clear_password: false,
            }))
            .await
            .unwrap(),
        )
        .await
        .proxy
        .unwrap();
    let ns = f.service.namespace().await.unwrap();
    let DbReply::EncryptedCredential(record) = db
        .execute(DbCommand::ReadEncryptedCredential(
            login.credential.clone().unwrap(),
        ))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        record.context,
        CredentialContext::login(&login.id, AuthKind::Password)
    );
    assert_ne!(record.ciphertext, b"synthetic-only");
    assert_eq!(
        &*f.service
            .group_secret(
                &f.service
                    .runtime_group(&f.owner, login.clone())
                    .await
                    .unwrap()
            )
            .await
            .unwrap()
            .bytes,
        b"synthetic-only"
    );
    assert_eq!(
        &*f.service
            .route(Some(&proxy))
            .await
            .unwrap()
            .unwrap()
            .secret
            .unwrap()
            .bytes,
        b"synthetic-only"
    );
    assert_eq!(keys.reads.load(Ordering::SeqCst), 1);
    f.service.shutdown().await.unwrap();
    f.service = service();
    db.execute(DbCommand::ConfigurationReady).await.unwrap();
    assert_eq!(f.service.namespace().await.unwrap(), ns);
    assert_eq!(
        &*f.service
            .group_secret(
                &f.service
                    .runtime_group(&f.owner, login.clone())
                    .await
                    .unwrap()
            )
            .await
            .unwrap()
            .bytes,
        b"synthetic-only"
    );
    f.service.route(Some(&proxy)).await.unwrap();
    let group = f.service.runtime_group(&f.owner, login).await.unwrap();
    f.service.group_secret(&group).await.unwrap();
    assert_eq!(keys.reads.load(Ordering::SeqCst), 2);
    f.service.shutdown().await.unwrap();
}

impl NativePrompt for Prompt {
    fn capture(&self, kind: AuthKind, _: Arc<AtomicBool>) -> FutureResult<'_, SecretMaterial> {
        Box::pin(async move {
            Ok(SecretMaterial {
                diagnostic: None,
                kind,
                bytes: b"synthetic-only".to_vec().into(),
                label: "fixture".into(),
            })
        })
    }
    fn update_context(&self, _: DisplayContext) {}
}
struct Transport;
impl SshTransport for Transport {
    fn probe(&self, _: ConnectionInput) -> FutureResult<'_, HostKey> {
        Box::pin(async {
            Ok(HostKey {
                public_key: "synthetic-public-key".into(),
                fingerprint: "SHA256:synthetic".into(),
                algorithm: "ssh-ed25519".into(),
            })
        })
    }
    fn authenticate(
        &self,
        _: ConnectionInput,
        _: HostKey,
        _: SecretMaterial,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        Box::pin(async { Ok(Box::new(SessionFixture) as Box<dyn SshSession>) })
    }
}
struct SessionFixture;
impl SshSession for SessionFixture {
    fn open_terminal(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn read(&mut self) -> FutureResult<'_, TransportEvent> {
        Box::pin(std::future::pending())
    }
    fn write<'a>(&'a mut self, _: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn resize(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn close(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Sink;
impl WorkspaceSink for Sink {
    fn event(&self, _: String, _: WorkspaceNotice) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn output(&self, _: String, _: String, _: u64, _: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn input_ack(&self, _: String, _: String, _: u64, _: u32) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn end(&self, _: String, _: String, _: u64) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Fixture {
    database: std::sync::Weak<Database>,
    service: Arc<WorkspaceService>,
    secrets: Arc<Secrets>,
    owner: String,
    path: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("vigilshell-workspace-test-{}", new_id()));
        let secrets = Arc::new(Secrets::default());
        let database = Arc::new(Database(
            Mutex::new(SqlitePreferences::open(&path).unwrap()),
            Mutex::new(None),
        ));
        let service = WorkspaceService::new(
            database.clone(),
            secrets.clone(),
            Arc::new(Prompt),
            Arc::new(Transport),
            Arc::new(Sink),
        );
        Self {
            database: Arc::downgrade(&database),
            service,
            secrets,
            owner: new_id(),
            path,
        }
    }
    fn monitor_group(&self, session: &str) -> String {
        self.service
            .state
            .lock()
            .unwrap()
            .sessions
            .get(session)
            .unwrap()
            .view
            .group_id
            .clone()
    }
    async fn run(&self, a: WorkspaceAction) -> Result<WorkspaceResult, WorkspaceError> {
        self.service.handle(&self.owner, a).await
    }
    async fn done(&self, r: WorkspaceResult) -> OperationView {
        let WorkspaceResult::Operation(o) = r else {
            panic!("operation expected")
        };
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let o = self.service.operation(&self.owner, &o.id).unwrap();
                if !["working", "trust"].contains(&o.phase.as_str()) {
                    return o;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn draft(&self, c: Option<String>) -> String {
        let WorkspaceResult::Draft(d) = self.run(WorkspaceAction::BeginDraft(c)).await.unwrap()
        else {
            panic!()
        };
        let o = self
            .run(WorkspaceAction::ConfigureDraft {
                draft: d.id.clone(),
                kind: AuthKind::Password,
            })
            .await
            .unwrap();
        assert_eq!(self.done(o).await.phase, "success");
        d.id
    }
    async fn create(&self) -> ConnectionRecord {
        let d = self.draft(None).await;
        let o = self
            .run(WorkspaceAction::Save(request(None, Some(d))))
            .await
            .unwrap();
        let o = self.done(o).await;
        assert_eq!(o.phase, "success");
        o.connection.unwrap()
    }
    async fn reopen(&mut self) {
        // Keep the final database drop on this thread so its file lock is released
        // before reopening; a zero Arc count alone does not await destruction.
        let closing_database = self.database.upgrade().unwrap();
        self.service.shutdown().await.unwrap();
        let previous_service = Arc::downgrade(&self.service);
        let previous_database = Arc::downgrade(&self.service.database);
        self.service = WorkspaceService::new(
            Arc::new(UnavailableDatabase),
            self.secrets.clone(),
            Arc::new(Prompt),
            Arc::new(Transport),
            Arc::new(Sink),
        );
        assert_eq!(
            previous_service.strong_count(),
            0,
            "previous service still retained"
        );
        assert_eq!(
            previous_database.strong_count(),
            1,
            "previous database still retained outside the fixture"
        );
        drop(closing_database);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.path.join("runtime.lock"))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if lock.try_lock().is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("previous database file lock was not released");
        lock.unlock().unwrap();
        drop(lock);
        let database = Arc::new(Database(
            Mutex::new(SqlitePreferences::open(&self.path).unwrap()),
            Mutex::new(None),
        ));
        self.database = Arc::downgrade(&database);
        self.service = WorkspaceService::new(
            database,
            self.secrets.clone(),
            Arc::new(Prompt),
            Arc::new(Transport),
            Arc::new(Sink),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn input() -> ConnectionInput {
    ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: Vec::new(),
        environment: Environment::None,
        name: "Fixture".into(),
        host: "127.0.0.1".into(),
        port: 22,
        username: "fixture".into(),
        auth: AuthKind::Password,
    }
}
fn request(p: Option<&ConnectionRecord>, draft: Option<String>) -> SaveRequest {
    SaveRequest {
        connection_id: p.map(|p| p.id.clone()),
        expected_version: p.map(|p| p.version),
        group_id: p.and_then(|p| p.group_id.clone()),
        input: input(),
        draft_id: draft,
    }
}
#[tokio::test]
async fn draft_cancel_does_not_save_or_remove_the_existing_secret() {
    let f = Fixture::new();
    let p = f.create().await;
    let d = f.draft(Some(p.id.clone())).await;
    assert!(matches!(
        f.service
            .handle(&new_id(), WorkspaceAction::DiscardDraft(d.clone()))
            .await,
        Err(WorkspaceError::NotFound)
    ));
    f.run(WorkspaceAction::DiscardDraft(d.clone()))
        .await
        .unwrap();
    f.run(WorkspaceAction::DiscardDraft(d)).await.unwrap();
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    assert_eq!(
        f.service.connection(&p.id).await.unwrap().version,
        p.version
    );
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn replacement_cleanup_failure_preserves_new_binding_and_recovers() {
    let f = Fixture::new();
    let p = f.create().await;
    let d = f.draft(Some(p.id.clone())).await;
    f.secrets.deny_remove.store(true, Ordering::SeqCst);
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(Some(&p), Some(d))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "success");
    let current = f.service.connection(&p.id).await.unwrap();
    assert_ne!(current.credential, p.credential);
    assert!(current.credential_pending);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 2);
    f.secrets.deny_remove.store(false, Ordering::SeqCst);
    f.run(WorkspaceAction::ReconcileCredentials(p.id.clone()))
        .await
        .unwrap();
    assert!(
        !f.service
            .connection(&p.id)
            .await
            .unwrap()
            .credential_pending
    );
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn failed_delete_stays_visible_and_retry_finishes() {
    let f = Fixture::new();
    let p = f.create().await;
    f.secrets.deny_remove.store(true, Ordering::SeqCst);
    let r = f
        .run(WorkspaceAction::Delete {
            id: p.id.clone(),
            version: p.version,
            disconnect: true,
            sessions: vec![],
        })
        .await
        .unwrap();
    assert_eq!(
        f.done(r).await.error,
        Some(WorkspaceError::CredentialDenied)
    );
    assert!(f.service.connection(&p.id).await.unwrap().deleting);
    let DbReply::Jobs(jobs) = f
        .service
        .database
        .execute(DbCommand::CredentialJobs)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].action, "delete");
    assert_eq!(jobs[0].old_ref, p.credential);
    f.secrets.deny_remove.store(false, Ordering::SeqCst);
    f.run(WorkspaceAction::ReconcileCredentials(p.id.clone()))
        .await
        .unwrap();
    assert!(matches!(
        f.service.connection(&p.id).await,
        Err(WorkspaceError::NotFound)
    ));
    assert!(f.secrets.values.lock().unwrap().is_empty());
    assert!(
        matches!(f.service.database.execute(DbCommand::CredentialJobs).await.unwrap(), DbReply::Jobs(jobs) if jobs.is_empty())
    );
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn historical_clear_jobs_recover_after_reopen_without_removing_current_bindings() {
    for binding in ["original", "removed", "replacement"] {
        let mut f = Fixture::new();
        let p = f.create().await;
        let namespace = f.service.namespace().await.unwrap();
        let old = p.credential.clone().unwrap();
        let current = match binding {
            "original" => Some(old.clone()),
            "removed" => None,
            _ => {
                let reference = new_id();
                f.secrets
                    .write(
                        &namespace,
                        &reference,
                        &SecretMaterial {
                            diagnostic: None,
                            kind: AuthKind::Password,
                            bytes: b"replacement-only".to_vec().into(),
                            label: "replacement".into(),
                        },
                        CredentialContext::login(&p.id, AuthKind::Password),
                    )
                    .await
                    .unwrap();
                Some(reference)
            }
        };
        if binding != "original" {
            f.service
                .database
                .execute(DbCommand::Save {
                    id: p.id.clone(),
                    expected_version: Some(p.version),
                    group_id: p.group_id.clone(),
                    input: p.input.clone(),
                    credential: current
                        .as_ref()
                        .map_or(CredentialChange::Remove, |reference| {
                            CredentialChange::Replace {
                                reference: reference.clone(),
                                label: "replacement".into(),
                            }
                        }),
                    trust: None,
                })
                .await
                .unwrap();
        }
        f.service
            .database
            .execute(DbCommand::PrepareCredential(CredentialJob {
                id: new_id(),
                connection_id: p.id.clone(),
                new_ref: None,
                old_ref: Some(old.clone()),
                action: "clear".into(),
            }))
            .await
            .unwrap();
        f.reopen().await;
        let before = f.service.connection(&p.id).await.unwrap();
        f.secrets.deny_remove.store(true, Ordering::SeqCst);
        let result = f.service.recover_credentials_for(Some(&p.id)).await;
        if binding == "original" {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err(), WorkspaceError::CredentialDenied);
            assert!(
                f.secrets
                    .values
                    .lock()
                    .unwrap()
                    .contains_key(&format!("{namespace}/{old}"))
            );
            assert!(
                matches!(f.service.database.execute(DbCommand::CredentialJobs).await.unwrap(), DbReply::Jobs(jobs) if jobs.len() == 1 && jobs[0].action == "clear")
            );
            f.secrets.deny_remove.store(false, Ordering::SeqCst);
            f.service
                .recover_credentials_for(Some(&p.id))
                .await
                .unwrap();
        }
        f.service.recover_credentials().await.unwrap();
        let after = f.service.connection(&p.id).await.unwrap();
        assert_eq!(after.credential, current);
        assert_eq!(after.version, before.version);
        assert_eq!(after.revision, before.revision);
        assert!(!after.deleting && !after.credential_pending);
        assert!(
            matches!(f.service.database.execute(DbCommand::CredentialJobs).await.unwrap(), DbReply::Jobs(jobs) if jobs.is_empty())
        );
        {
            let values = f.secrets.values.lock().unwrap();
            assert_eq!(values.len(), usize::from(current.is_some()));
            if let Some(reference) = current {
                assert!(values.contains_key(&format!("{namespace}/{reference}")));
            }
        }
        assert_eq!(f.secrets.reads.load(Ordering::SeqCst), 0);
        f.service.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn historical_clear_job_waits_for_an_existing_runtime_credential_lease() {
    let f = Fixture::new();
    let p = f.create().await;
    let group = f.service.runtime_group(&f.owner, p.clone()).await.unwrap();
    f.service
        .database
        .execute(DbCommand::PrepareCredential(CredentialJob {
            id: new_id(),
            connection_id: p.id.clone(),
            new_ref: None,
            old_ref: p.credential.clone(),
            action: "clear".into(),
        }))
        .await
        .unwrap();
    f.service
        .database
        .execute(DbCommand::Save {
            id: p.id.clone(),
            expected_version: Some(p.version),
            group_id: p.group_id,
            input: p.input,
            credential: CredentialChange::Remove,
            trust: None,
        })
        .await
        .unwrap();
    f.service.recover_credentials().await.unwrap();
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    assert!(
        f.service
            .connection(&p.id)
            .await
            .unwrap()
            .credential_pending
    );
    assert!(
        matches!(f.service.database.execute(DbCommand::CredentialJobs).await.unwrap(), DbReply::Jobs(jobs) if jobs.len() == 1)
    );
    f.service.release_group(&f.owner, &group).await;
    assert!(f.secrets.values.lock().unwrap().is_empty());
    assert!(
        !f.service
            .connection(&p.id)
            .await
            .unwrap()
            .credential_pending
    );
    assert!(
        matches!(f.service.database.execute(DbCommand::CredentialJobs).await.unwrap(), DbReply::Jobs(jobs) if jobs.is_empty())
    );
    assert_eq!(f.secrets.reads.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn draft_test_cancel_invalidates_challenge_and_releases_draft() {
    let f = Fixture::new();
    let d = f.draft(None).await;
    let r = f
        .run(WorkspaceAction::Test(TestRequest {
            connection_id: None,
            input: Some(input()),
            draft_id: Some(d.clone()),
        }))
        .await
        .unwrap();
    let WorkspaceResult::Operation(o) = r else {
        panic!()
    };
    let challenge = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(c) = f.service.operation(&f.owner, &o.id).unwrap().challenge {
                return c.id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        f.run(WorkspaceAction::DiscardDraft(d.clone())).await,
        Err(WorkspaceError::Busy)
    ));
    f.run(WorkspaceAction::Cancel(o.id.clone())).await.unwrap();
    assert_eq!(
        f.done(WorkspaceResult::Operation(o)).await.phase,
        "cancelled"
    );
    assert!(matches!(
        f.run(WorkspaceAction::Confirm {
            challenge,
            accepted: true
        })
        .await,
        Err(WorkspaceError::TrustExpired)
    ));
    f.run(WorkspaceAction::DiscardDraft(d)).await.unwrap();
    assert!(f.secrets.values.lock().unwrap().is_empty());
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn save_limit_releases_reservations() {
    let f = Fixture::new();
    let d = f.draft(None).await;
    for _ in 0..64 {
        f.service.begin(&f.owner, "fixture", true).unwrap();
    }
    assert!(matches!(
        f.run(WorkspaceAction::Save(request(None, Some(d.clone()))))
            .await,
        Err(WorkspaceError::Busy)
    ));
    assert!(f.service.state.lock().unwrap().changing.is_empty());
    assert!(!f.service.state.lock().unwrap().drafts[&d].busy);
    f.run(WorkspaceAction::DiscardDraft(d)).await.unwrap();
    f.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn trust_confirmation_precedes_authentication_and_sessions_keep_target() {
    let f = Fixture::new();
    let p = f.create().await;
    let WorkspaceResult::Operation(o) = f
        .run(WorkspaceAction::Open {
            id: p.id.clone(),
            columns: 80,
            rows: 24,
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let challenge = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(c) = f.service.operation(&f.owner, &o.id).unwrap().challenge {
                return c.id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    f.run(WorkspaceAction::Confirm {
        challenge,
        accepted: true,
    })
    .await
    .unwrap();
    let done = f.done(WorkspaceResult::Operation(o)).await;
    assert_eq!(done.phase, "success");
    let mut edit = request(Some(&p), None);
    edit.input.host = "new.example.test".into();
    let r = f.run(WorkspaceAction::Save(edit)).await.unwrap();
    assert_eq!(f.done(r).await.phase, "success");
    let sid = done.session_id.unwrap();
    assert_eq!(
        f.service.state.lock().unwrap().sessions[&sid]
            .view
            .connection
            .input
            .host,
        "127.0.0.1"
    );
    f.run(WorkspaceAction::Close(sid)).await.unwrap();
    f.service.shutdown().await.unwrap();
}

struct Flood;
impl SshSession for Flood {
    fn open_terminal(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn read(&mut self) -> FutureResult<'_, TransportEvent> {
        Box::pin(async { Ok(TransportEvent::Data(vec![b'x'; 32768])) })
    }
    fn write<'a>(&'a mut self, _: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(std::future::pending())
    }
    fn resize(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn close(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct CountSink(Arc<std::sync::atomic::AtomicUsize>);
impl WorkspaceSink for CountSink {
    fn event(&self, _: String, _: WorkspaceNotice) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn output(&self, _: String, _: String, _: u64, data: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.0.fetch_add(data.len(), Ordering::SeqCst);
            Ok(())
        })
    }
    fn input_ack(&self, _: String, _: String, _: u64, _: u32) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn end(&self, _: String, _: String, _: u64) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
#[tokio::test]
async fn slow_consumer_is_bounded_and_cancel_interrupts_blocked_input() {
    let f = Fixture::new();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(Transport),
        Arc::new(CountSink(count.clone())),
    );
    let (tx, rx) = mpsc::channel(128);
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        service
            .run_session("owner", "session", Box::new(Flood), rx, task_cancel)
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while count.load(Ordering::SeqCst) < 256 * 1024 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(count.load(Ordering::SeqCst), 256 * 1024);
    tx.send(SessionCommand::Input {
        generation: Some(0),
        sequence: 1,
        bytes: b"blocked".to_vec(),
    })
    .await
    .unwrap();
    tokio::task::yield_now().await;
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), task)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
}

struct SshFixturePrompt;
impl NativePrompt for SshFixturePrompt {
    fn capture(&self, kind: AuthKind, _: Arc<AtomicBool>) -> FutureResult<'_, SecretMaterial> {
        Box::pin(async move {
            Ok(SecretMaterial {
                diagnostic: None,
                kind,
                bytes: b"sidebar-fixture".to_vec().into(),
                label: "isolated fixture".into(),
            })
        })
    }
    fn update_context(&self, _: DisplayContext) {}
}
struct EchoSink(Arc<Mutex<Vec<u8>>>);
impl WorkspaceSink for EchoSink {
    fn event(&self, _: String, _: WorkspaceNotice) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn output(&self, _: String, _: String, _: u64, data: Vec<u8>) -> FutureResult<'_, ()> {
        self.0.lock().unwrap().extend(data);
        Box::pin(async { Ok(()) })
    }
    fn input_ack(&self, _: String, _: String, _: u64, _: u32) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn end(&self, _: String, _: String, _: u64) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
#[tokio::test]
async fn real_ssh_session_survives_layout_removal_and_environment_save() {
    let ssh = live_fixture::Fixture::start().await;
    let port = ssh.port;
    let mut f = Fixture::new();
    let output = Arc::new(Mutex::new(Vec::new()));
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(SshFixturePrompt),
        Arc::new(vigilshell_transport_russh::RusshTransport),
        Arc::new(EchoSink(output.clone())),
    );
    let draft = f.draft(None).await;
    let mut request = request(None, Some(draft));
    request.input.port = port;
    request.input.username = "fixture".into();
    let p = f
        .done(f.run(WorkspaceAction::Save(request.clone())).await.unwrap())
        .await
        .connection
        .unwrap();
    let WorkspaceResult::Operation(open) = f
        .run(WorkspaceAction::Open {
            id: p.id.clone(),
            columns: 80,
            rows: 24,
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let challenge = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(c) = f.service.operation(&f.owner, &open.id).unwrap().challenge {
                return c.id;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    f.run(WorkspaceAction::Confirm {
        challenge,
        accepted: true,
    })
    .await
    .unwrap();
    let done = f.done(WorkspaceResult::Operation(open)).await;
    assert_eq!(done.phase, "success");
    let sid = done.session_id.unwrap();
    let trust = f.service.trust(&p.id).await.unwrap().unwrap();
    let WorkspaceResult::Database(DbReply::Group(group)) = f
        .run(WorkspaceAction::Database(DbCommand::CreateGroup(
            "live".into(),
        )))
        .await
        .unwrap()
    else {
        panic!()
    };
    let WorkspaceResult::Database(DbReply::Catalogue(page)) = f
        .run(WorkspaceAction::Database(DbCommand::List {
            query: "".into(),
            offset: 0,
            limit: 50,
            kind: "connections".into(),
            version: None,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.run(WorkspaceAction::Database(DbCommand::UpdateLayout(
        GroupLayout {
            revision: page.layout_revision,
            group_ids: vec![group.id.clone()],
            buckets: vec![
                GroupBucket {
                    group_id: Some(group.id.clone()),
                    connection_ids: vec![p.id.clone()],
                },
                GroupBucket {
                    group_id: None,
                    connection_ids: vec![],
                },
            ],
        },
    )))
    .await
    .unwrap();
    f.run(WorkspaceAction::Database(DbCommand::DeleteGroup {
        id: group.id,
        version: group.version,
    }))
    .await
    .unwrap();
    request.connection_id = Some(p.id.clone());
    request.expected_version = Some(p.version);
    request.draft_id = None;
    request.input.environment = Environment::Production;
    let saved = f
        .done(f.run(WorkspaceAction::Save(request)).await.unwrap())
        .await
        .connection
        .unwrap();
    assert_eq!(saved.revision, p.revision);
    assert!(saved.group_id.is_none());
    assert_eq!(saved.credential, p.credential);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    assert_eq!(
        f.service
            .trust(&p.id)
            .await
            .unwrap()
            .unwrap()
            .key
            .fingerprint,
        trust.key.fingerprint
    );
    assert_eq!(
        f.service.state.lock().unwrap().sessions[&sid].view.phase,
        "running"
    );
    f.service
        .input(&f.owner, &sid, 1, b"AFTER_ORGANIZATION".to_vec())
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if String::from_utf8_lossy(&output.lock().unwrap()).contains("AFTER_ORGANIZATION") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
    f.service.shutdown().await.unwrap();
}

#[path = "../../../../tests/fixtures/ssh_server.rs"]
mod live_fixture;

async fn open_confirmed(f: &Fixture, id: &str) -> OperationView {
    let r = f
        .run(WorkspaceAction::Open {
            id: id.into(),
            columns: 80,
            rows: 24,
        })
        .await
        .unwrap();
    let WorkspaceResult::Operation(op) = &r else {
        panic!()
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let o = f.service.operation(&f.owner, &op.id).unwrap();
            if let Some(c) = o.challenge {
                f.run(WorkspaceAction::Confirm {
                    challenge: c.id,
                    accepted: true,
                })
                .await
                .unwrap();
            }
            if !["working", "trust"].contains(&o.phase.as_str()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    f.done(r).await
}
#[tokio::test]
async fn runtime_group_reuses_tunnel_and_credentials_until_last_terminal_closes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let ssh = live_fixture::Fixture::start().await;
    let mut f = Fixture::new();
    let output = Arc::new(Mutex::new(Vec::new()));
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(SshFixturePrompt),
        Arc::new(vigilshell_transport_russh::RusshTransport),
        Arc::new(EchoSink(output.clone())),
    );
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = echo.local_addr().unwrap().port();
    let echo_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = echo.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = socket.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    let port = live_fixture::unused_port().await;
    let mut create = request(None, Some(f.draft(None).await));
    create.input.port = ssh.port;
    create.input.username = "fixture".into();
    create.input.terminal.encoding = "gbk".into();
    create.input.tunnels = vec![TunnelRule {
        id: new_id(),
        name: "echo".into(),
        kind: "local".into(),
        bind_host: "127.0.0.1".into(),
        listen_port: port,
        target_host: Some("127.0.0.1".into()),
        target_port: Some(target),
        enabled: true,
    }];
    let p = f
        .done(f.run(WorkspaceAction::Save(create)).await.unwrap())
        .await
        .connection
        .unwrap();
    let reads_before = f.secrets.reads.load(Ordering::SeqCst);
    let first = open_confirmed(&f, &p.id).await;
    assert_eq!(first.phase, "success");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if f.service
                .tunnel_statuses(&f.owner, &p.id)
                .iter()
                .any(|s| s.phase == "running")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    f.service
        .input(
            &f.owner,
            first.session_id.as_ref().unwrap(),
            1,
            "中文".as_bytes().to_vec(),
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if output.lock().unwrap().as_slice() == "中文".as_bytes() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let current = f.service.connection(&p.id).await.unwrap();
    let mut update = request(Some(&current), Some(f.draft(Some(p.id.clone())).await));
    update.input = current.input.clone();
    update.input.host = "new.example.test".into();
    let updated = f
        .done(f.run(WorkspaceAction::Save(update)).await.unwrap())
        .await
        .connection
        .unwrap();
    assert!(updated.credential_pending);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 2);
    let second = open_confirmed(&f, &p.id).await;
    assert_eq!(second.phase, "success");
    assert_eq!(
        f.secrets.reads.load(Ordering::SeqCst) - reads_before,
        1,
        "terminals and forwarding share the frozen secret after replacement"
    );
    assert_eq!(
        ssh.shared.logins.load(Ordering::SeqCst),
        3,
        "two terminals plus a single tunnel transport"
    );
    let WorkspaceResult::Draft(removal) = f
        .run(WorkspaceAction::BeginDraft(Some(p.id.clone())))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.run(WorkspaceAction::ClearDraft(removal.id.clone()))
        .await
        .unwrap();
    let mut clear = request(Some(&updated), Some(removal.id));
    clear.input = updated.input.clone();
    let cleared = f
        .done(f.run(WorkspaceAction::Save(clear)).await.unwrap())
        .await;
    assert_eq!(cleared.phase, "success");
    assert!(cleared.connection.unwrap().credential.is_none());
    assert_eq!(
        f.secrets.values.lock().unwrap().len(),
        1,
        "leased original secret remains until the last terminal closes"
    );
    assert!(matches!(
        f.run(WorkspaceAction::Open {
            id: p.id.clone(),
            columns: 80,
            rows: 24
        })
        .await,
        Err(WorkspaceError::CredentialMissing)
    ));
    f.run(WorkspaceAction::Close(first.session_id.unwrap()))
        .await
        .unwrap();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream.write_all(b"still alive").await.unwrap();
    let mut bytes = [0; 11];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"still alive");
    drop(stream);
    f.run(WorkspaceAction::Close(second.session_id.unwrap()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if f.service.state.lock().unwrap().groups.is_empty()
                && f.secrets.values.lock().unwrap().is_empty()
                && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        f.service.connection(&p.id).await.unwrap().input.host,
        "new.example.test"
    );
    f.service.shutdown().await.unwrap();
    echo_task.abort();
}

#[tokio::test]
async fn proxy_drafts_are_isolated_and_secret_replacement_and_clear_settle() {
    let f = Fixture::new();
    let connection_draft = f.draft(None).await;
    let WorkspaceResult::Draft(d) = f.run(WorkspaceAction::BeginProxyDraft(None)).await.unwrap()
    else {
        panic!()
    };
    assert!(d.proxy_id.is_none() && d.connection_id.is_none());
    f.run(WorkspaceAction::DiscardDraft(connection_draft))
        .await
        .unwrap();
    assert!(matches!(
        f.run(WorkspaceAction::ConfigureDraft {
            draft: d.id.clone(),
            kind: AuthKind::PrivateKey
        })
        .await,
        Err(WorkspaceError::InvalidInput)
    ));
    let capture = f
        .run(WorkspaceAction::ConfigureDraft {
            draft: d.id.clone(),
            kind: AuthKind::Password,
        })
        .await
        .unwrap();
    assert!(f.done(capture).await.draft.unwrap().prepared);
    assert!(matches!(
        f.run(WorkspaceAction::Save(request(None, Some(d.id.clone()))))
            .await,
        Err(WorkspaceError::InvalidInput)
    ));
    let mut request = ProxySave {
        proxy: ProxyRecord {
            id: new_id(),
            name: "proxy".into(),
            kind: "socks5".into(),
            host: "127.0.0.1".into(),
            port: 1080,
            username: "fixture".into(),
            version: 0,
            credential: None,
        },
        expected_version: None,
        draft_id: Some(d.id),
        clear_password: false,
    };
    let saved = f
        .done(
            f.run(WorkspaceAction::SaveProxy(request.clone()))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(saved.phase, "success");
    request.proxy = saved.proxy.unwrap();
    let first = request.proxy.credential.clone();
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    let WorkspaceResult::Draft(d) = f
        .run(WorkspaceAction::BeginProxyDraft(Some(
            request.proxy.id.clone(),
        )))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.done(
        f.run(WorkspaceAction::ConfigureDraft {
            draft: d.id.clone(),
            kind: AuthKind::Password,
        })
        .await
        .unwrap(),
    )
    .await;
    request.expected_version = Some(request.proxy.version);
    request.draft_id = Some(d.id);
    request.proxy = f
        .done(
            f.run(WorkspaceAction::SaveProxy(request.clone()))
                .await
                .unwrap(),
        )
        .await
        .proxy
        .unwrap();
    assert_ne!(request.proxy.credential, first);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    request.expected_version = Some(request.proxy.version);
    request.draft_id = None;
    request.clear_password = true;
    let cleared = f
        .done(f.run(WorkspaceAction::SaveProxy(request)).await.unwrap())
        .await;
    assert_eq!(cleared.phase, "success");
    assert!(cleared.proxy.unwrap().credential.is_none());
    assert!(f.secrets.values.lock().unwrap().is_empty());
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn credential_removal_is_staged_for_password_and_private_key() {
    for kind in [AuthKind::Password, AuthKind::PrivateKey] {
        let f = Fixture::new();
        let WorkspaceResult::Draft(d) = f.run(WorkspaceAction::BeginDraft(None)).await.unwrap()
        else {
            panic!()
        };
        assert_eq!(d.intent, CredentialIntent::Keep);
        let captured = f
            .run(WorkspaceAction::ConfigureDraft {
                draft: d.id.clone(),
                kind,
            })
            .await
            .unwrap();
        assert_eq!(f.done(captured).await.phase, "success");
        let mut create = request(None, Some(d.id));
        create.input.auth = kind;
        let p = f
            .done(f.run(WorkspaceAction::Save(create)).await.unwrap())
            .await
            .connection
            .unwrap();
        let original = p.credential.clone();
        let WorkspaceResult::Draft(d) = f
            .run(WorkspaceAction::BeginDraft(Some(p.id.clone())))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            f.service
                .handle(&new_id(), WorkspaceAction::ClearDraft(d.id.clone()))
                .await,
            Err(WorkspaceError::NotFound)
        ));
        for _ in 0..2 {
            let WorkspaceResult::Draft(view) = f
                .run(WorkspaceAction::ClearDraft(d.id.clone()))
                .await
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(view.intent, CredentialIntent::Remove);
            assert!(!view.prepared && view.saved);
        }
        assert_eq!(
            f.service.connection(&p.id).await.unwrap().credential,
            original
        );
        assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
        assert!(matches!(
            f.run(WorkspaceAction::Test(TestRequest {
                connection_id: Some(p.id.clone()),
                input: None,
                draft_id: Some(d.id.clone())
            }))
            .await,
            Err(WorkspaceError::CredentialMissing)
        ));
        f.run(WorkspaceAction::DiscardDraft(d.id)).await.unwrap();
        assert_eq!(
            f.service.connection(&p.id).await.unwrap().credential,
            original
        );
        let WorkspaceResult::Draft(d) = f
            .run(WorkspaceAction::BeginDraft(Some(p.id.clone())))
            .await
            .unwrap()
        else {
            panic!()
        };
        f.run(WorkspaceAction::ClearDraft(d.id.clone()))
            .await
            .unwrap();
        let mut save = request(Some(&p), Some(d.id));
        save.input = p.input.clone();
        save.input.name = "saved without credential".into();
        let result = f
            .done(f.run(WorkspaceAction::Save(save)).await.unwrap())
            .await;
        assert_eq!(result.phase, "success");
        let updated = result.connection.unwrap();
        assert!(updated.credential.is_none() && updated.credential_label.is_none());
        assert!(updated.version > p.version && updated.revision > p.revision);
        assert_eq!(updated.input.name, "saved without credential");
        assert!(f.secrets.values.lock().unwrap().is_empty());
        assert!(matches!(
            f.run(WorkspaceAction::Open {
                id: p.id,
                columns: 80,
                rows: 24
            })
            .await,
            Err(WorkspaceError::CredentialMissing)
        ));
        f.service.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn credential_removal_conflict_and_cleanup_failure_preserve_correct_binding() {
    let f = Fixture::new();
    let p = f.create().await;
    let d = f.draft(Some(p.id.clone())).await;
    f.run(WorkspaceAction::ClearDraft(d.clone())).await.unwrap();
    let mut stale = request(Some(&p), Some(d.clone()));
    stale.expected_version = Some(p.version + 1);
    assert!(matches!(
        f.run(WorkspaceAction::Save(stale)).await,
        Err(WorkspaceError::Conflict)
    ));
    assert_eq!(
        f.service.connection(&p.id).await.unwrap().credential,
        p.credential
    );
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    assert_eq!(
        f.service.state.lock().unwrap().drafts[&d].intent,
        CredentialIntent::Remove
    );
    f.secrets.deny_remove.store(true, Ordering::SeqCst);
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(Some(&p), Some(d))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "success");
    let updated = result.connection.unwrap();
    assert!(updated.credential.is_none() && updated.credential_pending);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    f.secrets.deny_remove.store(false, Ordering::SeqCst);
    f.run(WorkspaceAction::ReconcileCredentials(p.id.clone()))
        .await
        .unwrap();
    assert!(f.secrets.values.lock().unwrap().is_empty());
    assert!(
        !f.service
            .connection(&p.id)
            .await
            .unwrap()
            .credential_pending
    );
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn credential_removal_can_be_replaced_or_saved_without_a_secret_on_creation() {
    let f = Fixture::new();
    let d = f.draft(None).await;
    f.run(WorkspaceAction::ClearDraft(d.clone())).await.unwrap();
    let replacement = f
        .run(WorkspaceAction::ConfigureDraft {
            draft: d.clone(),
            kind: AuthKind::Password,
        })
        .await
        .unwrap();
    assert_eq!(
        f.done(replacement).await.draft.unwrap().intent,
        CredentialIntent::Replace
    );
    f.run(WorkspaceAction::ClearDraft(d.clone())).await.unwrap();
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(None, Some(d))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "success");
    assert!(result.connection.unwrap().credential.is_none());
    assert!(f.secrets.values.lock().unwrap().is_empty());
    f.service.shutdown().await.unwrap();
}

struct SaveFailureDatabase {
    inner: Arc<dyn WorkspaceDatabase>,
    fail: Arc<AtomicBool>,
}
impl WorkspaceDatabase for SaveFailureDatabase {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if self.fail.load(Ordering::SeqCst) && matches!(command, DbCommand::Save { .. }) {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.inner.execute(command).await
        })
    }
}
struct CancelPrompt;
impl NativePrompt for CancelPrompt {
    fn capture(&self, _: AuthKind, _: Arc<AtomicBool>) -> FutureResult<'_, SecretMaterial> {
        Box::pin(async { Err(WorkspaceError::Cancelled) })
    }
    fn update_context(&self, _: DisplayContext) {}
}
#[tokio::test]
async fn credential_removal_survives_cancelled_input_and_failed_database_commit() {
    let mut f = Fixture::new();
    let p = f.create().await;
    f.service.shutdown().await.unwrap();
    let fail = Arc::new(AtomicBool::new(true));
    f.service = WorkspaceService::new(
        Arc::new(SaveFailureDatabase {
            inner: f.service.database.clone(),
            fail: fail.clone(),
        }),
        f.secrets.clone(),
        Arc::new(CancelPrompt),
        Arc::new(Transport),
        Arc::new(Sink),
    );
    let WorkspaceResult::Draft(d) = f
        .run(WorkspaceAction::BeginDraft(Some(p.id.clone())))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.service
        .state
        .lock()
        .unwrap()
        .drafts
        .get_mut(&d.id)
        .unwrap()
        .busy = true;
    assert!(matches!(
        f.run(WorkspaceAction::ClearDraft(d.id.clone())).await,
        Err(WorkspaceError::Busy)
    ));
    f.service
        .state
        .lock()
        .unwrap()
        .drafts
        .get_mut(&d.id)
        .unwrap()
        .busy = false;
    f.run(WorkspaceAction::ClearDraft(d.id.clone()))
        .await
        .unwrap();
    let cancelled = f
        .run(WorkspaceAction::ConfigureDraft {
            draft: d.id.clone(),
            kind: AuthKind::Password,
        })
        .await
        .unwrap();
    assert_eq!(f.done(cancelled).await.phase, "cancelled");
    assert_eq!(
        f.service.state.lock().unwrap().drafts[&d.id].intent,
        CredentialIntent::Remove
    );
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(Some(&p), Some(d.id.clone()))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "failed");
    assert_eq!(
        f.service.connection(&p.id).await.unwrap().credential,
        p.credential
    );
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
    assert!(!f.service.state.lock().unwrap().drafts[&d.id].busy);
    fail.store(false, Ordering::SeqCst);
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(Some(&p), Some(d.id))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "success");
    assert!(f.secrets.values.lock().unwrap().is_empty());
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn credential_removal_cleanup_recovers_with_a_reopened_database() {
    let mut f = Fixture::new();
    let p = f.create().await;
    let d = f.draft(Some(p.id.clone())).await;
    f.run(WorkspaceAction::ClearDraft(d.clone())).await.unwrap();
    f.secrets.deny_remove.store(true, Ordering::SeqCst);
    let result = f
        .done(
            f.run(WorkspaceAction::Save(request(Some(&p), Some(d))))
                .await
                .unwrap(),
        )
        .await;
    assert_eq!(result.phase, "success");
    assert!(result.connection.unwrap().credential_pending);
    f.secrets.deny_remove.store(false, Ordering::SeqCst);
    f.reopen().await;
    f.service.recover_credentials().await.unwrap();
    assert!(f.secrets.values.lock().unwrap().is_empty());
    let restored = f.service.connection(&p.id).await.unwrap();
    assert!(restored.credential.is_none() && !restored.credential_pending);
    f.service.shutdown().await.unwrap();
}

struct UnavailableDatabase;
impl WorkspaceDatabase for UnavailableDatabase {
    fn execute(&self, _: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async { Err(WorkspaceError::StorageUnavailable) })
    }
}

#[derive(Default)]
struct ToolsFixture {
    sample_delay: Mutex<Duration>,
    sample_gate: Mutex<Option<Arc<Semaphore>>>,
    sample_running: std::sync::atomic::AtomicUsize,
    sample_peak: std::sync::atomic::AtomicUsize,
    fail_sample: AtomicBool,
    conflict: AtomicBool,
    samples: std::sync::atomic::AtomicU64,
    running: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
    finish: AtomicBool,
    closed: AtomicBool,
}
impl crate::ports::ServerTools for ToolsFixture {
    fn sample(&self, _: bool) -> FutureResult<'_, vigilshell_domain::tools::RawSample> {
        Box::pin(async {
            let n = self.samples.fetch_add(1, Ordering::SeqCst) + 1;
            struct Active<'a>(&'a std::sync::atomic::AtomicUsize);
            impl Drop for Active<'_> {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            let active = self.sample_running.fetch_add(1, Ordering::SeqCst) + 1;
            self.sample_peak.fetch_max(active, Ordering::SeqCst);
            let _active = Active(&self.sample_running);
            let delay = *self.sample_delay.lock().unwrap();
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let gate = self.sample_gate.lock().unwrap().clone();
            if let Some(gate) = gate {
                let _permit = gate.acquire().await.unwrap();
            }

            if self.fail_sample.load(Ordering::SeqCst) {
                return Err(WorkspaceError::ToolsUnavailable);
            }
            Ok(vigilshell_domain::tools::RawSample {
                cpu_total: Some(n * 100),
                cpu_idle: Some(n * 50),
                view: vigilshell_domain::tools::ServerSample {
                    hostname: Some("fixture".into()),
                    memory_total: Some(1024),
                    memory_available: Some(512),
                    ..Default::default()
                },
                ..Default::default()
            })
        })
    }
    fn list(
        &self,
        path: String,
        _: Arc<AtomicBool>,
    ) -> FutureResult<'_, (String, Vec<vigilshell_domain::tools::RemoteEntry>)> {
        Box::pin(async move {
            Ok((
                path,
                (0..205)
                    .map(|n| vigilshell_domain::tools::RemoteEntry {
                        name: format!("文件 {n}"),
                        path: format!("/home/文件 {n}"),
                        kind: "file".into(),
                        size: Some(1),
                        modified: None,
                        permissions: None,
                        uid: None,
                        gid: None,
                    })
                    .collect(),
            ))
        })
    }
    fn mkdir(&self, _: String) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::FilesPermissionDenied) })
    }
    fn rename(&self, _: String, _: String) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::FilesExists) })
    }
    fn remove(&self, _: String, _: bool) -> FutureResult<'_, ()> {
        Box::pin(async { Err(WorkspaceError::FilesUnsupported) })
    }
    fn transfer(&self, io: crate::ports::TransferIo) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if self.conflict.load(Ordering::SeqCst) && !io.overwrite {
                return Err(WorkspaceError::FilesExists);
            }
            let n = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(n, Ordering::SeqCst);
            io.total.store(1024, Ordering::SeqCst);
            while !self.finish.load(Ordering::SeqCst) && !io.cancel.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            self.running.fetch_sub(1, Ordering::SeqCst);
            if io.cancel.load(Ordering::SeqCst) {
                Err(WorkspaceError::Cancelled)
            } else {
                io.progress.store(1024, Ordering::SeqCst);
                Ok(())
            }
        })
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async {
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
}
struct ToolsTransport(Arc<ToolsFixture>);
impl SshTransport for ToolsTransport {
    fn probe(&self, input: ConnectionInput) -> FutureResult<'_, HostKey> {
        Transport.probe(input)
    }
    fn authenticate(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        Transport.authenticate(input, key, secret)
    }
    fn tools(
        &self,
        _: ConnectionInput,
        _: HostKey,
        _: SecretMaterial,
        _: Option<ProxyRoute>,
    ) -> FutureResult<'_, Arc<dyn crate::ports::ServerTools>> {
        Box::pin(async { Ok(self.0.clone() as Arc<dyn crate::ports::ServerTools>) })
    }
}
#[tokio::test]
async fn server_tools_are_session_owned_paginated_and_transfers_survive_terminal_close() {
    use crate::tools::{ToolAction as A, ToolResult as R};
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools.clone())),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let before = f.secrets.reads.load(Ordering::SeqCst);
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    let group = f
        .service
        .state
        .lock()
        .unwrap()
        .groups
        .get(&(f.owner.clone(), profile.id.clone()))
        .unwrap()
        .clone();
    f.secrets.deny_read.store(true, Ordering::SeqCst);
    assert!(matches!(
        f.service
            .handle(
                &new_id(),
                WorkspaceAction::Tools(A::Refresh {
                    session: session.clone()
                })
            )
            .await,
        Err(WorkspaceError::Disconnected)
    ));
    let WorkspaceResult::Tools(R::Snapshot(first)) = f
        .run(WorkspaceAction::Tools(A::Refresh {
            session: session.clone(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(first.sample.cpu_percent.is_none());
    let WorkspaceResult::Tools(R::Snapshot(second)) = f
        .run(WorkspaceAction::Tools(A::Refresh {
            session: session.clone(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(second.sample.cpu_percent, Some(50.0));
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: f.monitor_group(&session),
        visible: false,
    }))
    .await
    .unwrap();
    let request = new_id();
    let WorkspaceResult::Tools(R::Directory(page)) = f
        .run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: "/home".into(),
            cursor: None,
            request: request.clone(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page.entries.len(), 100);
    let WorkspaceResult::Tools(R::Directory(page)) = f
        .run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: "/home".into(),
            cursor: page.next,
            request: request.clone(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page.entries.len(), 100);
    f.run(WorkspaceAction::Tools(A::CancelList {
        request: request.clone(),
    }))
    .await
    .unwrap();
    assert!(matches!(
        f.run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: "/home".into(),
            cursor: page.next,
            request
        }))
        .await,
        Err(WorkspaceError::NotFound)
    ));
    let WorkspaceResult::Tools(R::Grant(grant)) = f
        .run(WorkspaceAction::Tools(A::Grant {
            direction: "download".into(),
            paths: vec![f.path.clone()],
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.run(WorkspaceAction::Tools(A::Transfer {
        session: session.clone(),
        grant: grant.id,
        paths: vec!["/one".into(), "/two".into(), "/three".into()],
        overwrite: false,
    }))
    .await
    .unwrap();
    let WorkspaceResult::Tools(R::Transfers(tasks)) =
        f.run(WorkspaceAction::Tools(A::Transfers)).await.unwrap()
    else {
        panic!()
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        while tools.running.load(Ordering::SeqCst) != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(tasks.len(), 3);
    assert!(tasks.iter().all(|task| task.created_at > 0));
    assert!(
        tasks
            .iter()
            .all(|t| t.batch_size == 3 && t.batch_id == tasks[0].batch_id)
    );
    assert_eq!(tools.peak.load(Ordering::SeqCst), 2);
    assert_eq!(f.secrets.reads.load(Ordering::SeqCst) - before, 1);
    f.run(WorkspaceAction::Close(session)).await.unwrap();
    assert!(!tools.closed.load(Ordering::SeqCst));
    f.service.group_secret(&group).await.unwrap();
    assert!(
        f.service
            .credential_leased(profile.credential.as_ref().unwrap())
    );
    for task in &tasks {
        f.run(WorkspaceAction::Tools(A::CancelTransfer {
            id: task.id.clone(),
        }))
        .await
        .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let WorkspaceResult::Tools(R::Transfers(values)) =
                f.run(WorkspaceAction::Tools(A::Transfers)).await.unwrap()
            else {
                panic!()
            };
            if values.iter().all(|t| t.phase == "cancelled") && tools.closed.load(Ordering::SeqCst)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !f.service
            .credential_leased(profile.credential.as_ref().unwrap())
    );
    assert_eq!(
        f.service.group_secret(&group).await.unwrap_err(),
        WorkspaceError::Cancelled
    );
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn server_monitor_stops_after_three_failures_and_does_not_sample_when_hidden() {
    use crate::tools::ToolAction as A;
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    tools.fail_sample.store(true, Ordering::SeqCst);
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools.clone())),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: f.monitor_group(&session),
        visible: true,
    }))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        while tools.samples.load(Ordering::SeqCst) < 3 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(5100)).await;
    assert_eq!(tools.samples.load(Ordering::SeqCst), 3);
    tools.fail_sample.store(false, Ordering::SeqCst);
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: f.monitor_group(&session),
        visible: true,
    }))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: f.monitor_group(&session),
        visible: false,
    }))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let samples = tools.samples.load(Ordering::SeqCst);
    assert_eq!(samples, 3);
    f.run(WorkspaceAction::Tools(A::Refresh {
        session: session.clone(),
    }))
    .await
    .unwrap();
    let samples = tools.samples.load(Ordering::SeqCst);
    assert!(samples > 3);
    tokio::time::sleep(Duration::from_millis(5100)).await;
    assert_eq!(tools.samples.load(Ordering::SeqCst), samples);
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn directory_snapshots_are_independent_bounded_and_owner_checked() {
    use crate::tools::{ToolAction as A, ToolResult as R};
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools)),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    let mut requests = Vec::new();
    for n in 0..8 {
        let request = new_id();
        f.run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: format!("/{n}"),
            cursor: None,
            request: request.clone(),
        }))
        .await
        .unwrap();
        requests.push(request);
    }
    assert!(matches!(
        f.run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: "/limit".into(),
            cursor: None,
            request: new_id()
        }))
        .await,
        Err(WorkspaceError::Busy)
    ));
    assert!(matches!(
        f.service
            .handle(
                &new_id(),
                WorkspaceAction::Tools(A::ReleaseList {
                    request: requests[0].clone()
                })
            )
            .await,
        Err(WorkspaceError::NotFound)
    ));
    f.run(WorkspaceAction::Tools(A::CancelList {
        request: requests[0].clone(),
    }))
    .await
    .unwrap();
    let WorkspaceResult::Tools(R::Directory(page)) = f
        .run(WorkspaceAction::Tools(A::List {
            session: session.clone(),
            path: "/1".into(),
            cursor: Some("100".into()),
            request: requests[1].clone(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page.entries.len(), 100);
    for request in requests {
        f.run(WorkspaceAction::Tools(A::ReleaseList { request }))
            .await
            .unwrap();
    }
    assert!(f.service.state.lock().unwrap().listings.is_empty());
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn transfer_conflict_waits_without_permit_and_resolves_after_terminal_close() {
    use crate::tools::{ToolAction as A, ToolResult as R};
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    tools.conflict.store(true, Ordering::SeqCst);
    tools.finish.store(true, Ordering::SeqCst);
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools.clone())),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    let WorkspaceResult::Tools(R::Grant(grant)) = f
        .run(WorkspaceAction::Tools(A::Grant {
            direction: "download".into(),
            paths: vec![f.path.clone()],
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    f.run(WorkspaceAction::Tools(A::Transfer {
        session: session.clone(),
        grant: grant.id,
        paths: vec!["/a".into(), "/b".into(), "/c".into()],
        overwrite: false,
    }))
    .await
    .unwrap();
    let tasks = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let WorkspaceResult::Tools(R::Transfers(tasks)) =
                f.run(WorkspaceAction::Tools(A::Transfers)).await.unwrap()
            else {
                panic!()
            };
            if tasks.len() == 3 && tasks.iter().all(|t| t.phase == "awaitingDecision") {
                break tasks;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(tools.running.load(Ordering::SeqCst), 0);
    f.run(WorkspaceAction::Close(session)).await.unwrap();
    assert!(!tools.closed.load(Ordering::SeqCst));
    let cancelled = &tasks[1];
    f.run(WorkspaceAction::Tools(A::CancelTransfer {
        id: cancelled.id.clone(),
    }))
    .await
    .unwrap();
    assert!(
        f.run(WorkspaceAction::Tools(A::ResolveConflict {
            id: cancelled.id.clone(),
            version: cancelled.conflict_version,
            decision: "overwrite".into(),
            batch: false
        }))
        .await
        .is_err()
    );
    let task = &tasks[0];
    f.run(WorkspaceAction::Tools(A::ResolveConflict {
        id: task.id.clone(),
        version: task.conflict_version,
        decision: "overwrite".into(),
        batch: true,
    }))
    .await
    .unwrap();
    assert!(
        f.run(WorkspaceAction::Tools(A::ResolveConflict {
            id: task.id.clone(),
            version: task.conflict_version,
            decision: "overwrite".into(),
            batch: true
        }))
        .await
        .is_err()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let WorkspaceResult::Tools(R::Transfers(tasks)) =
                f.run(WorkspaceAction::Tools(A::Transfers)).await.unwrap()
            else {
                panic!()
            };
            if tasks.iter().filter(|t| t.phase == "complete").count() == 2
                && tasks.iter().filter(|t| t.phase == "cancelled").count() == 1
                && tools.closed.load(Ordering::SeqCst)
            {
                assert!(tasks.iter().all(|task| task.conflict.is_none()));
                assert!(task.created_at > 0);
                assert_eq!(
                    tasks.iter().find(|t| t.id == task.id).unwrap().created_at,
                    task.created_at
                );
                assert_eq!(
                    tasks
                        .iter()
                        .find(|t| t.id == cancelled.id)
                        .unwrap()
                        .created_at,
                    cancelled.created_at
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn server_monitor_starts_sampling_when_visible() {
    use crate::tools::ToolAction as A;
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools.clone())),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    assert_eq!(tools.samples.load(Ordering::SeqCst), 0);
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: f.monitor_group(&session),
        visible: true,
    }))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(7), async {
        while tools.samples.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn server_monitor_warms_up_once_then_obeys_interval_and_visibility() {
    use crate::ports::PreferencesRepository;
    use crate::tools::{ToolAction as A, ToolResult as R};
    use vigilshell_domain::{PreferencesPatch, UpdatePreferencesRequest};
    async fn settle() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }
    for interval in [1, 5, 15] {
        let mut f = Fixture::new();
        f.database
            .upgrade()
            .unwrap()
            .0
            .lock()
            .unwrap()
            .update(UpdatePreferencesRequest {
                patch: PreferencesPatch {
                    monitor_sample_seconds: Some(interval),
                    monitor_full_seconds: Some(if interval == 15 { 120 } else { 60 }),
                    ..Default::default()
                },
                base: PreferencesPatch {
                    monitor_sample_seconds: Some(5),
                    monitor_full_seconds: Some(60),
                    ..Default::default()
                },
                expected_revision: 0,
            })
            .unwrap();
        let tools = Arc::new(ToolsFixture::default());
        f.service = WorkspaceService::new(
            f.service.database.clone(),
            f.secrets.clone(),
            Arc::new(Prompt),
            Arc::new(ToolsTransport(tools.clone())),
            Arc::new(Sink),
        );
        let profile = f.create().await;
        let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: f.monitor_group(&session),
            visible: true,
        }))
        .await
        .unwrap();
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_millis(999)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 2);
        tokio::time::advance(Duration::from_secs(u64::from(interval)) - Duration::from_millis(1))
            .await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 2);
        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 3);
        let WorkspaceResult::Tools(R::Snapshot(snapshot)) = f
            .run(WorkspaceAction::Tools(A::Subscribe {
                group: f.monitor_group(&session),
                visible: false,
            }))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(snapshot.sample.cpu_percent, Some(50.0));
        tokio::time::advance(Duration::from_secs(60)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 3);
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: f.monitor_group(&session),
            visible: true,
        }))
        .await
        .unwrap();
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 4);
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: f.monitor_group(&session),
            visible: false,
        }))
        .await
        .unwrap();
        tokio::time::advance(Duration::from_secs(2)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 4);
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: f.monitor_group(&session),
            visible: true,
        }))
        .await
        .unwrap();
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 5);
        tokio::time::advance(Duration::from_secs(1)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 6);
        f.service.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn server_monitor_caps_concurrency_discards_hidden_work_and_shares_tabs() {
    use crate::tools::{ToolAction as A, ToolResult as R};
    let mut f = Fixture::new();
    let tools = Arc::new(ToolsFixture::default());
    let gate = Arc::new(Semaphore::new(0));
    *tools.sample_gate.lock().unwrap() = Some(gate.clone());
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools.clone())),
        Arc::new(Sink),
    );
    let mut groups = Vec::new();
    let mut profiles = Vec::new();
    let mut sessions = Vec::new();
    for _ in 0..4 {
        let profile = f.create().await;
        let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
        let group = f.monitor_group(&session);
        profiles.push(profile);
        sessions.push(session);
        groups.push(group.clone());
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group,
            visible: true,
        }))
        .await
        .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while tools.sample_running.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(tools.samples.load(Ordering::SeqCst), 2);
    let second = open_confirmed(&f, &profiles[0].id)
        .await
        .session_id
        .unwrap();
    assert_eq!(f.monitor_group(&second), groups[0]);
    f.run(WorkspaceAction::Close(sessions[0].clone()))
        .await
        .unwrap();
    f.run(WorkspaceAction::Tools(A::Subscribe {
        group: groups[0].clone(),
        visible: true,
    }))
    .await
    .unwrap();
    assert_eq!(tools.samples.load(Ordering::SeqCst), 2);
    assert!(matches!(
        f.service
            .handle(
                &new_id(),
                WorkspaceAction::Tools(A::Subscribe {
                    group: groups[0].clone(),
                    visible: true
                })
            )
            .await,
        Err(WorkspaceError::Disconnected)
    ));
    // Hide one in-flight group and one waiting for a global slot.
    for i in [0, 2] {
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: groups[i].clone(),
            visible: false,
        }))
        .await
        .unwrap();
    }
    gate.add_permits(2);
    tokio::time::timeout(Duration::from_secs(1), async {
        while tools.samples.load(Ordering::SeqCst) < 3
            || tools.sample_running.load(Ordering::SeqCst) > 0
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(tools.samples.load(Ordering::SeqCst), 3);
    assert_eq!(tools.sample_peak.load(Ordering::SeqCst), 2);
    let WorkspaceResult::Tools(R::Snapshot(snapshot)) = f
        .run(WorkspaceAction::Tools(A::Subscribe {
            group: groups[0].clone(),
            visible: false,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(snapshot.sample.collected_at, 0);
    f.service.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn server_monitor_counts_work_time_and_does_not_catch_up_overruns() {
    use crate::ports::PreferencesRepository;
    use crate::tools::ToolAction as A;
    use vigilshell_domain::{PreferencesPatch, UpdatePreferencesRequest};
    async fn settle() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }
    for (work_ms, start_ms) in [(400, 1000), (1200, 2000)] {
        let mut f = Fixture::new();
        f.database
            .upgrade()
            .unwrap()
            .0
            .lock()
            .unwrap()
            .update(UpdatePreferencesRequest {
                patch: PreferencesPatch {
                    monitor_sample_seconds: Some(1),
                    ..Default::default()
                },
                base: PreferencesPatch {
                    monitor_sample_seconds: Some(5),
                    ..Default::default()
                },
                expected_revision: 0,
            })
            .unwrap();
        let tools = Arc::new(ToolsFixture::default());
        *tools.sample_delay.lock().unwrap() = Duration::from_millis(work_ms);
        f.service = WorkspaceService::new(
            f.service.database.clone(),
            f.secrets.clone(),
            Arc::new(Prompt),
            Arc::new(ToolsTransport(tools.clone())),
            Arc::new(Sink),
        );
        let profile = f.create().await;
        let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
        f.run(WorkspaceAction::Tools(A::Subscribe {
            group: f.monitor_group(&session),
            visible: true,
        }))
        .await
        .unwrap();
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_millis(work_ms)).await;
        settle().await;
        tokio::time::advance(Duration::from_millis(start_ms - work_ms - 1)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(tools.samples.load(Ordering::SeqCst), 2);
        assert_eq!(tools.sample_peak.load(Ordering::SeqCst), 1);
        f.service.shutdown().await.unwrap();
    }
}

#[path = "session_end_tests.rs"]
mod session_end_tests;

mod file_management;

#[path = "agent_control_tests.rs"]
mod agent_control_tests;
