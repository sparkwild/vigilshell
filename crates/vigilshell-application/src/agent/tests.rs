use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use vigilshell_application_contracts::workspace::CredentialChange;
use vigilshell_domain::workspace::{AuthKind, ConnectionInput, Environment};
use vigilshell_storage_sqlite::SqlitePreferences;
mod access_deletion;
mod configuration_unavailable;
mod execution_boundary;
mod execution_controls;
mod maintenance_outcomes;
mod script_origin;
mod session_close;
mod system_settings;
// Synthetic configuration codec for Agent tests; never accesses the system key store.
#[derive(Debug)]
struct TestConfigurationCipher;
impl ConnectionConfigCipher for TestConfigurationCipher {
    fn seal(&self, id: &str, bytes: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
        let mut value = vec![0; 48];
        value.extend_from_slice(id.as_bytes());
        value.push(0);
        value.extend_from_slice(bytes);
        Ok(value)
    }
    fn open(
        &self,
        id: &str,
        bytes: &[u8],
    ) -> Result<vigilshell_credentials::VaultKeyMaterial, WorkspaceError> {
        let prefix = self.seal(id, &[])?;
        Ok(bytes
            .strip_prefix(prefix.as_slice())
            .ok_or(WorkspaceError::ConfigurationCorrupt)?
            .to_vec()
            .into())
    }
}
struct Database(Mutex<SqlitePreferences>, AtomicBool);
impl WorkspaceDatabase for Database {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if self.1.load(Ordering::Acquire)
                && matches!(
                    &command,
                    DbCommand::Execution(
                        ExecutionDbCommand::Register(_) | ExecutionDbCommand::Save { .. }
                    ) | DbCommand::Agent(AgentDbCommand::SaveAuditTask { .. })
                )
            {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.0.lock().unwrap().execute(command)
        })
    }
}
#[derive(Default)]
struct Sink(Mutex<Vec<AgentEvent>>);
impl AgentSink for Sink {
    fn publish(&self, event: AgentEvent) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.0.lock().unwrap().push(event);
            Ok(())
        })
    }
}
struct Connector;
impl AgentConnector for Connector {
    fn connect(&self, _: String, _: u64) -> FutureResult<'_, Arc<dyn AgentConnection>> {
        Box::pin(async { Err(WorkspaceError::ToolsUnavailable) })
    }
}
#[derive(Clone)]
struct Request {
    operation: RuntimeOperation,
}
#[derive(Default)]
struct Connection {
    requests: Mutex<Vec<Request>>,
    hold: Arc<AtomicBool>,
    confirm_stop: AtomicBool,
    closed: AtomicBool,
    inspected: Mutex<Vec<PolicyResourceQuery>>,
    inspect_hold: AtomicBool,
    inputs: Arc<Mutex<Vec<TerminalControl>>>,
    terminal_events: Arc<Mutex<VecDeque<AgentProcessEvent>>>,
    file_hold: AtomicBool,
    file_control: Mutex<Option<FileExecutionControl>>,
    native_result: Mutex<Option<NativeExecutionReport>>,
    unavailable: AtomicBool,
    snapshot_text: Mutex<Option<String>>,
    snapshot_reads: AtomicUsize,
}
fn support() -> ExecutionEnvironment {
    let policy = ConnectionPolicy::default();
    ExecutionEnvironment {
        operating_system: "Linux".into(),
        architecture: "aarch64".into(),
        account_uid: Some(1000),
        supported_tools: policy
            .tool_capabilities(None, false)
            .into_iter()
            .map(|t| t.tool)
            .collect(),
        sftp: true,
        atomic_rename: true,
        ..Default::default()
    }
}
struct Process {
    events: VecDeque<AgentProcessEvent>,
    inputs: Arc<Mutex<Vec<TerminalControl>>>,
    terminal_events: Arc<Mutex<VecDeque<AgentProcessEvent>>>,
    hold: Arc<AtomicBool>,
    terminal: bool,
    stopped: bool,
    confirm: bool,
}
impl AgentProcess for Process {
    fn read(&mut self) -> FutureResult<'_, AgentProcessEvent> {
        Box::pin(async move {
            if let Some(event) = self.events.pop_front() {
                return Ok(event);
            }
            loop {
                if self.terminal
                    && let Some(event) = self.terminal_events.lock().unwrap().pop_front()
                {
                    return Ok(event);
                }
                if self.stopped {
                    if self.confirm {
                        self.confirm = false;
                        self.events.push_back(AgentProcessEvent::Closed);
                        return Ok(AgentProcessEvent::Exit(143));
                    }
                    std::future::pending::<()>().await;
                }
                if !self.hold.load(Ordering::Acquire) && !self.terminal {
                    return Ok(AgentProcessEvent::Closed);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    }
    fn input<'a>(&'a mut self, bytes: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async move {
            self.inputs.lock().unwrap().push(TerminalControl::Write {
                text: String::from_utf8_lossy(bytes).into_owned(),
            });
            Ok(())
        })
    }
    fn resize(&mut self, columns: u32, rows: u32) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.inputs.lock().unwrap().push(TerminalControl::Resize {
                columns: columns as u16,
                rows: rows as u16,
            });
            Ok(())
        })
    }
    fn signal_stop(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.stopped = true;
            Ok(())
        })
    }
    fn stop(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.stopped = true;
            Ok(())
        })
    }
}
struct Prepared {
    connection: Arc<Connection>,
    operation: RuntimeOperation,
}
impl PreparedNativeOperation for Prepared {
    fn recovery_paths(&self) -> Vec<String> {
        vec![]
    }
    fn execute(
        self: Box<Self>,
        output: Arc<dyn NativeOperationOutput>,
        control: FileExecutionControl,
    ) -> FutureResult<'static, NativeExecutionReport> {
        Box::pin(async move {
            self.connection.requests.lock().unwrap().push(Request {
                operation: self.operation.clone(),
            });
            while self.connection.hold.load(Ordering::Acquire) {
                if control.cancel.load(Ordering::Acquire) {
                    return Ok(NativeExecutionReport {
                        outcome: Some(ExecutionOutcome::Unknown {
                            reason: ExecutionStopReason::Cancelled,
                        }),
                        effects: FileCommitState::Unknown,
                        error: Some("native_result_unknown".into()),
                        json: None,
                        recovery_paths: vec![],
                    });
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            output
                .append("stdout", "中文 output\n".as_bytes().to_vec())
                .await?;
            if let Some(report) = self.connection.native_result.lock().unwrap().clone() {
                return Ok(report);
            }
            Ok(NativeExecutionReport {
                outcome: Some(
                    if matches!(self.operation, RuntimeOperation::PowerControl { .. }) {
                        ExecutionOutcome::RequestAccepted {}
                    } else {
                        ExecutionOutcome::Completed { exit_code: 0 }
                    },
                ),
                effects: FileCommitState::Unchanged,
                error: None,
                json: None,
                recovery_paths: vec![],
            })
        })
    }
}
impl Connection {
    fn process(&self, terminal: bool, text: &str) -> Box<dyn AgentProcess> {
        let mut events = VecDeque::from([AgentProcessEvent::Stdout(text.as_bytes().to_vec())]);
        if !terminal && !self.hold.load(Ordering::Acquire) {
            events.push_back(AgentProcessEvent::Exit(0));
            events.push_back(AgentProcessEvent::Closed);
        }
        Box::new(Process {
            events,
            inputs: self.inputs.clone(),
            terminal_events: self.terminal_events.clone(),
            hold: self.hold.clone(),
            terminal,
            stopped: false,
            confirm: self.confirm_stop.load(Ordering::Acquire),
        })
    }
}
struct FakePreparedScript {
    connection: Arc<Connection>,
    operation: RuntimeOperation,
}
impl PreparedScript for FakePreparedScript {
    fn launch(self: Box<Self>) -> FutureResult<'static, Box<dyn AgentProcess>> {
        Box::pin(async move {
            self.connection.requests.lock().unwrap().push(Request {
                operation: self.operation,
            });
            Ok(self.connection.process(false, "SCRIPT_EXECUTED"))
        })
    }
}
impl AgentConnection for Connection {
    fn execute_files(
        &self,
        operation: CheckedFileOperation,
        _: FileTransferHandles,
        control: FileExecutionControl,
    ) -> FutureResult<'_, FileExecutionReport> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(Request {
                operation: operation.operation().clone(),
            });
            *self.file_control.lock().unwrap() = Some(control.clone());
            while self.file_hold.load(Ordering::Acquire) {
                if control.cancel.load(Ordering::Acquire) {
                    return Ok(FileExecutionReport {
                        commit: FileCommitState::Unchanged,
                        error: Some("cancelled".into()),
                        json: None,
                        recovery_paths: vec![],
                        bytes: control.bytes.load(Ordering::Acquire),
                        hash: None,
                    });
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let bytes = if let RuntimeOperation::FilesWrite { data, .. } = operation.operation() {
                STANDARD.decode(data).unwrap()
            } else {
                vec![]
            };
            use sha2::{Digest, Sha256};
            let hash = format!("{:x}", Sha256::digest(&bytes));
            Ok(FileExecutionReport {
                commit: FileCommitState::Committed,
                error: None,
                json: Some(serde_json::json!({"hash":hash}).to_string()),
                recovery_paths: vec![],
                bytes: bytes.len() as u64,
                hash: Some(hash),
            })
        })
    }

    fn probe_environment(&self) -> FutureResult<'_, ExecutionEnvironment> {
        Box::pin(async { Ok(support()) })
    }
    fn prepare_native(
        self: Arc<Self>,
        operation: CheckedNativeOperation,
    ) -> FutureResult<'static, Box<dyn PreparedNativeOperation>> {
        Box::pin(async move {
            if self.unavailable.load(Ordering::Acquire) {
                return Err(WorkspaceError::ToolsUnavailable);
            }
            let operation = operation.operation().clone();
            Ok(Box::new(Prepared {
                connection: self,
                operation,
            }) as Box<dyn PreparedNativeOperation>)
        })
    }
    fn execute_command(&self, _: CheckedCommand) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(Request {
                operation: RuntimeOperation::SystemGet {},
            });
            Ok(self.process(false, "command output"))
        })
    }
    fn execute_terminal(&self, _: CheckedTerminal) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(Request {
                operation: RuntimeOperation::SystemGet {},
            });
            Ok(self.process(true, "terminal ready"))
        })
    }
    fn prepare_script(
        &self,
        operation: CheckedScriptOperation,
    ) -> FutureResult<'_, ScriptPreparation> {
        Box::pin(async move {
            self.snapshot_reads.fetch_add(1, Ordering::Relaxed);
            let RuntimeOperation::Script {
                source,
                interpreter,
                arguments,
                directory,
            } = &operation.request().operation
            else {
                return Err(WorkspaceError::InvalidInput);
            };
            let root = format!(
                "/home/fixture/.local/share/vigilshell/script-staging/{}",
                operation.request().identity.task_id
            );
            let (path, text) = match source {
                ScriptSource::Inline { text } => (format!("{root}/entry"), text.clone()),
                ScriptSource::RemoteFile { target, .. } => (
                    format!("/{}", target.path),
                    self.snapshot_text
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(|| "printf source".into()),
                ),
            };
            use sha2::{Digest, Sha256};
            let snapshot = ScriptSnapshot {
                source_version: matches!(source, ScriptSource::RemoteFile { .. })
                    .then(|| "f".repeat(64)),
                hash: format!("{:x}", Sha256::digest(text.as_bytes())),
                path,
                text,
                directory: normalize_path("/", &directory.path).unwrap(),
                interpreter: *interpreter,
                arguments: arguments.clone(),
            };
            Ok(ScriptPreparation {
                snapshot,
                staging_path: matches!(source, ScriptSource::Inline { .. })
                    .then(|| format!("{root}/entry")),
            })
        })
    }
    fn prepare_script_launch(
        self: Arc<Self>,
        operation: CheckedScriptLaunch,
    ) -> FutureResult<'static, Box<dyn PreparedScript>> {
        Box::pin(async move {
            if let Some(snapshot) = operation.snapshot()
                && matches!(
                    operation.operation().request().operation,
                    RuntimeOperation::Script {
                        source: ScriptSource::RemoteFile { .. },
                        ..
                    }
                )
            {
                let current = self
                    .snapshot_text
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "printf source".into());
                if current != snapshot.text {
                    return Err(WorkspaceError::Conflict);
                }
            }
            Ok(Box::new(FakePreparedScript {
                connection: self,
                operation: operation.operation().request().operation.clone(),
            }) as Box<dyn PreparedScript>)
        })
    }
    fn inspect_resources(
        &self,
        query: PolicyResourceQuery,
    ) -> FutureResult<'_, PolicyResourcePage> {
        Box::pin(async move {
            self.inspected.lock().unwrap().push(query.clone());
            while self.inspect_hold.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(match query {
                PolicyResourceQuery::Directory { path, .. } => PolicyResourcePage::Directory {
                    path,
                    page: AgentFilesPage {
                        entries: vec![],
                        next_cursor: None,
                    },
                },
            })
        })
    }
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.closed.store(true, Ordering::Release);
            Ok(())
        })
    }
}
struct Fixture {
    database: Arc<Database>,
    _directory: tempfile::TempDir,
    service: Arc<AgentService>,
    connection: Arc<Connection>,
    sink: Arc<Sink>,
    entry: AccessEntry,
    connection_id: String,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_connector(Arc::new(Connector)).await
    }
    async fn with_connector(connector: Arc<dyn AgentConnector>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(Database(
            Mutex::new(SqlitePreferences::open(directory.path()).unwrap()),
            AtomicBool::new(false),
        ));
        {
            let mut db = database.0.lock().unwrap();
            let DbReply::VaultMetadata(meta) = db.execute(DbCommand::VaultMetadata).unwrap() else {
                panic!()
            };
            db.execute(DbCommand::InitializeVault {
                namespace: meta.namespace,
                key_version: meta.key_version,
            })
            .unwrap();
            db.execute(DbCommand::WithConfigurationCipher {
                cipher: Arc::new(TestConfigurationCipher),
                command: Box::new(DbCommand::ConfigurationReady),
            })
            .unwrap();
        }
        let sink = Arc::new(Sink::default());
        let service = AgentService::new(
            database.clone(),
            connector,
            sink.clone(),
            Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
        );
        service.initialize().await.unwrap();
        let connection_id = new_id();
        service
            .database
            .execute(DbCommand::Save {
                id: connection_id.clone(),
                expected_version: None,
                group_id: None,
                input: ConnectionInput {
                    name: "Fixture".into(),
                    host: "localhost".into(),
                    port: 22,
                    username: "fixture".into(),
                    auth: AuthKind::Password,
                    environment: Environment::Test,
                    notes: String::new(),
                    terminal: Box::default(),
                    proxy_id: None,
                    tunnels: Vec::new(),
                },
                credential: CredentialChange::Keep,
                trust: None,
            })
            .await
            .unwrap();
        let entry = AccessEntry {
            client: None,
            id: "entry".into(),
            name: "Configured entry name".into(),
            enabled: true,
            generation: 1,
        };
        let mut policy = ConnectionPolicy {
            connection_id: connection_id.clone(),
            connection_revision: 1,
            ..Default::default()
        };
        policy.execution.level = OperationLevel::Operate;
        policy.execution.file_transfer = true;
        policy.execution.protected_paths = vec![ProtectedPath {
            path: "/srv/app/secret".into(),
            kind: ScopeKind::Directory,
        }];
        let config = AgentConfiguration {
            settings: AgentSettings {
                enabled: true,
                revision: 1,
                ..Default::default()
            },
            entries: vec![AccessSecret {
                entry: entry.clone(),
                digest: "a".repeat(64),
                sealed: "fixture".into(),
            }],
            policies: vec![ConnectionPolicySettings {
                access_enabled: true,
                policy,
            }],
        };
        service
            .db(AgentDbCommand::SaveConfiguration {
                configuration: config.clone(),
                expected: 0,
            })
            .await
            .unwrap();
        let connection = Arc::new(Connection::default());
        let view = AgentContext {
            terminal: None,
            support: Some(support()),
            id: "session".into(),
            entry_id: entry.id.clone(),
            entry_name: entry.name.clone(),
            connection_id: connection_id.clone(),
            connection_name: "Fixture".into(),
            connection_revision: 1,
            phase: ContextPhase::Ready,
            cwd: "/srv/app".into(),
            environment: BTreeMap::new(),
            revision: 0,
            created_at: now(),
            error: None,
        };
        service.create_recording(&view, &entry).await.unwrap();
        {
            let mut state = service.state.lock().unwrap();
            state.config = config;
            state
                .connections
                .insert(format!("{connection_id}:1"), connection.clone());
            state.contexts.insert(
                view.id.clone(),
                Arc::new(Context {
                    target_tab: None,
                    created_terminal: false,
                    workspace_tab: Mutex::new(None),
                    closing: tokio::sync::Mutex::new(()),
                    recorded_policies: Mutex::new(Default::default()),
                    unconfirmed: AtomicBool::new(false),
                    entry_generation: 1,
                    view: Mutex::new(view),
                    cancel: CancellationToken::new(),
                }),
            );
        }
        Self {
            _directory: directory,
            database,
            service,
            connection,
            sink,
            entry,
            connection_id,
        }
    }
    async fn start(&self, operation: RuntimeOperation, key: &str) -> AgentTask {
        let reply = self
            .service
            .start_operation(
                self.entry.clone(),
                RuntimeCall {
                    parent_task_id: None,
                    session_id: "session".into(),
                    request_key: key.into(),
                    timeout_seconds: None,
                    operation,
                },
            )
            .await
            .unwrap();
        let AgentReply::Task { task } = reply else {
            panic!()
        };
        task
    }
    async fn finished(&self, id: &str) -> AgentTask {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let task = self.service.get_task(id).await.unwrap();
                if !task.phase.active() {
                    return task;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn running(&self, id: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if self.service.get_task(id).await.unwrap().phase == TaskPhase::Running {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
// 用固定系统查询验证准入和任务生命周期；真实文件 I/O 由 SFTP 夹具覆盖。
fn server_query() -> RuntimeOperation {
    RuntimeOperation::SystemGet {}
}

struct InspectorConnector(Arc<Connection>);
impl AgentConnector for InspectorConnector {
    fn open_workspace(
        &self,
        identity: WorkspaceAgentIdentity,
        _: Option<String>,
    ) -> AgentFuture<'_, WorkspaceTerminalTab> {
        Box::pin(async move {
            Ok(WorkspaceTerminalTab {
                id: "test-tab".into(),
                name: "test".into(),
                connection_id: identity.connection_id,
                control: None,
            })
        })
    }
    fn workspace_control_live(&self, _: String) -> AgentFuture<'_, bool> {
        Box::pin(async { Ok(true) })
    }

    fn connect(&self, _: String, _: u64) -> FutureResult<'_, Arc<dyn AgentConnection>> {
        Box::pin(async { Ok(self.0.clone() as Arc<dyn AgentConnection>) })
    }
}

#[tokio::test]
async fn every_new_session_requires_identity_even_when_transport_is_pooled() {
    let f = Fixture::new().await;
    for key in ["one", "two"] {
        let AgentReply::Context { context } = f
            .service
            .open_context(f.entry.clone(), &f.connection_id, key, None)
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(context.phase, ContextPhase::WaitingUser);
        assert_eq!(context.entry_name, "Configured entry name");
        assert!(
            f.sink
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event,AgentEvent::Attention {id,..} if id==&context.id))
        );
    }
    f.service.shutdown().await;
}

