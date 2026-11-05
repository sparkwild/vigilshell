use super::*;
#[derive(Default)]
struct WorkspacePort {
    fail_release: AtomicBool,
    identities: Mutex<Vec<WorkspaceAgentIdentity>>,
    releases: Mutex<Vec<(String, bool)>>,
    controls: Mutex<vigilshell_domain::workspace_control::WorkspaceControl>,
}
impl AgentConnector for WorkspacePort {
    fn open_workspace(
        &self,
        identity: WorkspaceAgentIdentity,
        tab_id: Option<String>,
    ) -> AgentFuture<'_, WorkspaceTerminalTab> {
        Box::pin(async move {
            match self
                .workspace_terminal(
                    identity.clone(),
                    WorkspaceTerminalAction::Takeover {
                        tab_id: tab_id.unwrap_or_else(|| format!("new-{}", identity.context_id)),
                        request_key: identity.context_id,
                    },
                )
                .await?
            {
                WorkspaceTerminalReply::Control { tab } => Ok(tab),
                _ => Err("unused"),
            }
        })
    }

    fn connect(&self, _: String, _: u64) -> FutureResult<'_, Arc<dyn AgentConnection>> {
        Box::pin(async { Ok(Arc::new(Connection::default()) as Arc<dyn AgentConnection>) })
    }
    fn workspace_terminal(
        &self,
        identity: WorkspaceAgentIdentity,
        command: WorkspaceTerminalAction,
    ) -> AgentFuture<'_, WorkspaceTerminalReply> {
        Box::pin(async move {
            self.identities.lock().unwrap().push(identity.clone());
            match command {
                WorkspaceTerminalAction::Takeover {
                    tab_id,
                    request_key,
                } => {
                    let c = self
                        .controls
                        .lock()
                        .unwrap()
                        .acquire(&identity.entry_id, &identity.context_id, &request_key)
                        .map_err(|_| "terminal_control_released")?;
                    Ok(WorkspaceTerminalReply::Control {
                        tab: WorkspaceTerminalTab {
                            id: tab_id,
                            name: "existing tab".into(),
                            connection_id: identity.connection_id,
                            control: Some(WorkspaceAgentControl {
                                entry_id: identity.entry_id,
                                entry_name: identity.entry_name,
                                context_id: identity.context_id,
                                lease_id: c.lease_id,
                                releasing: false,
                            }),
                        },
                    })
                }
                WorkspaceTerminalAction::ListTabs {} => {
                    Ok(WorkspaceTerminalReply::Tabs { tabs: vec![] })
                }
                _ => Err("unused"),
            }
        })
    }
    fn workspace_control_live(&self, context_id: String) -> AgentFuture<'_, bool> {
        Box::pin(async move {
            Ok(self
                .controls
                .lock()
                .unwrap()
                .active()
                .is_some_and(|c| c.context_id == context_id))
        })
    }
    fn release_workspace(&self, context_id: String, finish: bool) -> AgentFuture<'_, ()> {
        Box::pin(async move {
            self.releases
                .lock()
                .unwrap()
                .push((context_id.clone(), finish));
            let mut state = self.controls.lock().unwrap();
            if let Some(c) = state
                .active()
                .filter(|c| c.context_id == context_id)
                .cloned()
            {
                state.begin_release(&c.lease_id).unwrap();
                if finish {
                    if self.fail_release.load(Ordering::Acquire) {
                        return Err("terminal_control_busy");
                    }
                    state.finish_release(&c.lease_id).unwrap();
                }
            }
            Ok(())
        })
    }
}
fn takeover(session: &str, key: &str) -> AgentOperation {
    AgentOperation::Invoke {
        digest: "a".repeat(64),
        call: AgentCall::TerminalManage(TerminalManageRequest::Workspace {
            session_id: session.into(),
            command: WorkspaceTerminalAction::Takeover {
                tab_id: "existing-tab".into(),
                request_key: key.into(),
            },
        }),
    }
}
#[tokio::test]
async fn mcp_workspace_takeover_reuses_authorization_and_requires_fresh_session_after_release() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    let first = f
        .service
        .request(takeover("session", "first"))
        .await
        .unwrap();
    let AgentReply::WorkspaceTerminal {
        result: WorkspaceTerminalReply::Control { tab },
    } = first
    else {
        panic!()
    };
    let first_lease = tab.control.unwrap().lease_id;
    let identity = port.identities.lock().unwrap()[0].clone();
    assert_eq!(identity.entry_id, f.entry.id);
    assert_eq!(identity.entry_name, f.entry.name);
    assert_eq!(identity.connection_id, f.connection_id);
    f.service
        .request(AgentOperation::CloseSession {
            session_id: "session".into(),
        })
        .await
        .unwrap();
    assert!(
        port.releases
            .lock()
            .unwrap()
            .contains(&("session".into(), false))
    );
    assert!(
        port.releases
            .lock()
            .unwrap()
            .contains(&("session".into(), true))
    );
    assert_eq!(
        f.service
            .request(takeover("session", "automatic-retry"))
            .await
            .unwrap_err(),
        "session_not_ready"
    );
    let AgentReply::Context { context } = f
        .service
        .request(AgentOperation::Invoke {
            digest: "a".repeat(64),
            call: AgentCall::SessionManage(SessionManageRequest::Open {
                tab_id: Some("existing-tab".into()),
                connection_id: f.connection_id.clone(),
                request_key: "new-user-task".into(),
            }),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        f.service
            .request(takeover(&context.id, "before-auth"))
            .await
            .unwrap_err(),
        "session_not_ready"
    );
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::ApproveConnection {
            context_id: context.id.clone(),
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
    tokio::time::timeout(Duration::from_secs(3), async {
        while f
            .service
            .context(&f.entry.id, &context.id)
            .unwrap()
            .view
            .lock()
            .unwrap()
            .phase
            != ContextPhase::Ready
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let tab = f
        .service
        .context(&f.entry.id, &context.id)
        .unwrap()
        .view
        .lock()
        .unwrap()
        .terminal
        .clone()
        .unwrap();
    assert_eq!(tab.id, "existing-tab");
    assert_ne!(tab.control.unwrap().lease_id, first_lease);
    assert_eq!(
        f.service
            .request(takeover("session", "late-old-request"))
            .await
            .unwrap_err(),
        "session_not_ready"
    );
    f.service.shutdown().await;
}

#[tokio::test]
async fn workspace_takeover_rejects_readonly_policy_before_accessing_the_terminal() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .level = OperationLevel::ReadOnly;
    assert_eq!(
        f.service
            .request(takeover("session", "blocked"))
            .await
            .unwrap_err(),
        "connection_denied"
    );
    assert!(port.identities.lock().unwrap().is_empty());
    f.service.shutdown().await;
}

#[tokio::test]
async fn workspace_takeover_release_failure_remains_locked_and_is_reported_to_ui() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    f.service
        .request(takeover("session", "first"))
        .await
        .unwrap();
    port.fail_release.store(true, Ordering::Release);
    assert_eq!(
        f.service
            .request(AgentOperation::CloseSession {
                session_id: "session".into()
            })
            .await
            .unwrap_err(),
        "terminal_control_busy"
    );
    assert!(!port.controls.lock().unwrap().manual_input_allowed());
    assert_eq!(
        f.service
            .request(takeover("session", "retry"))
            .await
            .unwrap_err(),
        "session_not_ready"
    );
    port.fail_release.store(false, Ordering::Release);
    f.service
        .request(AgentOperation::CloseSession {
            session_id: "session".into(),
        })
        .await
        .unwrap();
    assert!(port.controls.lock().unwrap().manual_input_allowed());
    f.service.shutdown().await;
}

#[tokio::test]
async fn workspace_takeover_rechecks_the_actual_connection_revision() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    let DbReply::Connection(profile) = f
        .database
        .execute(DbCommand::Get(f.connection_id.clone()))
        .await
        .unwrap()
    else {
        panic!()
    };
    let mut input = profile.input;
    input.port += 1;
    f.database
        .execute(DbCommand::Save {
            id: profile.id,
            expected_version: Some(profile.revision),
            group_id: profile.group_id,
            input,
            credential: CredentialChange::Keep,
            trust: None,
        })
        .await
        .unwrap();
    assert_eq!(
        f.service
            .request(takeover("session", "stale-profile"))
            .await
            .unwrap_err(),
        "connection_changed"
    );
    assert!(port.identities.lock().unwrap().is_empty());
    f.service.shutdown().await;
}

