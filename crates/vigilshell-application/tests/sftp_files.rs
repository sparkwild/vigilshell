//! 专用无宿主挂载的 OpenSSH 容器；没有端口变量时不连接任何地址。
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::{Arc, Mutex};
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
struct Test {
    service: Arc<LocalExecutor>,
    connection: Arc<dyn AgentConnection>,
    policy: ConnectionPolicy,
    local: tempfile::TempDir,
    _db: tempfile::TempDir,
    root: String,
}
impl Test {
    async fn new() -> Self {
        let port = std::env::var("VIGILSHELL_TEST_SFTP_PORT")
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
            username: "fixture".into(),
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
                    bytes: b"runtime-fixture-only".to_vec().into(),
                    label: String::new(),
                },
                None,
            )
            .await
            .unwrap();
        let local = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let grant = LocalDirectoryGrant {
            id: "local".into(),
            path: local.path().to_str().unwrap().into(),
        };
        let policy = ConnectionPolicy {
            connection_id: "connection".into(),
            connection_revision: 1,
            revision: 1,
            verify_connect: true,
            execution: ExecutionPolicy {
                level: OperationLevel::Operate,
                file_transfer: true,
                upload_roots: vec![grant.clone()],
                download_roots: vec![grant],
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
            db,
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
        let t = Self {
            service,
            connection,
            policy,
            local,
            _db: dir,
            root: format!("srv/fixture/{}", new_id()),
        };
        t.ok(RuntimeOperation::FilesMkdir {
            target: t.path(""),
            parents: false,
        })
        .await;
        t
    }
    fn path(&self, name: &str) -> ScopedPath {
        ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: format!(
                "{}{}",
                self.root,
                if name.is_empty() {
                    String::new()
                } else {
                    format!("/{name}")
                }
            ),
        }
    }
    fn request(&self, operation: RuntimeOperation) -> ExecutionRequest {
        let id = new_id();
        ExecutionRequest {
            identity: ExecutionIdentity {
                task_id: id.clone(),
                entry_id: "entry".into(),
                entry_generation: 1,
                connection_id: "connection".into(),
                connection_revision: 1,
                session_id: "session".into(),
                policy_revision: 1,
            },
            request_key: id,
            timeout_seconds: Some(30),
            operation,
        }
    }
    async fn execute(&self, operation: RuntimeOperation) -> FileExecutionReport {
        let task = self.service.submit(self.request(operation)).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(40), async {
            loop {
                let task = self.service.get("entry", &task.id).await.unwrap();
                if !task.state.active() {
                    return task.file_report.unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn ok(&self, op: RuntimeOperation) -> serde_json::Value {
        let tool = serde_json::to_value(&op).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_owned();
        let r = self.execute(op).await;
        assert!(r.error.is_none(), "{tool}: {r:?}");
        serde_json::from_str(r.json.as_deref().unwrap()).unwrap()
    }
    async fn write(&self, name: &str, text: &str) -> serde_json::Value {
        self.ok(RuntimeOperation::FilesWrite {
            target: self.path(name),
            data: STANDARD.encode(text),
            mode: FileWriteMode::Create,
            expected_hash: None,
        })
        .await
    }
    async fn cleanup(self) {
        self.ok(RuntimeOperation::FilesRemove {
            target: self.path(""),
            recursive: true,
        })
        .await;
        self.connection.close().await.unwrap();
    }
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn file_actions_versions_metadata_and_paging() {
    let t = Test::new().await;
    let initial = t.write("original", "first second 中文").await;
    let hash = initial["hash"].as_str().unwrap();
    let conflict = t
        .execute(RuntimeOperation::FilesWrite {
            target: t.path("original"),
            data: STANDARD.encode("wrong"),
            mode: FileWriteMode::Replace,
            expected_hash: Some("0".repeat(64)),
        })
        .await;
    assert_eq!(conflict.error.as_deref(), Some("file_changed"));
    assert_eq!(conflict.commit, FileCommitState::Unchanged);
    let edit = t
        .ok(RuntimeOperation::FilesEdit {
            target: t.path("original"),
            edits: vec![
                TextReplacement {
                    before: "first".into(),
                    after: "ONE".into(),
                },
                TextReplacement {
                    before: "second".into(),
                    after: "TWO".into(),
                },
            ],
            expected_hash: hash.into(),
        })
        .await;
    t.ok(RuntimeOperation::FilesWrite {
        target: t.path("original"),
        data: STANDARD.encode(" appended"),
        mode: FileWriteMode::Append,
        expected_hash: Some(edit["hash"].as_str().unwrap().into()),
    })
    .await;
    let read = t
        .ok(RuntimeOperation::FilesRead {
            target: t.path("original"),
            offset: 0,
            length: 1024,
        })
        .await;
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        "ONE TWO 中文 appended".as_bytes()
    );
    t.ok(RuntimeOperation::FilesSetAttributes {
        target: t.path("original"),
        mode: Some(0o640),
        modified_seconds: Some(1_700_000_000),
    })
    .await;
    t.ok(RuntimeOperation::FilesCopy {
        source: t.path("original"),
        target: t.path("copy"),
        recursive: false,
        overwrite: false,
    })
    .await;
    t.ok(RuntimeOperation::FilesMove {
        source: t.path("copy"),
        target: t.path("moved"),
        overwrite: false,
    })
    .await;
    t.ok(RuntimeOperation::FilesLink {
        target: t.path("link"),
        destination: t.path("original"),
    })
    .await;
    let link = t
        .ok(RuntimeOperation::FilesStat {
            target: t.path("link"),
            hash: false,
        })
        .await;
    assert_eq!(link["kind"], "symlink");
    let blocked = t
        .execute(RuntimeOperation::FilesWrite {
            target: t.path("link"),
            data: STANDARD.encode("bad"),
            mode: FileWriteMode::Replace,
            expected_hash: None,
        })
        .await;
    assert_eq!(blocked.commit, FileCommitState::Unchanged);
    assert!(blocked.error.is_some());
    t.ok(RuntimeOperation::FilesRemove {
        target: t.path("link"),
        recursive: false,
    })
    .await;
    let mut after = None;
    let mut names = vec![];
    loop {
        let page = t
            .ok(RuntimeOperation::FilesList {
                target: t.path(""),
                after: after.clone(),
                limit: 1,
            })
            .await;
        for e in page["entries"].as_array().unwrap() {
            names.push(e["name"].as_str().unwrap().to_owned());
        }
        after = page["nextCursor"].as_str().map(str::to_owned);
        if after.is_none() {
            break;
        }
    }
    assert_eq!(names, vec!["moved", "original"]);
    t.ok(RuntimeOperation::FilesMkdir {
        target: t.path("nested/deep"),
        parents: true,
    })
    .await;
    t.write("nested/deep/file", "tree").await;
    t.ok(RuntimeOperation::FilesCopy {
        source: t.path("nested"),
        target: t.path("treecopy"),
        recursive: true,
        overwrite: false,
    })
    .await;
    t.cleanup().await;
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn scoped_binary_transfer_integrity_and_denials() {
    let t = Test::new().await;
    let bytes: Vec<_> = (0..200_000).map(|i| (i % 251) as u8).collect();
    std::fs::write(t.local.path().join("source"), &bytes).unwrap();
    let upload = t
        .ok(RuntimeOperation::TransfersUpload {
            local: ScopedPath {
                scope_id: "local".into(),
                path: "source".into(),
            },
            remote: t.path("remote"),
            overwrite: false,
        })
        .await;
    let download = t
        .ok(RuntimeOperation::TransfersDownload {
            remote: t.path("remote"),
            local: ScopedPath {
                scope_id: "local".into(),
                path: "download".into(),
            },
            overwrite: false,
        })
        .await;
    assert_eq!(upload["hash"], download["hash"]);
    assert_eq!(
        std::fs::read(t.local.path().join("download")).unwrap(),
        bytes
    );
    let denied = t
        .service
        .submit(t.request(RuntimeOperation::TransfersDownload {
            remote: t.path("remote"),
            local: ScopedPath {
                scope_id: "local".into(),
                path: "../escape".into(),
            },
            overwrite: false,
        }))
        .await;
    assert!(denied.is_err());
    let mut readonly = t.policy.clone();
    readonly.execution.level = OperationLevel::ReadOnly;
    assert!(
        check_file_operation(
            &t.request(RuntimeOperation::TransfersUpload {
                local: ScopedPath {
                    scope_id: "local".into(),
                    path: "source".into()
                },
                remote: t.path("forbidden"),
                overwrite: false
            }),
            &readonly
        )
        .is_err()
    );
    assert!(
        check_file_operation(
            &t.request(RuntimeOperation::FilesRemove {
                target: ScopedPath {
                    scope_id: ACCOUNT_SCOPE_ID.into(),
                    path: String::new()
                },
                recursive: true
            }),
            &t.policy
        )
        .is_err()
    );
    t.cleanup().await;
}

impl Test {
    async fn command(&self, command: String) -> String {
        let request = self.request(RuntimeOperation::Command {
            command,
            directory: self.path(""),
        });
        let mut process = self
            .connection
            .execute_command(check_command(&request, &self.policy).unwrap())
            .await
            .unwrap();
        let mut output = Vec::new();
        let mut exit = None;
        loop {
            match process.read().await.unwrap() {
                AgentProcessEvent::Stdout(b) => output.extend(b),
                AgentProcessEvent::Exit(c) => exit = Some(c),
                AgentProcessEvent::Closed => break,
                _ => {}
            }
        }
        assert_eq!(exit, Some(0));
        String::from_utf8(output).unwrap()
    }
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn metadata_append_identity_and_snapshot_cursor_are_preserved() {
    let t = Test::new().await;
    t.write("a", "one two").await;
    t.write("b", "two").await;
    let path = format!("/{}", t.path("a").path);
    let quoted = shell_quote(&path);
    t.command(format!("setfattr -n user.vigilshell -v retained {quoted}"))
        .await;
    t.ok(RuntimeOperation::FilesSetAttributes {
        target: t.path("a"),
        mode: Some(0o640),
        modified_seconds: None,
    })
    .await;
    let inode = t.command(format!("stat -c %i {quoted}")).await;
    t.ok(RuntimeOperation::FilesWrite {
        target: t.path("a"),
        data: STANDARD.encode(" suffix"),
        mode: FileWriteMode::Append,
        expected_hash: None,
    })
    .await;
    assert_eq!(inode, t.command(format!("stat -c %i {quoted}")).await);
    t.ok(RuntimeOperation::FilesWrite {
        target: t.path("a"),
        data: STANDARD.encode("replacement"),
        mode: FileWriteMode::Replace,
        expected_hash: None,
    })
    .await;
    assert_eq!(
        t.command(format!(
            "getfattr --only-values -n user.vigilshell {quoted}"
        ))
        .await,
        "retained"
    );
    assert_eq!(
        t.ok(RuntimeOperation::FilesStat {
            target: t.path("a"),
            hash: false
        })
        .await["mode"],
        0o640
    );
    let page = t
        .ok(RuntimeOperation::FilesList {
            target: t.path(""),
            after: None,
            limit: 1,
        })
        .await;
    t.write("aa-new", "new").await;
    let next = t
        .ok(RuntimeOperation::FilesList {
            target: t.path(""),
            after: Some(page["nextCursor"].as_str().unwrap().into()),
            limit: 10,
        })
        .await;
    assert_eq!(next["entries"].as_array().unwrap().len(), 1);
    assert_eq!(next["entries"][0]["name"], "b");
    let duplicate = t
        .execute(RuntimeOperation::FilesEdit {
            target: t.path("b"),
            edits: vec![TextReplacement {
                before: "absent".into(),
                after: "bad".into(),
            }],
            expected_hash: t
                .ok(RuntimeOperation::FilesStat {
                    target: t.path("b"),
                    hash: true,
                })
                .await["hash"]
                .as_str()
                .unwrap()
                .into(),
        })
        .await;
    assert_eq!(duplicate.commit, FileCommitState::Unchanged);
    assert!(duplicate.error.is_some());
    t.ok(RuntimeOperation::FilesLink {
        target: t.path("escape"),
        destination: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "etc/passwd".into(),
        },
    })
    .await;
    assert!(
        t.execute(RuntimeOperation::FilesRead {
            target: t.path("escape"),
            offset: 0,
            length: 10
        })
        .await
        .error
        .is_some()
    );
    // 递归清理只 unlink 链接自身，不跟随系统目标。
    t.cleanup().await;
}
struct CorruptUpload {
    step: u8,
    connection: Arc<dyn AgentConnection>,
    command: CheckedCommand,
}
impl AgentUpload for CorruptUpload {
    fn size(&self) -> u64 {
        4
    }
    fn read(&mut self) -> FutureResult<'_, Vec<u8>> {
        Box::pin(async move {
            if self.step == 0 {
                self.step = 1;
                return Ok(b"good".to_vec());
            }
            let mut process = self
                .connection
                .execute_command(self.command.clone())
                .await?;
            loop {
                if matches!(process.read().await?, AgentProcessEvent::Closed) {
                    break;
                }
            }
            self.step = 2;
            Ok(vec![])
        })
    }
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn corrupted_staging_hash_never_replaces_destination() {
    let t = Test::new().await;
    t.write("target", "original").await;
    let request = t.request(RuntimeOperation::TransfersUpload {
        local: ScopedPath {
            scope_id: "local".into(),
            path: "source".into(),
        },
        remote: t.path("target"),
        overwrite: true,
    });
    let checked = check_file_operation(&request, &t.policy).unwrap();
    let temporary = checked.staging_path(&format!("/{}", t.path("target").path));
    let command = check_command(
        &t.request(RuntimeOperation::Command {
            command: format!("printf bad > {}", shell_quote(&temporary)),
            directory: t.path(""),
        }),
        &t.policy,
    )
    .unwrap();
    let result = t
        .connection
        .execute_files(
            checked,
            FileTransferHandles {
                upload: Some(Box::new(CorruptUpload {
                    step: 0,
                    connection: t.connection.clone(),
                    command,
                })),
                download: None,
            },
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert_eq!(result.commit, FileCommitState::Unchanged);
    assert_eq!(result.error.as_deref(), Some("file_changed"));
    assert!(result.recovery_paths.is_empty());
    let read = t
        .ok(RuntimeOperation::FilesRead {
            target: t.path("target"),
            offset: 0,
            length: 20,
        })
        .await;
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"original"
    );
    t.cleanup().await;
}
struct StalledUpload;
impl AgentUpload for StalledUpload {
    fn size(&self) -> u64 {
        1
    }
    fn read(&mut self) -> FutureResult<'_, Vec<u8>> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn cancellation_interrupts_stalled_local_input_and_cleans_staging() {
    let t = Test::new().await;
    let request = t.request(RuntimeOperation::TransfersUpload {
        local: ScopedPath {
            scope_id: "local".into(),
            path: "source".into(),
        },
        remote: t.path("target"),
        overwrite: false,
    });
    let operation = check_file_operation(&request, &t.policy).unwrap();
    let control = FileExecutionControl::default();
    let signal = control.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        signal.store(true, std::sync::atomic::Ordering::Release);
    });
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        t.connection.execute_files(
            operation,
            FileTransferHandles {
                upload: Some(Box::new(StalledUpload)),
                download: None,
            },
            control,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.commit, FileCommitState::Unchanged);
    assert_eq!(result.error.as_deref(), Some("cancelled"));
    assert!(result.recovery_paths.is_empty());
    assert!(
        t.ok(RuntimeOperation::FilesList {
            target: t.path(""),
            after: None,
            limit: 10
        })
        .await["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    t.cleanup().await;
}

#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn cross_filesystem_move_refuses_without_copy_delete_fallback() {
    let t = Test::new().await;
    t.write("source", "unchanged").await;
    let mut policy = t.policy.clone();
    policy.execution.protect_system = false;
    let directory = ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: format!("dev/shm/vigilshell-{}", new_id()),
    };
    let run = |operation| {
        let request = t.request(operation);
        check_file_operation(&request, &policy).unwrap()
    };
    let created = t
        .connection
        .execute_files(
            run(RuntimeOperation::FilesMkdir {
                target: directory.clone(),
                parents: false,
            }),
            FileTransferHandles::default(),
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert!(created.error.is_none(), "{created:?}");
    let moved = t
        .connection
        .execute_files(
            run(RuntimeOperation::FilesMove {
                source: t.path("source"),
                target: ScopedPath {
                    path: format!("{}/destination", directory.path),
                    ..directory.clone()
                },
                overwrite: false,
            }),
            FileTransferHandles::default(),
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert_eq!(moved.commit, FileCommitState::Unchanged);
    assert_eq!(moved.error.as_deref(), Some("file_capability_unavailable"));
    let cleaned = t
        .connection
        .execute_files(
            run(RuntimeOperation::FilesRemove {
                target: directory,
                recursive: false,
            }),
            FileTransferHandles::default(),
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert!(cleaned.error.is_none());
    t.cleanup().await;
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn cancelled_recursive_removal_reports_partial_effects() {
    let t = Test::new().await;
    t.ok(RuntimeOperation::FilesMkdir {
        target: t.path("tree"),
        parents: false,
    })
    .await;
    let paths = (0..30)
        .map(|i| shell_quote(&format!("/{}/tree/f{i:02}", t.root)))
        .collect::<Vec<_>>()
        .join(" ");
    t.command(format!("touch {paths}")).await;
    let control = FileExecutionControl::default();
    let plan = check_file_operation(
        &t.request(RuntimeOperation::FilesRemove {
            target: t.path("tree"),
            recursive: true,
        }),
        &t.policy,
    )
    .unwrap();
    let monitor = async {
        loop {
            let stat = t
                .execute(RuntimeOperation::FilesStat {
                    target: t.path("tree/f00"),
                    hash: false,
                })
                .await;
            if stat.error.as_deref() == Some("file_not_found") {
                control
                    .cancel
                    .store(true, std::sync::atomic::Ordering::Release);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    let work = t
        .connection
        .execute_files(plan, FileTransferHandles::default(), control.clone());
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        tokio::join!(work, monitor)
    })
    .await
    .unwrap();
    let result = result.unwrap();
    assert_eq!(result.commit, FileCommitState::Partial);
    assert_eq!(result.error.as_deref(), Some("cancelled"));
    t.cleanup().await;
}

#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn file_modes_and_competing_versions_do_not_lose_updates() {
    let t = Test::new().await;
    let missing = t
        .execute(RuntimeOperation::FilesWrite {
            target: t.path("missing"),
            data: STANDARD.encode("bad"),
            mode: FileWriteMode::Create,
            expected_hash: Some("0".repeat(64)),
        })
        .await;
    assert_eq!(missing.commit, FileCommitState::Unchanged);
    assert_eq!(missing.error.as_deref(), Some("file_changed"));
    t.ok(RuntimeOperation::FilesWrite {
        target: t.path("upsert"),
        data: STANDARD.encode("initial"),
        mode: FileWriteMode::Replace,
        expected_hash: None,
    })
    .await;
    t.ok(RuntimeOperation::FilesWrite {
        target: t.path("append-new"),
        data: STANDARD.encode("new"),
        mode: FileWriteMode::Append,
        expected_hash: None,
    })
    .await;
    let hash = t
        .ok(RuntimeOperation::FilesStat {
            target: t.path("upsert"),
            hash: true,
        })
        .await["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let operation = |text| RuntimeOperation::FilesWrite {
        target: t.path("upsert"),
        data: STANDARD.encode(text),
        mode: FileWriteMode::Replace,
        expected_hash: Some(hash.clone()),
    };
    let (first, second) = tokio::join!(
        t.execute(operation("first")),
        t.execute(operation("second"))
    );
    assert_eq!(
        [&first, &second]
            .iter()
            .filter(|r| r.error.is_none())
            .count(),
        1
    );
    assert_eq!(
        [&first, &second]
            .iter()
            .filter(|r| r.error.as_deref() == Some("file_changed"))
            .count(),
        1
    );
    for path in ["source", "empty-copy", "empty-move"] {
        t.ok(RuntimeOperation::FilesMkdir {
            target: t.path(path),
            parents: false,
        })
        .await;
    }
    t.write("source/file", "content").await;
    t.ok(RuntimeOperation::FilesCopy {
        source: t.path("source"),
        target: t.path("empty-copy"),
        recursive: true,
        overwrite: true,
    })
    .await;
    t.ok(RuntimeOperation::FilesMove {
        source: t.path("source"),
        target: t.path("empty-move"),
        overwrite: true,
    })
    .await;
    let unsupported = t
        .execute(RuntimeOperation::FilesCopy {
            source: t.path("empty-copy"),
            target: t.path("empty-move"),
            recursive: true,
            overwrite: true,
        })
        .await;
    assert_eq!(unsupported.commit, FileCommitState::Unchanged);
    assert_eq!(
        unsupported.error.as_deref(),
        Some("file_capability_unavailable")
    );
    t.cleanup().await;
}
struct DisconnectUpload {
    sent: bool,
    connection: Arc<dyn AgentConnection>,
}
impl AgentUpload for DisconnectUpload {
    fn size(&self) -> u64 {
        4
    }
    fn read(&mut self) -> FutureResult<'_, Vec<u8>> {
        Box::pin(async move {
            if !self.sent {
                self.sent = true;
                return Ok(b"data".to_vec());
            }
            self.connection.close().await?;
            Ok(vec![])
        })
    }
}
#[tokio::test]
#[ignore = "requires explicit dedicated loopback OpenSSH/SFTP fixture"]
async fn disconnect_keeps_unknown_result_and_exact_recovery_object() {
    let t = Test::new().await;
    t.write("target", "original").await;
    let operation = check_file_operation(
        &t.request(RuntimeOperation::TransfersUpload {
            local: ScopedPath {
                scope_id: "local".into(),
                path: "source".into(),
            },
            remote: t.path("target"),
            overwrite: true,
        }),
        &t.policy,
    )
    .unwrap();
    let recovery = operation.staging_path(&format!("/{}", t.path("target").path));
    let report = t
        .connection
        .execute_files(
            operation,
            FileTransferHandles {
                upload: Some(Box::new(DisconnectUpload {
                    sent: false,
                    connection: t.connection.clone(),
                })),
                download: None,
            },
            FileExecutionControl::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.commit, FileCommitState::Unknown);
    assert_eq!(report.recovery_paths, vec![recovery]);
    let next = Test::new().await;
    let read = next
        .ok(RuntimeOperation::FilesRead {
            target: t.path("target"),
            offset: 0,
            length: 32,
        })
        .await;
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"original"
    );
    next.ok(RuntimeOperation::FilesRemove {
        target: t.path(""),
        recursive: true,
    })
    .await;
    next.cleanup().await;
}
