use super::*;

async fn change(service: &Arc<AgentService>, change: AgentMutation) -> AgentResult<AgentReply> {
    let AgentReply::Challenge { id, .. } = service.prepare(change).await? else {
        panic!("administration must return a challenge")
    };
    service.commit(&id).await
}

async fn configuration(service: &AgentService) -> AgentConfiguration {
    let AgentDbReply::Configuration(config) =
        service.db(AgentDbCommand::Configuration).await.unwrap()
    else {
        panic!()
    };
    config
}

fn deletion(id: &str, generation: u64) -> AgentMutation {
    AgentMutation::DeleteAccess {
        id: id.into(),
        generation,
    }
}

async fn legacy_revoked_entry(f: &Fixture) {
    let mut config = configuration(&f.service).await;
    let expected = config.settings.revision;
    let entry = &mut config.entries[0];
    entry.entry.enabled = false;
    entry.entry.generation += 1;
    entry.digest.clear();
    entry.sealed.clear();
    config.settings.revision += 1;
    f.service
        .db(AgentDbCommand::SaveConfiguration {
            configuration: config,
            expected,
        })
        .await
        .unwrap();
    f.service.refresh_configuration().await.unwrap();
    f.service.stop_tasks(Some(&f.entry.id)).await;
}

async fn revoke(f: &Fixture) {
    change(
        &f.service,
        AgentMutation::Revoke {
            id: f.entry.id.clone(),
        },
    )
    .await
    .unwrap();
}

async fn history(database: &Database, entry: &str) -> Vec<String> {
    let commands = [
        DbCommand::AgentHistory(AgentHistoryCommand::Tasks {
            filter: TaskHistoryFilter {
                entry_id: Some(entry.into()),
                ..Default::default()
            },
        }),
        DbCommand::AgentHistory(AgentHistoryCommand::RequestReceipt {
            entry: entry.into(),
            key: "deletion-history".into(),
        }),
        DbCommand::AgentHistory(AgentHistoryCommand::Recordings {
            filter: RecordingFilter {
                entry_id: Some(entry.into()),
                ..Default::default()
            },
        }),
        DbCommand::AgentHistory(AgentHistoryCommand::Activity {
            session_id: "session".into(),
            task_id: None,
            cursor: 0,
            include_output: true,
        }),
        DbCommand::Agent(AgentDbCommand::Observations),
    ];
    let mut snapshot = Vec::new();
    for command in commands {
        snapshot.push(format!("{:?}", database.execute(command).await.unwrap()));
    }
    snapshot
}

fn recording_files(root: &std::path::Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &entry.path(), files);
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    let directory = root.join("agent-recordings");
    walk(&directory, &directory, &mut files);
    files
}

fn command() -> RuntimeOperation {
    RuntimeOperation::Command {
        command: "printf deletion-history".into(),
        directory: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/app".into(),
        },
    }
}

#[tokio::test]
async fn access_deletion_preserves_history_receipts_observations_and_files_after_reopen() {
    removal_preserves_history(false).await;
}

#[tokio::test]
async fn access_revocation_preserves_history_receipts_observations_and_files_after_reopen() {
    removal_preserves_history(true).await;
}

