//! Native preparation must not bypass durable admission, cancellation or final authorization.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use vigilshell_application::{agent::local_execution::LocalExecutor, ports::*};
use vigilshell_domain::{agent::*, workspace::*};
use vigilshell_storage_sqlite::SqlitePreferences;
struct Database {
    store: Mutex<SqlitePreferences>,
    fail_register: AtomicBool,
    fail_dispatch: AtomicBool,
}
impl WorkspaceDatabase for Database {
    fn execute(&self, c: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
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
}
impl AgentConnection for Native {
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
    fn current(&self, _: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess> {
        Box::pin(async move {
            if self.native.revoked.load(Ordering::SeqCst) {
                return Err(WorkspaceError::InvalidInput);
            }
            Ok(ExecutionAccess {
                policy: self.policy.clone(),
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
