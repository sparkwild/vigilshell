use super::*;
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
    let mut db = SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
    crate::test_support::unlock(&mut db);
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
    for i in 0..75 {
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
    let first = page(&mut db, "connections", "", 0, None).unwrap();
    assert_eq!(first.connections.len(), 50);
    assert_eq!(first.next, Some(50));
    assert!(first.groups.is_empty());
    let second = page(&mut db, "connections", "", 50, Some(first.version)).unwrap();
    assert_eq!(second.connections.len(), 25);
    assert_eq!(second.next, None);
    let listed = first
        .connections
        .iter()
        .chain(&second.connections)
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
    for (query, expected) in [("target-user", 62), ("host-62.example", 62), ("%_\\", 15)] {
        let matches = page(&mut db, "connections", query, 0, Some(first.version)).unwrap();
        assert_eq!(matches.connections.len(), 1, "{query}");
        assert_eq!(matches.connections[0].id, ids[expected]);
    }
    let group = &groups[0];
    db.execute(DbCommand::UpdateGroup {
        id: group.id.clone(),
        version: group.version,
        name: "renamed".into(),
    })
    .unwrap();
    assert!(matches!(
        page(&mut db, "connections", "", 50, Some(first.version)),
        Err(WorkspaceError::Conflict)
    ));
}
