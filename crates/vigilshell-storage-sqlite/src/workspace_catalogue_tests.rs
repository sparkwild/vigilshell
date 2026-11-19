use super::*;
use std::sync::{Arc, Mutex};
use vigilshell_application_contracts::workspace::CredentialChange;

fn page(
    db: &mut SqlitePreferences,
    kind: &str,
    query: &str,
    offset: u32,
    version: Option<u64>,
) -> Result<Catalogue, WorkspaceError> {
    let DbReply::Catalogue(value) = db.execute(DbCommand::List {
        query: query.into(),
        offset,
        limit: 50,
        kind: kind.into(),
        version,
    })?
    else {
        panic!()
    };
    Ok(value)
}

#[test]
fn catalogue_keeps_paging_query_semantics_and_conflict_detection() {
    #[derive(Debug, Default)]
    struct CountingCipher(Mutex<Vec<String>>);
    impl ConnectionConfigCipher for CountingCipher {
        fn seal(&self, id: &str, bytes: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
            crate::test_support::Cipher.seal(id, bytes)
        }
        fn open(
            &self,
            id: &str,
            bytes: &[u8],
        ) -> Result<zeroize::Zeroizing<Vec<u8>>, WorkspaceError> {
            self.0.lock().unwrap().push(id.into());
            crate::test_support::Cipher.open(id, bytes)
        }
    }
    let mut db = SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
    crate::test_support::unlock(&mut db);
    let counting = Arc::new(CountingCipher::default());
    db.execute(DbCommand::WithConfigurationCipher {
        cipher: counting.clone(),
        command: Box::new(DbCommand::ConfigurationReady),
    })
    .unwrap();
    let take_opens = || std::mem::take(&mut *counting.0.lock().unwrap());
    let mut groups = Vec::new();
    for i in 0..55 {
        let DbReply::Group(group) = db
            .execute(DbCommand::CreateGroup(format!("group-{i}")))
            .unwrap()
        else {
            panic!()
        };
        groups.push(group);
    }
    let mut ids = Vec::new();
    for i in 0..125 {
        let id = new_id();
        db.execute(DbCommand::Save {
            id: id.clone(),
            expected_version: None,
            group_id: Some(groups[i % 55].id.clone()),
            input: ConnectionInput {
                name: if i == 15 {
                    "literal%_\\sample".into()
                } else {
                    format!("server-{i}")
                },
                host: format!("host-{i}.example"),
                port: 22,
                username: if i == 62 {
                    "target-user".into()
                } else {
                    "fixture".into()
                },
                auth: AuthKind::Password,
                notes: String::new(),
                terminal: Box::default(),
                proxy_id: None,
                tunnels: vec![],
                environment: Environment::Test,
            },
            credential: CredentialChange::Keep,
            trust: None,
        })
        .unwrap();
        ids.push(id);
    }
    take_opens();
    let first = page(&mut db, "connections", "", 0, None).unwrap();
    assert_eq!(first.connections.len(), 50);
    let assert_opens = |ids: &[String]| {
        let opened = take_opens();
        assert_eq!(opened[0], "connection-configuration-verifier");
        assert_eq!(&opened[1..], ids);
    };
    assert_opens(&ids[..50]);
    assert_eq!(first.next, Some(50));
    assert!(first.groups.is_empty());
    let second = page(&mut db, "connections", "", 50, Some(first.version)).unwrap();
    assert_eq!(second.connections.len(), 50);
    assert_eq!(second.next, Some(100));
    assert_opens(&ids[50..100]);
    let third = page(&mut db, "connections", "", 100, Some(first.version)).unwrap();
    assert_eq!(third.connections.len(), 25);
    assert_eq!(third.next, None);
    assert_opens(&ids[100..]);
    db.execute(DbCommand::Get(ids[62].clone())).unwrap();
    assert_opens(&ids[62..63]);
    let listed = first
        .connections
        .iter()
        .chain(&second.connections)
        .chain(&third.connections)
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(listed, ids);
    let group_page = page(&mut db, "groups", "", 0, Some(first.version)).unwrap();
    assert_eq!(group_page.groups.len(), 50);
    assert!(group_page.connections.is_empty());
    assert_eq!(
        page(&mut db, "groups", "", 50, Some(first.version))
            .unwrap()
            .groups
            .len(),
        5
    );
    for (query, expected) in [
        ("target-user", 62),
        ("host-62.example", 62),
        ("literal", 15),
        ("%_\\", 15),
    ] {
        let matches = page(&mut db, "connections", query, 0, Some(first.version)).unwrap();
        assert_eq!(matches.connections.len(), 1, "{query}");
        assert_eq!(matches.connections[0].id, ids[expected]);
    }
    for (query, expected_len) in [("fixture", 124), ("host-", 125)] {
        let mut offset = Some(0);
        let mut matches = Vec::new();
        while let Some(next) = offset {
            let result = page(&mut db, "connections", query, next, Some(first.version)).unwrap();
            offset = result.next;
            matches.extend(result.connections.into_iter().map(|p| p.id));
        }
        assert_eq!(matches.len(), expected_len);
        assert_eq!(
            matches
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            expected_len
        );
    }
    take_opens();
    let group = &groups[0];
    db.execute(DbCommand::UpdateGroup {
        id: group.id.clone(),
        version: group.version,
        name: "renamed".into(),
    })
    .unwrap();
    assert!(take_opens().is_empty());
    assert!(matches!(
        page(&mut db, "connections", "", 50, Some(first.version)),
        Err(WorkspaceError::Conflict)
    ));
}