async fn open_authorized(f: &Fixture, target: Option<String>, key: &str) -> AgentContext {
    let AgentReply::Context { context } = f
        .service
        .request(AgentOperation::Invoke {
            digest: "a".repeat(64),
            call: AgentCall::SessionManage(SessionManageRequest::Open {
                connection_id: f.connection_id.clone(),
                request_key: key.into(),
                tab_id: target,
            }),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(context.phase, ContextPhase::WaitingUser);
    assert!(context.terminal.is_none());
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::ApproveConnection {
            context_id: context.id.clone(),
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
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let view = f
                .service
                .context(&f.entry.id, &context.id)
                .unwrap()
                .view
                .lock()
                .unwrap()
                .clone();
            if matches!(view.phase, ContextPhase::Ready | ContextPhase::Failed) {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn new_readonly_session_creates_a_visible_terminal_without_granting_raw_input() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .level = OperationLevel::ReadOnly;
    let context = open_authorized(&f, None, "new-readonly").await;
    assert_eq!(context.phase, ContextPhase::Ready);
    let tab = context.terminal.clone().unwrap();
    assert!(tab.id.starts_with("new-"));
    assert!(!port.controls.lock().unwrap().manual_input_allowed());
    let reply = f
        .service
        .request(AgentOperation::Invoke {
            digest: "a".repeat(64),
            call: AgentCall::TerminalManage(TerminalManageRequest::Workspace {
                session_id: context.id.clone(),
                command: WorkspaceTerminalAction::WriteTab {
                    tab_id: tab.id,
                    lease_id: tab.control.unwrap().lease_id,
                    request_key: "raw".into(),
                    text: "pwd\n".into(),
                },
            }),
        })
        .await;
    assert_eq!(reply.unwrap_err(), "connection_denied");
    let AgentReply::Task { task } = f
        .service
        .start_operation(
            f.entry.clone(),
            RuntimeCall {
                session_id: context.id.clone(),
                request_key: "inspect".into(),
                parent_task_id: None,
                timeout_seconds: None,
                operation: server_query(),
            },
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Succeeded);
    let repeated = f
        .service
        .open_context(f.entry.clone(), &f.connection_id, "new-readonly", None)
        .await
        .unwrap();
    assert!(matches!(repeated, AgentReply::Context { context: c } if c.id == context.id));
    assert_eq!(port.identities.lock().unwrap().len(), 1);
    assert_eq!(
        f.service
            .open_context(
                f.entry.clone(),
                &f.connection_id,
                "new-readonly",
                Some("existing-tab".into())
            )
            .await
            .unwrap_err(),
        "request_conflict"
    );
    f.service.end_context(&context.id, true).await.unwrap();
    assert!(port.controls.lock().unwrap().manual_input_allowed());
    f.service.shutdown().await;
}

#[tokio::test]
async fn readonly_existing_tab_request_fails_without_creating_or_taking_a_terminal() {
    let port = Arc::new(WorkspacePort::default());
    let f = Fixture::with_connector(port.clone()).await;
    f.service.state.lock().unwrap().config.policies[0]
        .policy
        .execution
        .level = OperationLevel::ReadOnly;
    let context = open_authorized(&f, Some("existing-tab".into()), "readonly-existing").await;
    assert_eq!(context.phase, ContextPhase::Failed);
    assert_eq!(context.error.as_deref(), Some("connection_denied"));
    assert!(port.identities.lock().unwrap().is_empty());
    f.service.shutdown().await;
}