async fn removal_preserves_history(active: bool) {
    let f = Fixture::new().await;
    let task = f.start(command(), "deletion-history").await;
    f.finished(&task.id).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let AgentReply::Activity { page } = f
                .service
                .activity("session".into(), Some(task.id.clone()), 0, true)
                .await
                .unwrap()
            else {
                panic!()
            };
            if page.records.iter().any(|record| {
                matches!(
                    &record.body,
                    ActivityBody::Operation {
                        phase: TaskPhase::Succeeded,
                        ..
                    }
                )
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.service
        .db(AgentDbCommand::Observe {
            observation: AccessObservation {
                entry_id: f.entry.id.clone(),
                generation: 1,
                endpoint: "http://127.0.0.1:51602/mcp".into(),
                endpoint_revision: 0,
                first_at: now(),
                last_at: now(),
            },
        })
        .await
        .unwrap();
    if active {
        f.service.close_context("session").await;
    } else {
        legacy_revoked_entry(&f).await;
    }
    let before = history(&f.database, &f.entry.id).await;
    let files = recording_files(f._directory.path());
    assert!(!files.is_empty());
    assert!(before[0].contains(&task.id));
    assert!(before[1].contains("task_present: true"));
    let old = configuration(&f.service).await;
    if active {
        revoke(&f).await;
    } else {
        change(&f.service, deletion(&f.entry.id, 2)).await.unwrap();
    }
    let config = configuration(&f.service).await;
    assert!(config.entries.is_empty());
    assert_eq!(config.settings.revision, old.settings.revision + 1);
    assert_eq!(config.settings.port, old.settings.port);
    assert_eq!(config.settings.enabled, old.settings.enabled);
    assert_eq!(
        config.settings.endpoint_revision,
        old.settings.endpoint_revision
    );
    assert_eq!(
        serde_json::to_value(&config.policies).unwrap(),
        serde_json::to_value(&old.policies).unwrap()
    );
    assert_eq!(history(&f.database, &f.entry.id).await, before);
    assert_eq!(recording_files(f._directory.path()), files);
    assert_eq!(
        f.service.authenticate(&"a".repeat(64)).unwrap_err(),
        "access_denied"
    );
    let revision = config.settings.revision;
    change(&f.service, deletion(&f.entry.id, 2)).await.unwrap();
    assert_eq!(configuration(&f.service).await.settings.revision, revision);
    f.service.shutdown().await;
    let Fixture {
        _directory,
        database,
        service,
        entry,
        ..
    } = f;
    drop(service);
    drop(database);
    let database = Arc::new(Database(
        Mutex::new(SqlitePreferences::open(_directory.path()).unwrap()),
        AtomicBool::new(false),
    ));
    let reopened = AgentService::new(
        database.clone(),
        Arc::new(Connector),
        Arc::new(Sink::default()),
        Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
    );
    reopened.initialize().await.unwrap();
    assert!(configuration(&reopened).await.entries.is_empty());
    assert_eq!(history(&database, &entry.id).await, before);
    assert_eq!(recording_files(_directory.path()), files);
    reopened.shutdown().await;
}

#[tokio::test]
async fn access_deletion_revalidates_active_generation_and_concurrent_confirmations() {
    let f = Fixture::new().await;
    assert!(matches!(
        f.service.prepare(deletion(&f.entry.id, 1)).await,
        Err("access_not_revoked")
    ));
    legacy_revoked_entry(&f).await;
    let AgentReply::Challenge { id, .. } =
        f.service.prepare(deletion(&f.entry.id, 2)).await.unwrap()
    else {
        panic!()
    };
    let mut secret = configuration(&f.service).await.entries.remove(0);
    secret.entry.enabled = true;
    secret.entry.generation = 3;
    secret.digest = "b".repeat(64);
    secret.sealed = "synthetic-new-token".into();
    change(&f.service, AgentMutation::Access { secret })
        .await
        .unwrap();
    assert!(matches!(
        f.service.commit(&id).await,
        Err("access_not_revoked")
    ));
    legacy_revoked_entry(&f).await;
    assert!(matches!(
        f.service.prepare(deletion(&f.entry.id, 2)).await,
        Err("configuration_conflict")
    ));
    let AgentReply::Challenge { id, .. } =
        f.service.prepare(deletion(&f.entry.id, 4)).await.unwrap()
    else {
        panic!()
    };
    let mut settings = configuration(&f.service).await.settings;
    settings.show_tray = !settings.show_tray;
    change(&f.service, AgentMutation::Settings { settings })
        .await
        .unwrap();
    assert!(matches!(
        f.service.commit(&id).await,
        Err("authorization_expired")
    ));
    assert_eq!(configuration(&f.service).await.entries.len(), 1);
    let AgentReply::Challenge { id: first, .. } =
        f.service.prepare(deletion(&f.entry.id, 4)).await.unwrap()
    else {
        panic!()
    };
    let AgentReply::Challenge { id: second, .. } =
        f.service.prepare(deletion(&f.entry.id, 4)).await.unwrap()
    else {
        panic!()
    };
    f.service.commit(&first).await.unwrap();
    let revision = configuration(&f.service).await.settings.revision;
    f.service.commit(&second).await.unwrap();
    assert_eq!(configuration(&f.service).await.settings.revision, revision);
    f.service.shutdown().await;
}

#[tokio::test]
async fn access_deletion_frees_one_slot_while_the_32_entry_limit_still_applies() {
    removal_frees_one_slot(false).await;
}

#[tokio::test]
async fn access_revocation_frees_one_slot_while_the_32_entry_limit_still_applies() {
    removal_frees_one_slot(true).await;
}

async fn removal_frees_one_slot(active: bool) {
    let f = Fixture::new().await;
    if !active {
        legacy_revoked_entry(&f).await;
    }
    let mut config = configuration(&f.service).await;
    let expected = config.settings.revision;
    for number in 1..32 {
        config.entries.push(AccessSecret {
            entry: AccessEntry {
                id: format!("entry-{number}"),
                name: format!("Other {number}"),
                client: None,
                enabled: true,
                generation: 1,
            },
            digest: format!("{number:064x}"),
            sealed: "synthetic-token".into(),
        });
    }
    config.settings.revision += 1;
    f.service
        .db(AgentDbCommand::SaveConfiguration {
            configuration: config,
            expected,
        })
        .await
        .unwrap();
    let extra = || AccessSecret {
        entry: AccessEntry {
            id: "new-entry".into(),
            name: "New entry".into(),
            client: None,
            enabled: true,
            generation: 1,
        },
        digest: "c".repeat(64),
        sealed: "synthetic-token".into(),
    };
    assert!(matches!(
        f.service
            .prepare(AgentMutation::Access { secret: extra() })
            .await,
        Err("invalid_access_entry")
    ));
    if active {
        revoke(&f).await;
    } else {
        change(&f.service, deletion(&f.entry.id, 2)).await.unwrap();
    }
    assert_eq!(configuration(&f.service).await.entries.len(), 31);
    assert_eq!(
        f.service.authenticate(&format!("{:064x}", 1)).unwrap().id,
        "entry-1"
    );
    change(&f.service, AgentMutation::Access { secret: extra() })
        .await
        .unwrap();
    assert_eq!(configuration(&f.service).await.entries.len(), 32);
    let mut another = extra();
    another.entry.id = "one-too-many".into();
    assert!(matches!(
        f.service
            .prepare(AgentMutation::Access { secret: another })
            .await,
        Err("invalid_access_entry")
    ));
    f.service.shutdown().await;
}

#[tokio::test]
async fn access_deletion_waits_for_revocation_tasks_and_sessions_to_finish() {
    let f = Fixture::new().await;
    f.connection.hold.store(true, Ordering::Release);
    let task = f.start(command(), "still-running").await;
    f.running(&task.id).await;
    let mut config = configuration(&f.service).await;
    let expected = config.settings.revision;
    config.entries[0].entry.enabled = false;
    config.entries[0].entry.generation = 2;
    config.entries[0].digest.clear();
    config.entries[0].sealed.clear();
    config.settings.revision += 1;
    f.service
        .db(AgentDbCommand::SaveConfiguration {
            configuration: config,
            expected,
        })
        .await
        .unwrap();
    assert!(matches!(
        f.service.prepare(deletion(&f.entry.id, 2)).await,
        Err("access_busy")
    ));
    f.service
        .context(&f.entry.id, "session")
        .unwrap()
        .view
        .lock()
        .unwrap()
        .phase = ContextPhase::Closed;
    assert!(matches!(
        f.service.prepare(deletion(&f.entry.id, 2)).await,
        Err("access_busy")
    ));
    f.connection.hold.store(false, Ordering::Release);
    f.finished(&task.id).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.service.executor.active_for_entry(&f.entry.id) == 0
                && f.service.state.lock().unwrap().tasks.is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    change(&f.service, deletion(&f.entry.id, 2)).await.unwrap();
    f.service.shutdown().await;
}

struct RefuseConfigurationSave(Arc<Database>);
impl WorkspaceDatabase for RefuseConfigurationSave {
    fn execute(&self, command: DbCommand) -> FutureResult<'_, DbReply> {
        Box::pin(async move {
            if matches!(
                &command,
                DbCommand::Agent(AgentDbCommand::SaveConfiguration { .. })
            ) {
                return Err(WorkspaceError::StorageUnavailable);
            }
            self.0.execute(command).await
        })
    }
}

#[tokio::test]
async fn access_deletion_storage_failure_keeps_configuration_and_history_intact() {
    removal_storage_failure(false).await;
}

#[tokio::test]
async fn access_revocation_storage_failure_keeps_configuration_and_history_intact() {
    removal_storage_failure(true).await;
}

async fn removal_storage_failure(active: bool) {
    let f = Fixture::new().await;
    if !active {
        legacy_revoked_entry(&f).await;
    }
    f.service.close_context("session").await;
    let before = configuration(&f.service).await;
    let records = history(&f.database, &f.entry.id).await;
    let service = AgentService::new(
        Arc::new(RefuseConfigurationSave(f.database.clone())),
        Arc::new(Connector),
        Arc::new(Sink::default()),
        Arc::new(vigilshell_local_files::LocalFiles::new(vec![])),
    );
    service.initialize().await.unwrap();
    assert!(matches!(
        change(
            &service,
            if active {
                AgentMutation::Revoke {
                    id: f.entry.id.clone(),
                }
            } else {
                deletion(&f.entry.id, 2)
            }
        )
        .await,
        Err("agent_storage_unavailable")
    ));
    assert_eq!(
        serde_json::to_value(configuration(&service).await).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert_eq!(service.state.lock().unwrap().config.entries.len(), 1);
    assert_eq!(history(&f.database, &f.entry.id).await, records);
    service.shutdown().await;
    f.service.shutdown().await;
}

#[tokio::test]
async fn access_revocation_stops_running_tasks_and_closes_sessions() {
    let f = Fixture::new().await;
    f.connection.hold.store(true, Ordering::Release);
    let task = f.start(command(), "revoke-running").await;
    f.running(&task.id).await;
    let revision = configuration(&f.service).await.settings.revision;
    revoke(&f).await;
    assert!(configuration(&f.service).await.entries.is_empty());
    assert_eq!(
        configuration(&f.service).await.settings.revision,
        revision + 1
    );
    assert_eq!(
        f.service.authenticate(&"a".repeat(64)).unwrap_err(),
        "access_denied"
    );
    assert!(!f.finished(&task.id).await.phase.active());
    assert_eq!(f.service.executor.active_for_entry(&f.entry.id), 0);
    assert!(matches!(
        f.service.state.lock().unwrap().contexts["session"]
            .view
            .lock()
            .unwrap()
            .phase,
        ContextPhase::Closed | ContextPhase::Disconnected
    ));
    f.service.shutdown().await;
}

#[tokio::test]
async fn access_revocation_rejects_stale_confirmation_after_credential_rotation() {
    let f = Fixture::new().await;
    let AgentReply::Challenge { id, .. } = f
        .service
        .prepare(AgentMutation::Revoke {
            id: f.entry.id.clone(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let mut secret = configuration(&f.service).await.entries.remove(0);
    secret.entry.generation += 1;
    secret.digest = "b".repeat(64);
    secret.sealed = "rotated-token".into();
    change(&f.service, AgentMutation::Access { secret })
        .await
        .unwrap();
    assert!(matches!(
        f.service.commit(&id).await,
        Err("authorization_expired")
    ));
    assert_eq!(
        f.service.authenticate(&"b".repeat(64)).unwrap().id,
        f.entry.id
    );
    assert_eq!(configuration(&f.service).await.entries.len(), 1);
    f.service.shutdown().await;
}
