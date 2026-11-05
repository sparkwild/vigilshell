use super::*;
use vigilshell_domain::agent::{
    WorkspaceTerminalAction as Action, WorkspaceTerminalReply as Reply,
};

struct ControlledSession {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    echo: VecDeque<Vec<u8>>,
}
impl SshSession for ControlledSession {
    fn open_terminal(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn read(&mut self) -> FutureResult<'_, TransportEvent> {
        Box::pin(async move {
            match self.echo.pop_front() {
                Some(bytes) => Ok(TransportEvent::Data(bytes)),
                None => std::future::pending().await,
            }
        })
    }
    fn write<'a>(&'a mut self, bytes: &'a [u8]) -> FutureResult<'a, ()> {
        Box::pin(async move {
            self.writes.lock().unwrap().push(bytes.to_vec());
            self.echo.push_back(bytes.to_vec());
            Ok(())
        })
    }
    fn resize(&mut self, _: u16, _: u16) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn close(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
async fn controlled_fixture() -> (
    Fixture,
    String,
    WorkspaceAgentIdentity,
    Arc<Mutex<Vec<Vec<u8>>>>,
    tokio::task::JoinHandle<Result<(), WorkspaceError>>,
) {
    let f = Fixture::new();
    let profile = f.create().await;
    let id = new_id();
    let identity = WorkspaceAgentIdentity {
        entry_id: "agent".into(),
        entry_name: "DeepSeek Harness".into(),
        context_id: "context-one".into(),
        connection_id: profile.id.clone(),
        connection_revision: profile.revision,
        terminal_seconds: Some(7200),
        idle_seconds: None,
    };
    let (tx, rx) = mpsc::channel(128);
    let cancel = CancellationToken::new();
    f.service.state.lock().unwrap().sessions.insert(
        id.clone(),
        Session {
            control_deadline: None,
            control_idle_deadline: None,
            control_idle_seconds: None,
            control: Default::default(),
            write_gate: Default::default(),
            output_history: Default::default(),
            output_bytes: 0,
            output_cursor: 0,
            agent_inputs: Default::default(),
            owner: f.owner.clone(),
            view: SessionView {
                agent_control: None,
                group_id: new_id(),
                id: id.clone(),
                operation_id: new_id(),
                connection: profile,
                phase: "running".into(),
                error: None,
                exit_code: None,
                end_reason: None,
            },
            cancel: cancel.clone(),
            controls: Some(tx),
        },
    );
    let service = f.service.clone();
    let owner = f.owner.clone();
    let sid = id.clone();
    let writes = Arc::new(Mutex::new(vec![]));
    let captured = writes.clone();
    let task = tokio::spawn(async move {
        service
            .run_session(
                &owner,
                &sid,
                Box::new(ControlledSession {
                    writes: captured,
                    echo: VecDeque::from([b"existing shell output\r\n".to_vec()]),
                }),
                rx,
                cancel,
            )
            .await
    });
    (f, id, identity, writes, task)
}
async fn wait_writes(writes: &Arc<Mutex<Vec<Vec<u8>>>>, n: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while writes.lock().unwrap().len() < n {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn take(f: &Fixture, identity: &WorkspaceAgentIdentity, id: &str, key: &str) -> String {
    let Reply::Control { tab } = f
        .service
        .agent_terminal(
            identity.clone(),
            Action::Takeover {
                tab_id: id.into(),
                request_key: key.into(),
            },
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    tab.control.unwrap().lease_id
}
fn write(id: &str, lease: &str, key: &str, text: &str) -> Action {
    Action::WriteTab {
        tab_id: id.into(),
        lease_id: lease.into(),
        request_key: key.into(),
        text: text.into(),
    }
}

#[tokio::test]
async fn workspace_takeover_release_reacquire_keeps_the_same_terminal_and_rejects_old_requests() {
    let (f, id, identity, writes, task) = controlled_fixture().await;
    f.service
        .input(&f.owner, &id, 1, b"cd /srv/app\n".to_vec())
        .unwrap();
    wait_writes(&writes, 1).await;
    let lease = take(&f, &identity, &id, "first").await;
    f.service
        .input(&f.owner, &id, 2, b"accidental manual input\n".to_vec())
        .unwrap();
    f.service
        .agent_terminal(identity.clone(), write(&id, &lease, "command", "pwd\n"))
        .await
        .unwrap();
    f.service
        .agent_terminal(identity.clone(), write(&id, &lease, "command", "pwd\n"))
        .await
        .unwrap();
    assert_eq!(
        writes.lock().unwrap().as_slice(),
        [b"cd /srv/app\n".to_vec(), b"pwd\n".to_vec()]
    );
    assert_eq!(
        f.service
            .agent_terminal(identity.clone(), write(&id, &lease, "command", "other\n"))
            .await
            .unwrap_err(),
        "request_conflict"
    );
    let output = f
        .service
        .agent_terminal(
            identity.clone(),
            Action::ReadTab {
                tab_id: id.clone(),
                lease_id: lease.clone(),
                after: 0,
            },
        )
        .await
        .unwrap();
    let Reply::Output { text, .. } = output else {
        panic!()
    };
    assert!(text.contains("existing shell output"));
    assert!(matches!(
        f.run(WorkspaceAction::Tools(
            crate::tools::ToolAction::CreateFile {
                session: id.clone(),
                path: "/tmp/should-not-exist".into()
            }
        ))
        .await,
        Err(WorkspaceError::Busy)
    ));
    assert!(matches!(
        f.run(WorkspaceAction::Close(id.clone())).await,
        Err(WorkspaceError::Busy)
    ));
    f.service
        .release_agent_terminal(&identity.context_id, false)
        .await
        .unwrap();
    f.service
        .input(&f.owner, &id, 3, b"during handover\n".to_vec())
        .unwrap();
    assert_eq!(
        f.service
            .agent_terminal(identity.clone(), write(&id, &lease, "old", "stale\n"))
            .await
            .unwrap_err(),
        "terminal_control_released"
    );
    f.service
        .release_agent_terminal(&identity.context_id, true)
        .await
        .unwrap();
    f.service
        .input(&f.owner, &id, 4, b"manual again\n".to_vec())
        .unwrap();
    wait_writes(&writes, 3).await;
    assert_eq!(writes.lock().unwrap()[2], b"manual again\n");
    assert_eq!(
        f.service
            .agent_terminal(
                identity.clone(),
                Action::Takeover {
                    tab_id: id.clone(),
                    request_key: "first".into()
                }
            )
            .await
            .unwrap_err(),
        "terminal_control_released"
    );
    let mut next = identity.clone();
    next.context_id = "context-two".into();
    let next_lease = take(&f, &next, &id, "second").await;
    assert_ne!(next_lease, lease);
    f.service
        .agent_terminal(
            next.clone(),
            write(&id, &next_lease, "next", "echo resumed\n"),
        )
        .await
        .unwrap();
    assert_eq!(
        f.service
            .agent_terminal(identity.clone(), write(&id, &lease, "late", "stale\n"))
            .await
            .unwrap_err(),
        "terminal_control_released"
    );
    f.service
        .release_agent_terminal(&identity.context_id, true)
        .await
        .unwrap();
    assert!(f.service.agent_control_live(&next.context_id));
    f.service
        .release_agent_terminal(&next.context_id, false)
        .await
        .unwrap();
    f.service
        .release_agent_terminal(&next.context_id, true)
        .await
        .unwrap();
    assert_eq!(writes.lock().unwrap().len(), 4);
    f.service.state.lock().unwrap().sessions[&id]
        .cancel
        .cancel();
    task.await.unwrap().unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn workspace_control_checks_connection_identity_and_deadlines() {
    let (f, id, identity, _, task) = controlled_fixture().await;
    let mut wrong = identity.clone();
    wrong.connection_id = new_id();
    assert_eq!(
        f.service
            .agent_terminal(
                wrong,
                Action::Takeover {
                    tab_id: id.clone(),
                    request_key: "bad".into()
                }
            )
            .await
            .unwrap_err(),
        "terminal_unavailable"
    );
    let lease = take(&f, &identity, &id, "first").await;
    let mut other = identity.clone();
    other.entry_id = "other-agent".into();
    assert_eq!(
        f.service
            .agent_terminal(other, write(&id, &lease, "fake", "echo no\n"))
            .await
            .unwrap_err(),
        "terminal_control_released"
    );
    f.service
        .state
        .lock()
        .unwrap()
        .sessions
        .get_mut(&id)
        .unwrap()
        .control_deadline = Some(std::time::Instant::now() - Duration::from_secs(1));
    assert!(!f.service.agent_control_live(&identity.context_id));
    assert_eq!(
        f.service
            .agent_terminal(identity.clone(), write(&id, &lease, "expired", "echo no\n"))
            .await
            .unwrap_err(),
        "terminal_control_released"
    );
    f.service.state.lock().unwrap().sessions[&id]
        .cancel
        .cancel();
    task.await.unwrap().unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn workspace_control_paginates_escaped_output_below_ipc_frame_limit() {
    let (f, id, identity, _, task) = controlled_fixture().await;
    let lease = take(&f, &identity, &id, "page").await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.service.state.lock().unwrap().sessions[&id].output_cursor == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    {
        let mut state = f.service.state.lock().unwrap();
        let session = state.sessions.get_mut(&id).unwrap();
        session.output_history = (1..=6).map(|i| (i, vec![0; 32768])).collect();
        session.output_cursor = 6;
        session.output_bytes = 6 * 32768;
    }
    let first = f
        .service
        .agent_terminal(
            identity.clone(),
            Action::ReadTab {
                tab_id: id.clone(),
                lease_id: lease.clone(),
                after: 0,
            },
        )
        .await
        .unwrap();
    assert!(serde_json::to_vec(&first).unwrap().len() < 1024 * 1024);
    let Reply::Output {
        text,
        cursor,
        incomplete,
    } = first
    else {
        panic!()
    };
    assert_eq!(text.len(), 128 * 1024);
    assert_eq!(cursor, 4);
    assert!(!incomplete);
    let Reply::Output { text, cursor, .. } = f
        .service
        .agent_terminal(
            identity,
            Action::ReadTab {
                tab_id: id.clone(),
                lease_id: lease,
                after: cursor,
            },
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(text.len(), 64 * 1024);
    assert_eq!(cursor, 6);
    f.service.state.lock().unwrap().sessions[&id]
        .cancel
        .cancel();
    task.await.unwrap().unwrap();
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn new_agent_terminal_uses_normal_workspace_and_preserves_it_after_release() {
    let f = Fixture::new();
    let p = f.create().await;
    let identity = WorkspaceAgentIdentity {
        entry_id: "agent".into(),
        entry_name: "Cursor".into(),
        context_id: new_id(),
        connection_id: p.id.clone(),
        connection_revision: p.revision,
        terminal_seconds: Some(7200),
        idle_seconds: None,
    };
    assert_eq!(
        f.service
            .open_agent_terminal(identity.clone(), None)
            .await
            .unwrap_err(),
        "workspace_unavailable"
    );
    f.service.register_display_owner(f.owner.clone());
    assert_eq!(
        f.service
            .open_agent_terminal(identity.clone(), None)
            .await
            .unwrap_err(),
        "trust_required"
    );
    assert!(f.service.state.lock().unwrap().sessions.is_empty());
    // 建立保存的主机信任，和人工连接使用同一工作区端口。
    let WorkspaceResult::Operation(op) = f
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
            if let Some(c) = f.service.operation(&f.owner, &op.id).unwrap().challenge {
                break c.id;
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
    let manual = f
        .done(WorkspaceResult::Operation(op))
        .await
        .session_id
        .unwrap();
    let created = f
        .service
        .open_agent_terminal(identity.clone(), None)
        .await
        .unwrap();
    assert_ne!(created.id, manual);
    let WorkspaceResult::Sessions(sessions) = f.run(WorkspaceAction::Sessions).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(sessions.len(), 2);
    assert!(
        sessions
            .iter()
            .any(|s| s.id == created.id && s.agent_control.is_some())
    );
    let group = {
        let state = f.service.state.lock().unwrap();
        let s = &state.sessions[&created.id];
        assert!(!s.control.manual_input_allowed());
        assert!(state.sessions[&manual].control.manual_input_allowed());
        s.view.group_id.clone()
    };
    assert!(!group.is_empty());
    f.service
        .release_agent_terminal(&identity.context_id, false)
        .await
        .unwrap();
    assert!(
        !f.service.state.lock().unwrap().sessions[&created.id]
            .control
            .manual_input_allowed()
    );
    f.service
        .release_agent_terminal(&identity.context_id, true)
        .await
        .unwrap();
    assert!(
        f.service.state.lock().unwrap().sessions[&created.id]
            .control
            .manual_input_allowed()
    );
    assert_eq!(
        f.service.state.lock().unwrap().sessions[&created.id]
            .view
            .phase,
        "running"
    );
    let mut next = identity.clone();
    next.context_id = new_id();
    let reacquired = f
        .service
        .open_agent_terminal(next, Some(created.id.clone()))
        .await
        .unwrap();
    assert_eq!(reacquired.id, created.id);
    assert_ne!(
        reacquired.control.unwrap().lease_id,
        created.control.unwrap().lease_id
    );
    f.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn agent_first_terminal_restores_saved_host_trust_for_the_shared_file_workspace() {
    use crate::tools::{ToolAction, ToolResult};
    let mut f = Fixture::new();
    let profile = f.create().await;
    open_confirmed(&f, &profile.id).await;
    f.service.shutdown().await.unwrap();
    let tools = Arc::new(ToolsFixture::default());
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(ToolsTransport(tools)),
        Arc::new(Sink),
    );
    f.service.register_display_owner(f.owner.clone());
    assert!(f.service.state.lock().unwrap().groups.is_empty());
    let identity = WorkspaceAgentIdentity {
        entry_id: "agent".into(),
        entry_name: "DeepSeek Harness".into(),
        context_id: new_id(),
        connection_id: profile.id,
        connection_revision: profile.revision,
        terminal_seconds: Some(7200),
        idle_seconds: None,
    };
    let created = f
        .service
        .open_agent_terminal(identity.clone(), None)
        .await
        .unwrap();
    let list = |session: String| {
        WorkspaceAction::Tools(ToolAction::List {
            session,
            path: "/home".into(),
            cursor: None,
            request: new_id(),
        })
    };
    assert!(matches!(
        f.run(list(created.id.clone())).await.unwrap(),
        WorkspaceResult::Tools(ToolResult::Directory(_))
    ));
    f.service
        .release_agent_terminal(&identity.context_id, true)
        .await
        .unwrap();
    assert!(matches!(
        f.run(list(created.id)).await.unwrap(),
        WorkspaceResult::Tools(ToolResult::Directory(_))
    ));
    f.service.shutdown().await.unwrap();
}