#[tokio::test]
async fn polling_and_playback_do_not_create_business_steps() {
    let f = Fixture::new().await;
    let task = f.start(server_query(), "read").await;
    f.finished(&task.id).await;
    assert!(matches!(
        f.connection.requests.lock().unwrap()[0].operation,
        RuntimeOperation::SystemGet {}
    ));
    let before = f.sink.0.lock().unwrap().len();
    for _ in 0..3 {
        f.service
            .invoke(
                f.entry.clone(),
                AgentAction::TaskReadOutput {
                    task_id: task.id.clone(),
                    cursor: None,
                    view: None,
                },
            )
            .await
            .unwrap();
    }
    assert_eq!(f.sink.0.lock().unwrap().len(), before);
    f.service.shutdown().await;
}

#[tokio::test]
async fn closing_policy_access_preserves_the_running_session() {
    let f = Fixture::new().await;
    f.connection.confirm_stop.store(true, Ordering::Release);
    f.connection.hold.store(true, Ordering::Release);
    let task = f.start(server_query(), "running-old-policy").await;
    f.running(&task.id).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.connection.requests.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut policy = f.service.policy(&f.connection_id);
    policy.execution.level = OperationLevel::Disabled;
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::SetPolicyAccess {
            connection_id: policy.connection_id.clone(),
            expected_revision: policy.revision,
            connection_revision: policy.connection_revision,
            enabled: false,
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    f.service.commit(&id).await.unwrap();
    assert_eq!(
        f.service.get_task(&task.id).await.unwrap().phase,
        TaskPhase::Running
    );
    assert!(
        !f.service.state.lock().unwrap().contexts["session"]
            .cancel
            .is_cancelled()
    );
    f.connection.hold.store(false, Ordering::Release);
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Succeeded);
    f.service.shutdown().await;
}

