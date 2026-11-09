use crate::workspace::db;
use rusqlite::{Connection, params};
use std::collections::HashSet;

use vigilshell_domain::workspace::*;

pub(crate) fn layout_revision(c: &Connection) -> Result<u64, WorkspaceError> {
    db(
        c.query_row("SELECT revision FROM group_layout WHERE id=1", [], |r| {
            r.get::<_, i64>(0).map(|n| n as u64)
        }),
    )
}
pub(crate) fn bump_layout(c: &Connection) -> Result<u64, WorkspaceError> {
    let next = next_version(layout_revision(c)?)?;
    db(c.execute(
        "UPDATE group_layout SET revision=?1 WHERE id=1",
        [next as i64],
    ))?;
    Ok(next)
}
pub(crate) fn unique_group_name(c: &Connection, base: &str) -> Result<String, WorkspaceError> {
    let mut label = base.to_owned();
    let mut suffix = 2;
    while db(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM connection_groups WHERE name=?1)",
        [&label],
        |r| r.get::<_, bool>(0),
    ))? {
        let tail = format!(" ({suffix})");
        let mut prefix = base.to_owned();
        while prefix.len() + tail.len() > 128 {
            prefix.pop();
        }
        label = format!("{prefix}{tail}");
        suffix += 1;
    }
    Ok(label)
}
pub(crate) fn apply_layout(c: &Connection, layout: &GroupLayout) -> Result<u64, WorkspaceError> {
    if layout.revision != layout_revision(c)? {
        return Err(WorkspaceError::Conflict);
    }
    let read = |table: &str| -> Result<HashSet<String>, WorkspaceError> {
        db(db(c.prepare(&format!("SELECT id FROM {table}")))?.query_map([], |r| r.get(0)))?
            .collect::<rusqlite::Result<_>>()
            .map_err(|_| WorkspaceError::StorageUnavailable)
    };
    let groups = read("connection_groups")?;
    let connections = read("connection_profiles")?;
    let proposed: HashSet<_> = layout.group_ids.iter().cloned().collect();
    if proposed != groups
        || proposed.len() != layout.group_ids.len()
        || layout.buckets.len() != groups.len() + 1
    {
        return Err(WorkspaceError::InvalidInput);
    }
    let mut buckets = HashSet::new();
    let mut members = HashSet::new();
    for bucket in &layout.buckets {
        if !buckets.insert(bucket.group_id.clone())
            || bucket
                .group_id
                .as_ref()
                .is_some_and(|id| !groups.contains(id))
        {
            return Err(WorkspaceError::InvalidInput);
        }
        for id in &bucket.connection_ids {
            if !connections.contains(id) || !members.insert(id.clone()) {
                return Err(WorkspaceError::InvalidInput);
            }
        }
    }
    if members != connections || !buckets.contains(&None) {
        return Err(WorkspaceError::InvalidInput);
    }
    for (i, id) in layout.group_ids.iter().enumerate() {
        db(c.execute(
            "UPDATE connection_groups SET sort_order=?1 WHERE id=?2",
            params![i as i64, id],
        ))?;
    }
    for bucket in &layout.buckets {
        for (i, id) in bucket.connection_ids.iter().enumerate() {
            db(c.execute(
                "UPDATE connection_profiles SET group_id=?1,group_sort_order=?2 WHERE id=?3",
                params![bucket.group_id, i as i64, id],
            ))?;
        }
    }
    bump_layout(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SqlitePreferences, workspace::get};
    use vigilshell_application_contracts::ports::{DbCommand, DbReply, WorkspaceRepository};
    use vigilshell_application_contracts::workspace::CredentialChange;
    fn create(
        store: &mut SqlitePreferences,
        group_id: Option<String>,
        label: String,
    ) -> ConnectionRecord {
        let DbReply::Connection(c) = store
            .execute(DbCommand::Save {
                id: new_id(),
                expected_version: None,
                group_id,
                input: ConnectionInput {
                    notes: String::new(),
                    terminal: Box::default(),
                    proxy_id: None,
                    tunnels: Vec::new(),
                    environment: Environment::None,
                    name: label,
                    host: "localhost".into(),
                    port: 22,
                    username: "fixture".into(),
                    auth: AuthKind::Password,
                },
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        c
    }
    fn group(store: &mut SqlitePreferences, name: &str) -> ConnectionGroup {
        let DbReply::Group(g) = store.execute(DbCommand::CreateGroup(name.into())).unwrap() else {
            panic!()
        };
        g
    }
    #[test]
    fn complete_layout_is_atomic_versioned_and_preserves_drafts_across_75_connections() {
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::test_support::open(root.path()).unwrap();
        let a = group(&mut store, "A");
        let b = group(&mut store, "B");
        let records: Vec<_> = (0..75)
            .map(|i| create(&mut store, Some(a.id.clone()), format!("connection-{i}")))
            .collect();
        let mut layout = GroupLayout {
            revision: layout_revision(&store.connection).unwrap(),
            group_ids: vec![b.id.clone(), a.id.clone()],
            buckets: vec![
                GroupBucket {
                    group_id: Some(a.id.clone()),
                    connection_ids: records.iter().skip(2).map(|c| c.id.clone()).collect(),
                },
                GroupBucket {
                    group_id: Some(b.id.clone()),
                    connection_ids: vec![records[1].id.clone()],
                },
                GroupBucket {
                    group_id: None,
                    connection_ids: vec![records[0].id.clone()],
                },
            ],
        };
        let mut invalid = layout.clone();
        invalid.buckets[0]
            .connection_ids
            .push(records[0].id.clone());
        assert_eq!(
            store.execute(DbCommand::UpdateLayout(invalid)).unwrap_err(),
            WorkspaceError::InvalidInput
        );
        assert_eq!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &records[0].id
            )
            .unwrap()
            .group_id,
            Some(a.id.clone())
        );
        store
            .execute(DbCommand::UpdateLayout(layout.clone()))
            .unwrap();
        assert_eq!(
            store
                .execute(DbCommand::UpdateLayout(layout.clone()))
                .unwrap_err(),
            WorkspaceError::Conflict
        );
        let before = &records[1];
        let mut input = before.input.clone();
        input.name = "unsaved draft".into();
        input.environment = Environment::Production;
        let DbReply::Connection(saved) = store
            .execute(DbCommand::Save {
                id: before.id.clone(),
                expected_version: Some(before.version),
                group_id: Some(a.id.clone()),
                input,
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(saved.group_id, Some(b.id.clone()));
        assert_eq!(saved.revision, before.revision);
        assert_eq!(saved.created_at, before.created_at);
        store
            .execute(DbCommand::DeleteGroup {
                id: b.id.clone(),
                version: b.version,
            })
            .unwrap();
        let c = get(
            &store.connection,
            store.configuration_cipher.as_deref().unwrap(),
            &before.id,
        )
        .unwrap();
        assert!(c.group_id.is_none());
        assert_eq!(c.group_sort_order, 1);
        assert_eq!(c.input.environment, Environment::Production);
        let first = store
            .execute(DbCommand::List {
                query: "".into(),
                offset: 0,
                limit: 50,
                kind: "connections".into(),
                version: None,
            })
            .unwrap();
        let DbReply::Catalogue(page) = first else {
            panic!()
        };
        assert_eq!(page.connections.len(), 50);
        assert_eq!(page.next, Some(50));
        let DbReply::Catalogue(next) = store
            .execute(DbCommand::List {
                query: "".into(),
                offset: 50,
                limit: 50,
                kind: "connections".into(),
                version: Some(page.version),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(next.connections.len(), 25);
        assert!(next.next.is_none());
        layout.revision = layout_revision(&store.connection).unwrap();
        assert_eq!(
            store.execute(DbCommand::UpdateLayout(layout)).unwrap_err(),
            WorkspaceError::InvalidInput
        );
        drop(store);
        let store = crate::test_support::open(root.path()).unwrap();
        assert_eq!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &before.id
            )
            .unwrap()
            .group_sort_order,
            1
        );
    }
    #[test]
    fn group_names_are_disambiguated_and_rename_does_not_reorder() {
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::test_support::open(root.path()).unwrap();
        let a = group(&mut store, "新分组");
        let b = group(&mut store, "新分组");
        assert_eq!(b.name, "新分组 (2)");
        assert_eq!(
            store
                .execute(DbCommand::UpdateGroup {
                    id: a.id.clone(),
                    version: a.version,
                    name: b.name.clone()
                })
                .unwrap_err(),
            WorkspaceError::DuplicateName
        );
        let DbReply::Group(renamed) = store
            .execute(DbCommand::UpdateGroup {
                id: a.id.clone(),
                version: a.version,
                name: "renamed".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(renamed.sort_order, 1);
    }
}
