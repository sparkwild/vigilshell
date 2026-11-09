#[path = "../../../tests/fixtures/command_ssh.rs"]
mod fixture;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use vigilshell_application::{agent::local_execution::LocalExecutor, ports::*};
use vigilshell_domain::{agent::*, workspace::*};
use vigilshell_storage_sqlite::SqlitePreferences;
use vigilshell_transport_russh::RusshTransport;

struct Database {
    store: Mutex<SqlitePreferences>,
    fail: AtomicBool,
    fail_dispatch: AtomicBool,
    fail_finish: AtomicBool,
    fail_output: AtomicBool,
}
impl WorkspaceDatabase for Database {
    fn execute(&self, c: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if self.fail_finish.load(Ordering::SeqCst)
                && matches!(&c,DbCommand::Execution(ExecutionDbCommand::Save{task,..}) if task.state.phase==ExecutionPhase::Settled)
                || self.fail_output.load(Ordering::SeqCst)
                    && matches!(&c, DbCommand::Agent(AgentDbCommand::Recording(_)))
                || self.fail.load(Ordering::SeqCst)
                || self.fail_dispatch.load(Ordering::SeqCst)
                    && matches!(&c,DbCommand::Execution(ExecutionDbCommand::Save{task,..}) if task.state.phase==ExecutionPhase::Dispatching)
            {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.store.lock().unwrap().execute(c)
        })
    }
}
struct Authority {
    policy: Mutex<ConnectionPolicy>,
    connection: Arc<dyn AgentConnection>,
    revoked: AtomicBool,
}
impl ExecutionAuthority for Authority {
    fn current(&self, id: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess> {
        Box::pin(async move {
            if self.revoked.load(Ordering::SeqCst)
                || id.entry_id != "entry"
                || id.entry_generation != 1
                || id.session_id != "session"
            {
                return Err(WorkspaceError::InvalidInput);
            }
            Ok(ExecutionAccess {
                policy: self.policy.lock().unwrap().clone(),
                connection: self.connection.clone(),
            })
        })
    }
}
struct Test {
    server: fixture::Fixture,
    directory: tempfile::TempDir,
    db: Arc<Database>,
    authority: Arc<Authority>,
    service: Arc<LocalExecutor>,
}
impl Test {
    async fn new() -> Self {
        let server = fixture::Fixture::start().await;
        let directory = tempfile::tempdir().unwrap();
        let input = ConnectionInput {
            notes: String::new(),
            terminal: Box::default(),
            proxy_id: None,
            tunnels: vec![],
            environment: Environment::Test,
            name: "isolated command fixture".into(),
            host: "127.0.0.1".into(),
            port: server.port,
            username: "command-fixture".into(),
            auth: AuthKind::Password,
        };
        let key = RusshTransport.probe(input.clone()).await.unwrap();
        let connection = RusshTransport
            .agent(
                input,
                key,
                SecretMaterial {
                    diagnostic: None,
                    kind: AuthKind::Password,
                    bytes: b"synthetic-only".to_vec().into(),
                    label: String::new(),
                },
                None,
            )
            .await
            .unwrap();
        let authority = Arc::new(Authority {
            policy: Mutex::new(ConnectionPolicy {
                connection_id: "connection".into(),
                connection_revision: 1,
                revision: 1,
                verify_connect: true,
                execution: ExecutionPolicy {
                    level: OperationLevel::Operate,
                    limits: ExecutionLimits {
                        concurrent_tasks: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }),
            connection,
            revoked: AtomicBool::new(false),
        });
        let db = Arc::new(Database {
            store: Mutex::new(SqlitePreferences::open(directory.path()).unwrap()),
            fail: AtomicBool::new(false),
            fail_dispatch: AtomicBool::new(false),
            fail_finish: AtomicBool::new(false),
            fail_output: AtomicBool::new(false),
        });
        db.execute(DbCommand::Agent(AgentDbCommand::recording(
            RecordingCommand::Create {
                session: RecordedSession {
                    id: "session".into(),
                    entry_id: "entry".into(),
                    entry_name: "synthetic".into(),
                    connection_id: "connection".into(),
                    connection_name: "loopback".into(),
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
        let service = LocalExecutor::new(
            db.clone(),
            authority.clone(),
            Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
            16 * 1024 * 1024,
        )
        .unwrap();
        Self {
            server,
            directory,
            db,
            authority,
            service,
        }
    }
    fn request(&self, id: &str, command: &str) -> ExecutionRequest {
        ExecutionRequest {
            identity: ExecutionIdentity {
                task_id: id.into(),
                entry_id: "entry".into(),
                entry_generation: 1,
                connection_id: "connection".into(),
                connection_revision: 1,
                session_id: "session".into(),
                policy_revision: 1,
            },
            request_key: id.into(),
            timeout_seconds: Some(8),
            operation: RuntimeOperation::Command {
                command: command.into(),
                directory: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: self
                        .directory
                        .path()
                        .to_str()
                        .unwrap()
                        .trim_start_matches('/')
                        .into(),
                },
            },
        }
    }
    async fn finished(&self, id: &str) -> ExecutionTask {
        tokio::time::timeout(std::time::Duration::from_secs(12), async {
            loop {
                let t = self.service.get("entry", id).await.unwrap();
                if t.state.phase == ExecutionPhase::Settled {
                    return t;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn running(&self, id: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if self.service.get("entry", id).await.unwrap().state.phase
                    == ExecutionPhase::Running
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn loopback_cwd_streams_utf8_exit_and_idempotency() {
    let t = Test::new().await;
    let request = t.request(
        "one",
        "pwd; printf '%s' '中文'; ls /definitely-missing-command-fixture",
    );
    t.service.submit(request.clone()).await.unwrap();
    let task = t.finished("one").await;
    assert!(
        matches!(task.state.outcome,Some(ExecutionOutcome::Completed{exit_code}) if exit_code!=0)
    );
    let mut duplicate = request.clone();
    duplicate.identity.task_id = "unused".into();
    assert_eq!(t.service.submit(duplicate).await.unwrap().id, "one");
    let mut conflict = request;
    conflict.operation = t.request("one", "printf changed").operation;
    assert!(matches!(
        t.service.submit(conflict).await,
        Err("request_conflict")
    ));
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
    let DbReply::Agent(AgentDbReply::Activity(page)) =
        t.db.execute(DbCommand::Agent(AgentDbCommand::recording(
            RecordingCommand::Read {
                include_output: true,
                session_id: "session".into(),
                task_id: Some("one".into()),
                cursor: 0,
            },
        )))
        .await
        .unwrap()
    else {
        panic!()
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    for r in page.records {
        if let ActivityBody::Output { stream, text, .. } = r.body {
            if stream == "stdout" {
                stdout.push_str(&text);
            } else {
                stderr.push_str(&text);
            }
        }
    }
    assert!(stdout.contains("中文"));
    assert!(stdout.contains(t.directory.path().file_name().unwrap().to_str().unwrap()));
    assert!(stderr.contains("definitely-missing-command-fixture"));
    assert!(!t.server.state.commands.lock().unwrap()[0].contains("vigilshell-agent-runtime"));
}
#[tokio::test]
async fn zero_dispatch_on_denial_registration_failure_and_attempt_save_failure() {
    let t = Test::new().await;
    for command in ["rm -rf /", "echo ok; rm -rf /etc", "ls > /etc/passwd"] {
        let denied = t
            .service
            .submit(t.request(&format!("deny-{}", command.len()), command))
            .await
            .unwrap();
        assert!(denied.state.initial_denial());
    }
    t.db.fail.store(true, Ordering::SeqCst);
    assert!(
        t.service
            .submit(t.request("save", "printf forbidden"))
            .await
            .is_err()
    );
    t.db.fail.store(false, Ordering::SeqCst);
    t.db.fail_dispatch.store(true, Ordering::SeqCst);
    t.service
        .submit(t.request("attempt", "printf forbidden"))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(t.server.state.commands.lock().unwrap().is_empty());
    t.db.fail_dispatch.store(false, Ordering::SeqCst);
    t.service.recover().await.unwrap();
    assert!(matches!(
        t.finished("attempt").await.state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Restarted
        })
    ));
}
#[tokio::test]
async fn queued_revision_change_and_cancel_do_not_dispatch() {
    let t = Test::new().await;
    t.service
        .submit(t.request("hold", "sleep 2"))
        .await
        .unwrap();
    t.running("hold").await;
    t.service
        .submit(t.request("queued", "printf forbidden"))
        .await
        .unwrap();
    t.authority.policy.lock().unwrap().revision += 1;
    t.finished("hold").await;
    assert!(matches!(
        t.finished("queued").await.state.outcome,
        Some(ExecutionOutcome::NotExecuted { .. })
    ));
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn cancel_and_missing_exit_are_not_success_or_rollback() {
    let t = Test::new().await;
    t.service
        .submit(t.request("cancel", "sleep 2"))
        .await
        .unwrap();
    t.running("cancel").await;
    t.service.cancel("entry", "cancel").await.unwrap();
    assert!(matches!(
        t.finished("cancel").await.state.outcome,
        Some(ExecutionOutcome::Partial {
            reason: ExecutionStopReason::Cancelled,
            termination_confirmed: true
        })
    ));
    t.server.state.omit_exit.store(true, Ordering::SeqCst);
    t.service
        .submit(t.request("unknown", "printf done"))
        .await
        .unwrap();
    assert!(matches!(
        t.finished("unknown").await.state.outcome,
        Some(ExecutionOutcome::Unknown { .. })
    ));
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn readonly_uses_trusted_programs_and_rejects_before_ssh() {
    let t = Test::new().await;
    t.authority.policy.lock().unwrap().execution.level = OperationLevel::ReadOnly;
    for (index, c) in ["ls; id", "ls > out", "rm file", "ls $(pwd)", "sh script.sh"]
        .into_iter()
        .enumerate()
    {
        let denied = t
            .service
            .submit(t.request(&format!("deny-{index}"), c))
            .await
            .unwrap();
        assert!(denied.state.initial_denial());
    }
    t.service.submit(t.request("read", "ls -a")).await.unwrap();
    assert!(matches!(
        t.finished("read").await.state.outcome,
        Some(ExecutionOutcome::Completed { exit_code: 0 })
    ));
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn timeout_and_queued_cancel_use_distinct_evidence() {
    let t = Test::new().await;
    let mut r = t.request("timeout", "sleep 2");
    r.timeout_seconds = Some(1);
    t.service.submit(r).await.unwrap();
    t.running("timeout").await;
    t.service
        .submit(t.request("queued-cancel", "printf forbidden"))
        .await
        .unwrap();
    t.service.cancel("entry", "queued-cancel").await.unwrap();
    assert!(matches!(
        t.finished("queued-cancel").await.state.outcome,
        Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Cancelled
        })
    ));
    assert!(matches!(
        t.finished("timeout").await.state.outcome,
        Some(ExecutionOutcome::Partial {
            reason: ExecutionStopReason::TimedOut,
            termination_confirmed: true
        })
    ));
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn bad_cwd_cannot_run_a_later_command_in_account_home() {
    let t = Test::new().await;
    let mut r = t.request("cwd", "printf first; printf second");
    let RuntimeOperation::Command { directory, .. } = &mut r.operation else {
        panic!()
    };
    directory.path = "definitely-absent-vigilshell-fixture".into();
    t.service.submit(r).await.unwrap();
    assert!(matches!(
        t.finished("cwd").await.state.outcome,
        Some(ExecutionOutcome::Completed { exit_code: 125 })
    ));
}

#[tokio::test]
async fn queued_entry_revocation_or_connection_change_never_starts_second_command() {
    for revoke in [true, false] {
        let t = Test::new().await;
        t.service
            .submit(t.request("hold", "sleep 1"))
            .await
            .unwrap();
        t.running("hold").await;
        t.service
            .submit(t.request("later", "printf forbidden"))
            .await
            .unwrap();
        if revoke {
            t.authority.revoked.store(true, Ordering::SeqCst);
        } else {
            t.authority.policy.lock().unwrap().connection_revision += 1;
        }
        t.finished("hold").await;
        assert!(matches!(
            t.finished("later").await.state.outcome,
            Some(ExecutionOutcome::NotExecuted {
                reason: NotExecutedReason::Denied
            })
        ));
        assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn output_preflight_failure_never_dispatches() {
    let t = Test::new().await;
    t.db.fail_output.store(true, Ordering::SeqCst);
    t.service
        .submit(t.request("record", "printf forbidden"))
        .await
        .unwrap();
    assert!(matches!(
        t.finished("record").await.state.outcome,
        Some(ExecutionOutcome::NotExecuted { .. })
    ));
    assert!(t.server.state.commands.lock().unwrap().is_empty());
}
#[tokio::test]
async fn failed_result_save_surfaces_failure_and_recovers_unknown_without_replay() {
    let t = Test::new().await;
    t.db.fail_finish.store(true, Ordering::SeqCst);
    let request = t.request("result", "printf completed");
    t.service.submit(request.clone()).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if matches!(
                t.service.get("entry", "result").await,
                Err("agent_storage_unavailable")
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    t.db.fail_finish.store(false, Ordering::SeqCst);
    t.service.recover().await.unwrap();
    assert!(matches!(
        t.finished("result").await.state.outcome,
        Some(ExecutionOutcome::Unknown {
            reason: ExecutionStopReason::Restarted
        })
    ));
    assert_eq!(t.service.submit(request).await.unwrap().id, "result");
    assert_eq!(t.server.state.commands.lock().unwrap().len(), 1);
}