#[tokio::test]
async fn policy_browser_works_with_agent_access_off_and_releases_on_close() {
    let connection = Arc::new(Connection::default());
    let f = Fixture::with_connector(Arc::new(InspectorConnector(connection.clone()))).await;
    {
        let mut state = f.service.state.lock().unwrap();
        state.config.settings.enabled = false;
        state.config.policies[0].policy.execution.level = OperationLevel::Disabled;
    }
    let events = f.sink.0.lock().unwrap().len();
    assert!(matches!(
        f.service
            .open_policy_browser(f.connection_id.clone(), 2)
            .await,
        Err("connection_changed")
    ));
    let AgentReply::PolicyBrowser { id } = f
        .service
        .open_policy_browser(f.connection_id.clone(), 1)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        f.service
            .read_policy_resources(
                &id,
                PolicyResourceQuery::Directory {
                    path: "/".into(),
                    after: None
                }
            )
            .await
            .unwrap(),
        AgentReply::PolicyResources { .. }
    ));
    assert!(connection.requests.lock().unwrap().is_empty());
    assert_eq!(f.sink.0.lock().unwrap().len(), events);
    assert!(f.service.state.lock().unwrap().tasks.is_empty());
    f.service.close_policy_browser(&id).await;
    assert!(connection.closed.load(Ordering::Acquire));
    assert!(matches!(
        f.service
            .read_policy_resources(
                &id,
                PolicyResourceQuery::Directory {
                    path: "/".into(),
                    after: None
                }
            )
            .await,
        Err("resource_browser_expired")
    ));
    f.service.shutdown().await;
}

