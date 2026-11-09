//! Test-only JSON-lines bridge to an explicitly selected disposable loopback SSH container.
use std::{
    io::{BufRead, Write},
    sync::{Arc, Mutex},
};
use vigilshell_application::{agent::local_execution::LocalExecutor, ports::*};
use vigilshell_domain::{agent::*, workspace::*};
use vigilshell_storage_sqlite::SqlitePreferences;
use vigilshell_transport_russh::RusshTransport;
struct Database(Mutex<SqlitePreferences>);
impl WorkspaceDatabase for Database {
    fn execute(&self, c: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move { self.0.lock().unwrap().execute(c) })
    }
}
struct Authority {
    policy: ConnectionPolicy,
    connection: Arc<dyn AgentConnection>,
}
impl ExecutionAuthority for Authority {
    fn current(&self, id: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess> {
        Box::pin(async move {
            if id.entry_id != "entry" || id.session_id != "session" || id.entry_generation != 1 {
                return Err(WorkspaceError::InvalidInput);
            }
            Ok(ExecutionAccess {
                policy: self.policy.clone(),
                connection: self.connection.clone(),
            })
        })
    }
}

#[tokio::main]
async fn main() {
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .expect("explicit loopback fixture port")
        .parse()
        .unwrap();
    let profile = ConnectionInput {
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
        environment: Environment::Test,
        name: "file acceptance fixture".into(),
        host: "127.0.0.1".into(),
        port,
        username: {
            let user = std::env::var("VIGILSHELL_TEST_NATIVE_USER").unwrap_or("root".into());
            assert!(matches!(user.as_str(), "root" | "fixture"));
            user
        },
        auth: AuthKind::Password,
    };
    let key = RusshTransport.probe(profile.clone()).await.unwrap();
    let connection = RusshTransport
        .agent(
            profile,
            key,
            SecretMaterial {
                diagnostic: None,
                kind: AuthKind::Password,
                bytes: b"native-fixture-only".to_vec().into(),
                label: String::new(),
            },
            None,
        )
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: true,
        execution: ExecutionPolicy {
            level: OperationLevel::Operate,
            script_review: std::env::var("VIGILSHELL_TEST_SCRIPT_REVIEW").as_deref() != Ok("false"),
            ..Default::default()
        },
    };
    let db = Arc::new(Database(Mutex::new(
        SqlitePreferences::open(dir.path()).unwrap(),
    )));
    db.execute(DbCommand::Agent(AgentDbCommand::recording(
        RecordingCommand::Create {
            session: RecordedSession {
                id: "session".into(),
                connection_id: "connection".into(),
                connection_name: "loopback".into(),
                entry_id: "entry".into(),
                entry_name: "fixture".into(),
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
        Arc::new(Authority {
            policy: policy.clone(),
            connection: connection.clone(),
        }),
        Arc::new(vigilshell_local_files::LocalFiles::new(vec![
            dir.path().into(),
        ])),
        16 * 1024 * 1024,
    )
    .unwrap();

    for line in std::io::stdin().lock().lines() {
        let value: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        if value["environment"] == true {
            println!(
                "{}",
                serde_json::to_string(&connection.probe_environment().await.unwrap()).unwrap()
            );
            std::io::stdout().flush().unwrap();
            continue;
        }
        let task = if let Some(id) = value["terminalTask"].as_str() {
            let task = service.get("entry", id).await.unwrap();
            let control = serde_json::from_value(value["control"].clone()).unwrap();
            let result = service
                .terminal_control(task.request.identity.clone(), control)
                .await;
            println!("{}", serde_json::json!({"delivery":format!("{result:?}")}));
            std::io::stdout().flush().unwrap();
            continue;
        } else if let Some(id) = value["approve"].as_str() {
            match service.get("entry", id).await {
                Ok(task) => {
                    if let ScriptReviewState::WaitingReview { mut binding } = task.state.review {
                        if let Some(hash) = value["hash"].as_str() {
                            binding.snapshot.hash = hash.into();
                        }
                        service.approve_script("entry", id, binding).await
                    } else {
                        Err("review_not_waiting")
                    }
                }
                Err(code) => Err(code),
            }
        } else if let Some(id) = value["reject"].as_str() {
            service.reject_script("entry", id).await
        } else if let Some(id) = value["cancel"].as_str() {
            service.cancel("entry", id).await.unwrap();
            service.get("entry", id).await
        } else if let Some(id) = value["get"].as_str() {
            service.get("entry", id).await
        } else {
            let operation: RuntimeOperation =
                serde_json::from_value(value["operation"].clone()).unwrap();
            let id = new_id();
            let request = ExecutionRequest {
                identity: ExecutionIdentity {
                    task_id: id.clone(),
                    entry_id: "entry".into(),
                    entry_generation: 1,
                    connection_id: "connection".into(),
                    connection_revision: 1,
                    session_id: "session".into(),
                    policy_revision: 1,
                },
                request_key: value["requestKey"].as_str().unwrap_or(&id).into(),
                timeout_seconds: Some(value["timeout"].as_u64().unwrap_or(180) as u32),
                operation,
            };
            let task = service.submit(request.clone()).await;
            if let Ok(task) = &task {
                assert_eq!(service.submit(request).await.unwrap().id, task.id);
            }
            task
        };
        let result = match task {
            Err(code) => serde_json::json!({"rejected":code}),
            Ok(task) => {
                let start = std::time::Instant::now();
                let mut cancelled = false;
                loop {
                    let task = service.get("entry", &task.id).await.unwrap();
                    if (value["returnRunning"] == true
                        && task.state.phase == ExecutionPhase::Running)
                        || !task.state.active()
                        || task.state.phase == ExecutionPhase::WaitingReview
                            && value["cancel"].is_null()
                        || value["get"].is_string()
                    {
                        let mut result = serde_json::to_value(&task).unwrap();
                        if value["includeOutput"] == true {
                            let DbReply::Agent(AgentDbReply::Activity(page)) = db
                                .execute(DbCommand::Agent(AgentDbCommand::recording(
                                    RecordingCommand::Read {
                                        include_output: true,
                                        session_id: "session".into(),
                                        task_id: Some(task.id.clone()),
                                        cursor: 0,
                                    },
                                )))
                                .await
                                .unwrap()
                            else {
                                panic!("recording reply")
                            };
                            result["recordedOutput"] = serde_json::to_value(page).unwrap();
                        }
                        break result;
                    }
                    if !cancelled
                        && value["cancelAfterMs"]
                            .as_u64()
                            .is_some_and(|ms| start.elapsed().as_millis() >= ms as u128)
                    {
                        service.cancel("entry", &task.id).await.unwrap();
                        cancelled = true;
                    }
                    if !cancelled
                        && value["disconnectAfterMs"]
                            .as_u64()
                            .is_some_and(|ms| start.elapsed().as_millis() >= ms as u128)
                    {
                        if !connection.is_closed() {
                            connection.close().await.unwrap();
                        }
                        cancelled = true;
                    }
                    assert!(
                        start.elapsed().as_secs() < 240,
                        "fixture task did not settle"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
        };
        println!("{}", result);
        std::io::stdout().flush().unwrap();
    }
    service.shutdown();
    if !connection.is_closed() {
        let _ = connection.close().await;
    }
}
