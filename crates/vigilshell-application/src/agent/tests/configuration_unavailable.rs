use super::*;

struct UnavailableConfiguration(std::sync::atomic::AtomicUsize);
impl WorkspaceDatabase for UnavailableConfiguration {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            assert!(matches!(
                command,
                DbCommand::Agent(AgentDbCommand::Configuration)
            ));
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(WorkspaceError::AgentConfigurationUnavailable)
        })
    }
}

#[tokio::test]
async fn invalid_optional_configuration_does_not_recover_or_rewrite_history() {
    let database = Arc::new(UnavailableConfiguration(
        std::sync::atomic::AtomicUsize::new(0),
    ));
    let service = AgentService::new(
        database.clone(),
        Arc::new(Connector),
        Arc::new(Sink::default()),
        Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
    );
    for _ in 0..2 {
        assert!(matches!(
            service.request(AgentOperation::Configuration {}).await,
            Err("agent_configuration_unavailable")
        ));
    }
    assert_eq!(database.0.load(Ordering::SeqCst), 2);
    assert!(service.state.lock().unwrap().tasks.is_empty());
    assert!(service.initialized.get().is_none());
}

struct ReadOnlyHistory;
impl WorkspaceDatabase for ReadOnlyHistory {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            let DbCommand::AgentHistory(command) = command else {
                panic!("history attempted initialization or mutation")
            };
            Ok(DbReply::AgentHistory(match command {
                AgentHistoryCommand::ActivitySources => AgentHistoryReply::ActivitySources(vec![]),
                AgentHistoryCommand::ActivitySummaries { .. } => {
                    AgentHistoryReply::ActivitySummaries(vec![])
                }
                AgentHistoryCommand::Recordings { .. } => {
                    AgentHistoryReply::Recordings(Default::default())
                }
                AgentHistoryCommand::Tasks { .. } => AgentHistoryReply::Tasks(Default::default()),
                AgentHistoryCommand::Activity { .. } => {
                    AgentHistoryReply::Activity(Default::default())
                }
                AgentHistoryCommand::Task { .. } => AgentHistoryReply::Task(None),
                _ => panic!("unexpected history query"),
            }))
        })
    }
}
#[tokio::test]
async fn history_queries_never_initialize_execution_or_read_configuration() {
    let service = AgentService::new(
        Arc::new(ReadOnlyHistory),
        Arc::new(Connector),
        Arc::new(Sink::default()),
        Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
    );
    for request in [
        AgentOperation::ActivitySources {},
        AgentOperation::ActivitySummaries {
            filter: Default::default(),
        },
        AgentOperation::Recordings {
            filter: Default::default(),
        },
        AgentOperation::Tasks {
            session_id: None,
            entry_id: None,
            connection_id: None,
            phase: None,
            offset: 0,
        },
        AgentOperation::Activity {
            session_id: "session".into(),
            task_id: None,
            cursor: 0,
            include_output: Some(true),
        },
    ] {
        service.request(request).await.unwrap();
    }
    for request in [
        AgentOperation::Task {
            task_id: "absent".into(),
        },
        AgentOperation::Output {
            task_id: "absent".into(),
            cursor: 0,
        },
        AgentOperation::LoadScreen {
            task_id: "absent".into(),
        },
    ] {
        assert_eq!(service.request(request).await.unwrap_err(), "not_found");
    }
    assert!(service.initialized.get().is_none());
}
