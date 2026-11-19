//! Native preparation must not bypass durable admission, cancellation or final authorization.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;
use vigilshell_application::{agent::local_execution::LocalExecutor, ports::*};
use vigilshell_domain::{agent::*, workspace::*};
use vigilshell_storage_sqlite::SqlitePreferences;
#[derive(Default)]
struct Pause {
    entered: Notify,
    release: Notify,
}
impl Pause {
    async fn wait(&self) {
        self.entered.notify_one();
        self.release.notified().await;
    }
}
struct Database {
    store: Mutex<SqlitePreferences>,
    fail_register: AtomicBool,
    fail_dispatch: AtomicBool,
    fail_recover: AtomicBool,
    recovery_pause: Mutex<Option<Arc<Pause>>>,
    history_pause: Mutex<Option<Arc<Pause>>>,
}
impl WorkspaceDatabase for Database {
    fn execute(&self, c: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if matches!(
                &c,
                DbCommand::AgentHistory(AgentHistoryCommand::Tasks { .. })
            ) {
                let pause = self.history_pause.lock().unwrap().take();
                if let Some(pause) = pause {
                    pause.wait().await;
                }
            }
            if matches!(c, DbCommand::Execution(ExecutionDbCommand::Recover)) {
                let pause = self.recovery_pause.lock().unwrap().take();
                if let Some(pause) = pause {
                    pause.wait().await;
                }
                if self.fail_recover.load(Ordering::SeqCst) {
                    return Err(WorkspaceError::StorageUnavailable);
                }
            }
            if self.fail_register.load(Ordering::SeqCst)
                && matches!(c, DbCommand::Execution(ExecutionDbCommand::Register(_)))
                || self.fail_dispatch.load(Ordering::SeqCst)
                    && matches!(&c,DbCommand::Execution(ExecutionDbCommand::Save{task,..}) if task.state.phase==ExecutionPhase::Dispatching)
            {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.store.lock().unwrap().execute(c)
        })
    }
}
#[derive(Default)]
struct Native {
    prepared: AtomicUsize,
    executed: AtomicUsize,
    hold: AtomicBool,
    revoke: AtomicBool,
    revoked: AtomicBool,
    authorization_pause: Mutex<Option<Arc<Pause>>>,
}
impl AgentConnection for Native {
    fn prepare_script(
        &self,
        operation: CheckedScriptOperation,
    ) -> FutureResult<'_, ScriptPreparation> {
        use sha2::{Digest, Sha256};
        Box::pin(async move {
            let RuntimeOperation::Script {
                source: ScriptSource::RemoteFile { target, .. },
                interpreter,
                arguments,
                directory,
            } = &operation.request().operation
            else {
                return Err(WorkspaceError::InvalidInput);
            };
            let text = "printf fixture";
            Ok(ScriptPreparation {
                staging_path: None,
                snapshot: ScriptSnapshot {
                    source_version: Some("f".repeat(64)),
                    path: format!("/{}", target.path),
                    hash: format!("{:x}", Sha256::digest(text.as_bytes())),
                    text: text.into(),
                    directory: format!("/{}", directory.path),
                    interpreter: *interpreter,
                    arguments: arguments.clone(),
                },
            })
        })
    }

    fn prepare_native(
        self: Arc<Self>,
        _: CheckedNativeOperation,
    ) -> FutureResult<'static, Box<dyn PreparedNativeOperation>> {
        Box::pin(async move {
            self.prepared.fetch_add(1, Ordering::SeqCst);
            if self.revoke.load(Ordering::SeqCst) {
                self.revoked.store(true, Ordering::SeqCst);
            }
            while self.hold.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            Ok(Box::new(Prepared(self)) as Box<dyn PreparedNativeOperation>)
        })
    }

    fn is_closed(&self) -> bool {
        false
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Prepared(Arc<Native>);
impl PreparedNativeOperation for Prepared {
    fn recovery_paths(&self) -> Vec<String> {
        vec!["/fixture/native-stage".into()]
    }
    fn execute(
        self: Box<Self>,
        _: Arc<dyn NativeOperationOutput>,
        _: FileExecutionControl,
    ) -> FutureResult<'static, NativeExecutionReport> {
        Box::pin(async move {
            self.0.executed.fetch_add(1, Ordering::SeqCst);
            Ok(NativeExecutionReport {
                outcome: Some(ExecutionOutcome::Completed { exit_code: 0 }),
                effects: FileCommitState::Committed,
                error: None,
                json: Some("{}".into()),
                recovery_paths: vec![],
            })
        })
    }
}
struct Authority {
    policy: ConnectionPolicy,
    native: Arc<Native>,
}
impl ExecutionAuthority for Authority {
    fn current(&self, identity: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess> {
        Box::pin(async move {
            if identity.connection_id == "connection" {
                let pause = self.native.authorization_pause.lock().unwrap().take();
                if let Some(pause) = pause {
                    pause.wait().await;
                }
            }
            if self.native.revoked.load(Ordering::SeqCst) {
                return Err(WorkspaceError::InvalidInput);
            }
            Ok(ExecutionAccess {
                policy: ConnectionPolicy {
                    connection_id: identity.connection_id,
                    ..self.policy.clone()
                },
                connection: self.native.clone(),
            })
        })
    }
}
struct Test {
    service: Arc<LocalExecutor>,
    db: Arc<Database>,
    native: Arc<Native>,
    _directory: tempfile::TempDir,
}
impl Test {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Arc::new(Database {
            store: Mutex::new(SqlitePreferences::open(directory.path()).unwrap()),
            fail_register: AtomicBool::new(false),
            fail_dispatch: AtomicBool::new(false),
            fail_recover: AtomicBool::new(false),
            recovery_pause: Mutex::new(None),
            history_pause: Mutex::new(None),
        });
        db.execute(DbCommand::Agent(AgentDbCommand::recording(
            RecordingCommand::Create {
                session: RecordedSession {
                    id: "session".into(),
                    connection_id: "connection".into(),
                    connection_name: "synthetic".into(),
                    entry_id: "entry".into(),
                    entry_name: "synthetic".into(),
                    created_at: 1,
                    ended_at: None,
                    result: None,
                    incomplete: false,
                    last_sequence: 0,
                    bytes: 0,
                },
            },
        )))
        .await
        .unwrap();
        let native = Arc::new(Native::default());
        let policy = ConnectionPolicy {
            connection_id: "connection".into(),
            connection_revision: 1,
            revision: 1,
            verify_connect: true,
            execution: ExecutionPolicy {
                level: OperationLevel::Operate,
                script_review: true,
                ..Default::default()
            },
        };
        let service = LocalExecutor::new(
            db.clone(),
            Arc::new(Authority {
                policy,
                native: native.clone(),
            }),
            Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
            16 * 1024 * 1024,
        )
        .unwrap();
        Self {
            service,
            db,
            native,
            _directory: directory,
        }
    }
    fn request(&self) -> ExecutionRequest {
        ExecutionRequest {
            identity: ExecutionIdentity {
                task_id: "task".into(),
                entry_id: "entry".into(),
                entry_generation: 1,
                connection_id: "connection".into(),
                connection_revision: 1,
                session_id: "session".into(),
                policy_revision: 1,
            },
            request_key: "request".into(),
            timeout_seconds: Some(5),
            operation: RuntimeOperation::PackagesInstall {
                packages: vec![PackageTarget::Repository {
                    name: "example".into(),
                    version: None,
                }],
            },
        }
    }
    async fn settled(&self) -> ExecutionTask {
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                let task = self.service.get("entry", "task").await.unwrap();
                if !task.state.active() {
                    return task;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }
}
#[tokio::test]
async fn native_preparation_requires_registration_and_dispatch_persistence() {
    for register in [true, false] {
        let t = Test::new().await;
        t.db.fail_register.store(register, Ordering::SeqCst);
        t.db.fail_dispatch.store(!register, Ordering::SeqCst);
        let result = t.service.submit(t.request()).await;
        if register {
            assert!(result.is_err());
            assert_eq!(t.native.prepared.load(Ordering::SeqCst), 0);
        } else {
            assert!(result.is_ok());
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while t.service.get("entry", "task").await.is_ok() {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(t.native.prepared.load(Ordering::SeqCst), 1);
        }
        assert_eq!(t.native.executed.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn native_recheck_after_preparation_blocks_revoked_access() {
    let t = Test::new().await;
    t.native.revoke.store(true, Ordering::SeqCst);
    t.service.submit(t.request()).await.unwrap();
    let task = t.settled().await;
    assert!(matches!(
        task.state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Denied
        })
    ));
    assert_eq!(t.native.prepared.load(Ordering::SeqCst), 1);
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn cancelling_native_preparation_never_dispatches() {
    let t = Test::new().await;
    t.native.hold.store(true, Ordering::SeqCst);
    t.service.submit(t.request()).await.unwrap();
    while t.native.prepared.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    t.service.cancel("entry", "task").await.unwrap();
    let task = t.settled().await;
    assert!(matches!(
        task.state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Cancelled
        })
    ));
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn packages_skip_standalone_review_and_reuse_receipt() {
    let t = Test::new().await;
    let req = t.request();
    t.service.submit(req.clone()).await.unwrap();
    let task = t.settled().await;
    assert!(matches!(
        task.state.review,
        ScriptReviewState::NotRequired {}
    ));
    assert!(matches!(
        task.state.outcome,
        Some(ExecutionOutcome::Completed { exit_code: 0 })
    ));
    let mut repeat = req.clone();
    repeat.identity.task_id = "duplicate".into();
    assert_eq!(t.service.submit(repeat).await.unwrap().id, task.id);
    let mut conflict = req;
    conflict.timeout_seconds = Some(2);
    assert_eq!(
        t.service.submit(conflict).await.unwrap_err(),
        "request_conflict"
    );
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn slow_authorization_does_not_block_other_connections_or_duplicate_admission() {
    let t = Test::new().await;
    let pause = Arc::new(Pause::default());
    *t.native.authorization_pause.lock().unwrap() = Some(pause.clone());
    let request = t.request();
    let first = {
        let service = t.service.clone();
        let request = request.clone();
        tokio::spawn(async move { service.submit(request).await })
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        pause.entered.notified().await;
        let mut other = t.request();
        other.identity.connection_id = "other-connection".into();
        other.identity.entry_id = "other-entry".into();
        other.identity.task_id = "other-task".into();
        let second = t.service.submit(other).await.unwrap();
        assert_eq!(second.id, "other-task");
        assert!(!first.is_finished());
        let repeat = {
            let service = t.service.clone();
            let mut duplicate = request.clone();
            duplicate.identity.task_id = "would-be-duplicate".into();
            tokio::spawn(async move { service.submit(duplicate).await })
        };
        pause.release.notify_one();
        assert_eq!(
            first.await.unwrap().unwrap().id,
            repeat.await.unwrap().unwrap().id
        );
    })
    .await;
    result.unwrap();
    let mut conflict = request;
    conflict.timeout_seconds = Some(4);
    assert!(matches!(
        t.service.submit(conflict).await,
        Err("request_conflict")
    ));
    t.settled().await;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while t
            .service
            .get("other-entry", "other-task")
            .await
            .unwrap()
            .state
            .active()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(t.native.prepared.load(Ordering::SeqCst), 2);
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn recovery_blocks_registration_and_fails_closed_until_it_succeeds() {
    let t = Test::new().await;
    let pause = Arc::new(Pause::default());
    *t.db.recovery_pause.lock().unwrap() = Some(pause.clone());
    t.db.fail_recover.store(true, Ordering::SeqCst);
    let service = t.service.clone();
    let recovery = tokio::spawn(async move { service.recover().await });
    tokio::time::timeout(std::time::Duration::from_secs(3), pause.entered.notified())
        .await
        .unwrap();
    let mut admission = std::pin::pin!(t.service.submit(t.request()));
    std::future::poll_fn(|cx| {
        use std::future::Future;
        assert!(admission.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let stored =
        t.db.execute(DbCommand::Execution(ExecutionDbCommand::Get {
            entry: "entry".into(),
            id: "task".into(),
        }))
        .await
        .unwrap();
    assert!(matches!(
        stored,
        DbReply::Execution(ExecutionDbReply::Task(None))
    ));
    pause.release.notify_one();
    assert_eq!(recovery.await.unwrap(), Err("agent_storage_unavailable"));
    assert!(matches!(admission.await, Err("agent_storage_unavailable")));
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 0);
    t.db.fail_recover.store(false, Ordering::SeqCst);
    t.service.recover().await.unwrap();
    assert!(
        t.service
            .submit(t.request())
            .await
            .unwrap()
            .state
            .outcome
            .is_none()
    );
    let task = t.settled().await;
    assert!(matches!(
        task.state.outcome,
        Some(ExecutionOutcome::Completed { exit_code: 0 })
    ));
}

#[tokio::test]
async fn rejected_registration_can_be_retried_without_a_leaked_reservation() {
    let t = Test::new().await;
    t.db.fail_register.store(true, Ordering::SeqCst);
    for i in 0..128 {
        let mut request = t.request();
        request.identity.task_id = format!("failed-{i}");
        request.request_key = request.identity.task_id.clone();
        assert!(matches!(
            t.service.submit(request).await,
            Err("agent_storage_unavailable")
        ));
    }
    assert_eq!(t.native.executed.load(Ordering::SeqCst), 0);
    t.db.fail_register.store(false, Ordering::SeqCst);
    t.service.submit(t.request()).await.unwrap();
    assert!(matches!(
        t.settled().await.state.outcome,
        Some(ExecutionOutcome::Completed { exit_code: 0 })
    ));
}

#[tokio::test]
async fn script_history_lookup_does_not_block_another_connection_from_registering() {
    let t = Test::new().await;
    let pause = Arc::new(Pause::default());
    *t.db.history_pause.lock().unwrap() = Some(pause.clone());
    let mut script = t.request();
    script.operation = RuntimeOperation::Script {
        source: ScriptSource::RemoteFile {
            target: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/fixture.sh".into(),
            },
            expected_hash: None,
        },
        interpreter: ScriptInterpreter::Sh,
        arguments: vec![],
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv".into(),
        },
    };
    let service = t.service.clone();
    let pending = tokio::spawn(async move { service.submit(script).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        pause.entered.notified().await;
        assert!(matches!(
            t.service.get("entry", "task").await,
            Err("not_found")
        ));
        t.service.recover().await.unwrap();
        let mut other = t.request();
        other.identity.task_id = "other-task".into();
        other.identity.connection_id = "other-connection".into();
        other.identity.entry_id = "other-entry".into();
        assert_eq!(t.service.submit(other).await.unwrap().id, "other-task");
        assert_eq!(
            t.service.get("other-entry", "other-task").await.unwrap().id,
            "other-task"
        );
        assert!(!pending.is_finished());
    })
    .await
    .unwrap();
    pause.release.notify_one();
    let script = pending.await.unwrap().unwrap();
    assert_eq!(script.state.phase, ExecutionPhase::WaitingReview);
    assert!(script.script_snapshot.is_some());
    t.service.cancel("entry", "task").await.unwrap();
    let script = t.settled().await;
    assert!(matches!(
        script.state.dispatch,
        DispatchState::NotDispatched {}
    ));
}
