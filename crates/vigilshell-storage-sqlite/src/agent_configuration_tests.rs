use super::*;
use vigilshell_application_contracts::ports::{AgentDbCommand, DbCommand, WorkspaceRepository};
use vigilshell_domain::workspace::WorkspaceError;

#[test]
fn malformed_optional_configuration_preserves_manual_workspace_and_original_body() {
    for body in [
        "{not valid json".to_owned(),
        r#"{"settings":{"enabled":true},"entries":[],"policies":[]}"#.to_owned(),
        r#"{"settings":{"revision":4,"enabled":false,"port":51602,"showTray":true,"retentionDays":30,"storageMib":1024},"entries":[],"policies":[]}"#.to_owned(),
    ] {
        let root = tempfile::tempdir().unwrap();
        let repository = crate::test_support::open(root.path()).unwrap();
        crate::connection_configuration::write_profile(&repository.connection, &crate::test_support::Cipher, &vigilshell_domain::workspace::ConnectionRecord {
            id: "00000000-0000-4000-8000-000000000001".into(), group_id: None, group_sort_order: 0, created_at: None,
            input: vigilshell_domain::workspace::ConnectionInput {
                name: "synthetic".into(), host: "fixture.invalid".into(), port: 22, username: "tester".into(),
                auth: vigilshell_domain::workspace::AuthKind::Password, notes: String::new(), terminal: Box::default(), proxy_id: None,
                tunnels: vec![], environment: vigilshell_domain::workspace::Environment::None,
            }, version: 1, revision: 1, credential: None, credential_label: None, deleting: false, credential_pending: false,
        }, false).unwrap();
        repository.connection.execute("UPDATE agent_configuration SET body=?1", [&body]).unwrap();
        drop(repository);
        let mut repository = crate::test_support::open(root.path()).unwrap();
        assert_eq!(repository.get().unwrap(), Preferences::default());
        assert!(matches!(repository.execute(DbCommand::Get("00000000-0000-4000-8000-000000000001".into())), Ok(vigilshell_application_contracts::ports::DbReply::Connection(_))));
        assert!(matches!(
            repository.execute(DbCommand::Agent(AgentDbCommand::Configuration)),
            Err(WorkspaceError::AgentConfigurationUnavailable)
        ));
        assert!(matches!(
            repository.execute(DbCommand::Agent(AgentDbCommand::SaveConfiguration {
                configuration: vigilshell_domain::agent::AgentConfiguration::default(),
                expected: 0,
            })),
            Err(WorkspaceError::AgentConfigurationUnavailable)
        ));
        let job = vigilshell_domain::workspace::new_id();
        repository.execute(DbCommand::BeginDeletion {id:"00000000-0000-4000-8000-000000000001".into(), version:1, job:job.clone()}).unwrap();
        repository.execute(DbCommand::Delete("00000000-0000-4000-8000-000000000001".into())).unwrap();
        repository.execute(DbCommand::FinishCredential(job)).unwrap();
        assert!(matches!(repository.execute(DbCommand::Get("00000000-0000-4000-8000-000000000001".into())), Err(WorkspaceError::NotFound)));
        let unchanged: String = repository.connection.query_row("SELECT body FROM agent_configuration WHERE id=1", [], |r|r.get(0)).unwrap();
        assert_eq!(unchanged, body);
        assert_eq!(repository.connection.query_row("SELECT revision FROM agent_configuration WHERE id=1", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
    }
}

#[test]
fn optional_configuration_isolation_does_not_relax_vault_or_storage_structure() {
    for change in [
        "DELETE FROM credential_vault",
        "DELETE FROM agent_configuration",
        "PRAGMA application_id=42",
        "CREATE TABLE unexpected(value TEXT)",
    ] {
        let root = tempfile::tempdir().unwrap();
        let repository = SqlitePreferences::open(root.path()).unwrap();
        repository
            .connection
            .execute("UPDATE agent_configuration SET body='broken'", [])
            .unwrap();
        repository.connection.execute_batch(change).unwrap();
        drop(repository);
        let before = fs::read(root.path().join(DATABASE_FILENAME)).unwrap();
        assert!(SqlitePreferences::open(root.path()).is_err(), "{change}");
        assert_eq!(
            fs::read(root.path().join(DATABASE_FILENAME)).unwrap(),
            before
        );
    }
}