#[tokio::test]
async fn closing_policy_browser_cancels_a_pending_read() {
    let connection = Arc::new(Connection::default());
    connection.inspect_hold.store(true, Ordering::Release);
    let f = Fixture::with_connector(Arc::new(InspectorConnector(connection.clone()))).await;
    let AgentReply::PolicyBrowser { id } = f
        .service
        .open_policy_browser(f.connection_id.clone(), 1)
        .await
        .unwrap()
    else {
        panic!()
    };
    let service = f.service.clone();
    let key = id.clone();
    let read = tokio::spawn(async move {
        service
            .read_policy_resources(
                &key,
                PolicyResourceQuery::Directory {
                    path: "/".into(),
                    after: None,
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while connection.inspected.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    f.service.close_policy_browser(&id).await;
    assert!(matches!(
        read.await.unwrap(),
        Err("resource_browser_expired")
    ));
    f.service.shutdown().await;
}

#[tokio::test]
async fn workspace_deletion_refreshes_agent_configuration_and_expires_prior_approval() {
    let f = Fixture::new().await;
    let policy = f.service.state.lock().unwrap().config.policies[0]
        .policy
        .clone();
    let settings = f.service.state.lock().unwrap().config.settings.clone();
    let AgentReply::Challenge { id, .. } = f
        .service
        .request(AgentOperation::Prepare {
            change: AgentMutation::SettingsPatch {
                patch: AgentSettingsPatch::Tray {
                    show_tray: !settings.show_tray,
                    base_show_tray: settings.show_tray,
                },
            },
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let job = new_id();
    f.service
        .database
        .execute(DbCommand::BeginDeletion {
            id: policy.connection_id.clone(),
            version: 1,
            job: job.clone(),
        })
        .await
        .unwrap();
    f.service
        .database
        .execute(DbCommand::Delete(policy.connection_id))
        .await
        .unwrap();
    f.service
        .database
        .execute(DbCommand::FinishCredential(job))
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .request(AgentOperation::Commit { challenge_id: id })
            .await,
        Err("authorization_expired")
    ));
    let AgentReply::Configuration { configuration, .. } = f
        .service
        .request(AgentOperation::Configuration {})
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(configuration.policies.is_empty());
    assert_eq!(configuration.entries.len(), 1);
    assert_eq!(configuration.settings.revision, settings.revision + 1);
    let AgentReply::Challenge { id, .. } = f
        .service
        .request(AgentOperation::Prepare {
            change: AgentMutation::SettingsPatch {
                patch: AgentSettingsPatch::Tray {
                    show_tray: !settings.show_tray,
                    base_show_tray: settings.show_tray,
                },
            },
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    f.service
        .request(AgentOperation::Commit { challenge_id: id })
        .await
        .unwrap();
    assert!(f.service.state.lock().unwrap().config.policies.is_empty());
}

#[tokio::test]
async fn rejected_calls_keep_denied_tasks_recordings_and_never_execute() {
    for code in ["unsupported_tool", "invalid_tool_arguments"] {
        let f = Fixture::new().await;
        let reply = f
            .service
            .request(AgentOperation::Reject {
                digest: "a".repeat(64),
                tool: "fixture_rejected_call".into(),
                code: code.into(),
            })
            .await
            .unwrap();
        assert!(matches!(reply, AgentReply::Done {}));
        let tasks = f
            .sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| {
                if let AgentEvent::Task { task } = event {
                    Some(task.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.phase, TaskPhase::Denied);
        assert_eq!(task.error.as_deref(), Some(code));
        assert!(task.finished_at.is_some());
        assert!(task.operation.is_none());
        let DbReply::AgentHistory(AgentHistoryReply::Task(Some(history))) = f
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Task {
                id: task.id.clone(),
            }))
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(history.display.phase.as_deref(), Some("denied"));
        assert_eq!(history.display.error.as_deref(), Some(code));
        let AgentReply::Recordings { sessions, .. } = f
            .service
            .request(AgentOperation::Recordings {
                filter: RecordingFilter {
                    session_id: Some(task.context_id.clone()),
                    ..Default::default()
                },
            })
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].result.as_deref(), Some("denied"));
        assert!(sessions[0].ended_at.is_some());
        assert!(!sessions[0].incomplete);
        assert!(f.connection.requests.lock().unwrap().is_empty());
        f.service.shutdown().await;
    }
}

#[tokio::test]
async fn rejection_storage_failure_never_publishes_a_completed_audit_task() {
    let f = Fixture::new().await;
    f.database.1.store(true, Ordering::Release);
    let result = f
        .service
        .request(AgentOperation::Reject {
            digest: "a".repeat(64),
            tool: "fixture_rejected_call".into(),
            code: "unsupported_tool".into(),
        })
        .await;
    assert!(matches!(result, Err("agent_storage_unavailable")));
    assert!(f.service.state.lock().unwrap().recording_blocked);
    assert!(
        !f.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, AgentEvent::Task { .. }))
    );
    assert!(f.connection.requests.lock().unwrap().is_empty());
    f.service.shutdown().await;
}

mod policy_settings;

mod workspace_takeover;
