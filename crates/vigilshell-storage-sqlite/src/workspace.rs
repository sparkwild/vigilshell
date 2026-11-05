use crate::group_layout::{apply_layout, bump_layout, layout_revision, unique_group_name};
// 工作区 Repository 只在持有 SQLite 连接的线程执行。
use super::SqlitePreferences;
use rusqlite::{Connection, OptionalExtension, params};

use vigilshell_application_contracts::ports::{
    CredentialJob, DbCommand, DbReply, WorkspaceRepository,
};

use vigilshell_application_contracts::workspace::CredentialChange;
use vigilshell_domain::workspace::*;

pub(crate) fn db<T>(value: rusqlite::Result<T>) -> Result<T, WorkspaceError> {
    value.map_err(|_| WorkspaceError::StorageUnavailable)
}
use crate::connection_configuration::{read_profile, write_profile};
use vigilshell_application_contracts::ports::ConnectionConfigCipher;
pub(crate) fn get(
    connection: &Connection,
    cipher: &dyn ConnectionConfigCipher,
    id: &str,
) -> Result<ConnectionRecord, WorkspaceError> {
    read_profile(connection, cipher, id)
}
fn group(connection: &Connection, id: &str) -> Result<ConnectionGroup, WorkspaceError> {
    db(connection
        .query_row(
            "SELECT id,name,version,sort_order FROM connection_groups WHERE id=?1",
            [id],
            |r| {
                Ok(ConnectionGroup {
                    sort_order: r.get::<_, i64>(3)? as u64,
                    id: r.get(0)?,
                    name: r.get(1)?,
                    version: r.get::<_, i64>(2)? as u64,
                })
            },
        )
        .optional())?
    .ok_or(WorkspaceError::NotFound)
}
fn check_version(actual: u64, expected: u64) -> Result<(), WorkspaceError> {
    if actual == expected {
        Ok(())
    } else {
        Err(WorkspaceError::Conflict)
    }
}
fn check_profile(p: &ConnectionRecord, expected: u64) -> Result<(), WorkspaceError> {
    check_version(p.version, expected)?;
    if p.deleting {
        Err(WorkspaceError::Busy)
    } else {
        Ok(())
    }
}
fn set_trust(c: &Connection, p: &ConnectionRecord, key: &HostKey) -> Result<(), WorkspaceError> {
    db(c.execute("INSERT INTO host_trusts VALUES (?1,?2,?3,?4,?5,?6,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(connection_id) DO UPDATE SET host=excluded.host,port=excluded.port,public_key=excluded.public_key,fingerprint=excluded.fingerprint,algorithm=excluded.algorithm,confirmed_at=excluded.confirmed_at",params![p.id,p.input.host,p.input.port,key.public_key,key.fingerprint,key.algorithm]))?;
    Ok(())
}
fn trust(c: &Connection, id: &str) -> Result<Option<HostTrust>, WorkspaceError> {
    db(c.query_row("SELECT connection_id,host,port,public_key,fingerprint,algorithm,confirmed_at FROM host_trusts WHERE connection_id=?1",[id],|r| Ok(HostTrust { connection_id:r.get(0)?,host:r.get(1)?,port:r.get(2)?,key:HostKey {public_key:r.get(3)?,fingerprint:r.get(4)?,algorithm:r.get(5)?},confirmed_at:r.get(6)? })).optional())
}

impl WorkspaceRepository for SqlitePreferences {
    fn execute(&mut self, command: DbCommand) -> Result<DbReply, WorkspaceError> {
        if let DbCommand::WithConfigurationCipher { cipher, command } = command {
            self.configuration_cipher = Some(cipher);
            return self.execute(*command);
        }
        let cipher = self.configuration_cipher.as_deref();
        if command.needs_connection_configuration() {
            crate::connection_configuration::verify(
                &self.connection,
                cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
            )?;
        }
        if matches!(command, DbCommand::ConfigurationReady) {
            return Ok(DbReply::Done);
        }
        if let Some(result) = crate::vault::execute(&mut self.connection, &command) {
            return result;
        }
        match command {
            DbCommand::Execution(command) => {
                return crate::execution_tasks::execute(&mut self.connection, command)
                    .map(DbReply::Execution);
            }
            DbCommand::AgentHistory(command) => {
                return crate::agent_history::execute(
                    &self.connection,
                    self.data_root.as_deref(),
                    command,
                )
                .map(DbReply::AgentHistory);
            }
            DbCommand::Agent(
                vigilshell_application_contracts::ports::AgentDbCommand::Recording(command),
            ) => {
                if self.recordings.is_none() {
                    let root = self
                        .data_root
                        .as_ref()
                        .ok_or(WorkspaceError::StorageUnavailable)?;
                    self.recordings = Some(
                        crate::recordings::RecordingStore::open(root)
                            .map_err(|_| WorkspaceError::StorageUnavailable)?,
                    );
                }
                return crate::recordings::execute(
                    &mut self.connection,
                    self.recordings.as_mut().unwrap(),
                    *command,
                )
                .map(DbReply::Agent);
            }
            DbCommand::Agent(command) => {
                return crate::agent::execute(&mut self.connection, command).map(DbReply::Agent);
            }
            DbCommand::VaultMetadata
            | DbCommand::InitializeVault { .. }
            | DbCommand::ReadEncryptedCredential(_)
            | DbCommand::WriteEncryptedCredential(_)
            | DbCommand::RemoveEncryptedCredential(_) => unreachable!(),
            DbCommand::Preferences => {
                return super::read(&self.connection)
                    .map(DbReply::Preferences)
                    .map_err(|_| WorkspaceError::StorageUnavailable);
            }
            DbCommand::ListProxies { offset, version } => {
                let version = page_version(&self.connection, version)?;
                let mut proxies = proxy_list(&self.connection, offset)?;
                let next = page_next(&mut proxies, offset);
                return Ok(DbReply::Proxies {
                    proxies,
                    version,
                    next,
                });
            }
            DbCommand::GetProxy(id) => return proxy_get(&self.connection, &id).map(DbReply::Proxy),
            DbCommand::ProxyReferences {
                id,
                offset,
                version,
            } => {
                let version = page_version(&self.connection, version)?;
                let mut names = proxy_references(
                    &self.connection,
                    cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                    &id,
                    offset,
                )?;
                let next = page_next(&mut names, offset);
                return Ok(DbReply::References {
                    names,
                    version,
                    next,
                });
            }
            DbCommand::Namespace => {
                return db(self.connection.query_row(
                    "SELECT namespace FROM workspace_meta WHERE id=1",
                    [],
                    |r| r.get(0),
                ))
                .map(DbReply::Namespace);
            }
            DbCommand::Get(id) => {
                return get(
                    &self.connection,
                    cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                    &id,
                )
                .map(DbReply::Connection);
            }
            DbCommand::Trust(id) => return trust(&self.connection, &id).map(DbReply::Trust),
            DbCommand::List {
                query,
                offset,
                limit,
                kind,
                version,
            } => {
                return list(
                    &self.connection,
                    cipher,
                    &query,
                    offset,
                    limit,
                    &kind,
                    version,
                )
                .map(DbReply::Catalogue);
            }
            DbCommand::CredentialJobs => {
                let mut s=db(self.connection.prepare("SELECT id,connection_id,new_ref,old_ref,action FROM credential_jobs ORDER BY rowid"))?;
                let rows = db(s.query_map([], |r| {
                    Ok(CredentialJob {
                        id: r.get(0)?,
                        connection_id: r.get(1)?,
                        new_ref: r.get(2)?,
                        old_ref: r.get(3)?,
                        action: r.get(4)?,
                    })
                }))?;
                return db(rows.collect()).map(DbReply::Jobs);
            }
            _ => {}
        }
        let tx = db(self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))?;
        let mut changed = true;
        let reply = match command {
            DbCommand::SaveProxy {
                mut proxy,
                expected_version,
            } => {
                proxy.validate()?;
                if db(tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM proxy_profiles WHERE name=?1 AND id<>?2)",
                    params![proxy.name, proxy.id],
                    |r| r.get::<_, bool>(0),
                ))? {
                    return Err(WorkspaceError::DuplicateName);
                }
                let old = if let Some(version) = expected_version {
                    let old = proxy_get(&tx, &proxy.id)?;
                    check_version(old.version, version)?;
                    Some(old)
                } else {
                    None
                };
                if old.is_none()
                    && db(
                        tx.query_row("SELECT COUNT(*) FROM proxy_profiles", [], |r| {
                            r.get::<_, i64>(0)
                        }),
                    )? >= 256
                {
                    return Err(WorkspaceError::Busy);
                }
                let route_changed = old.as_ref().is_some_and(|old| {
                    old.kind != proxy.kind
                        || old.host != proxy.host
                        || old.port != proxy.port
                        || old.username != proxy.username
                        || old.credential != proxy.credential
                });
                changed = old.as_ref() != Some(&proxy);
                if changed {
                    proxy.version = if let Some(version) = expected_version {
                        next_version(version)?
                    } else {
                        1
                    };
                    if expected_version.is_some() {
                        db(tx.execute("UPDATE proxy_profiles SET name=?2,kind=?3,host=?4,port=?5,username=?6,credential_ref=?7,version=?8 WHERE id=?1", params![proxy.id,proxy.name,proxy.kind,proxy.host,proxy.port,proxy.username,proxy.credential,proxy.version as i64]))?;
                    } else {
                        db(tx.execute(
                            "INSERT INTO proxy_profiles VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                            params![
                                proxy.id,
                                proxy.name,
                                proxy.kind,
                                proxy.host,
                                proxy.port,
                                proxy.username,
                                proxy.credential,
                                proxy.version as i64
                            ],
                        ))?;
                    }
                    if route_changed {
                        db(tx.execute(
                            "UPDATE connection_profiles SET revision=revision+1 WHERE proxy_id=?1",
                            [&proxy.id],
                        ))?;
                    }
                }
                DbReply::Proxy(proxy)
            }
            DbCommand::DeleteProxy { id, version } => {
                check_version(proxy_get(&tx, &id)?.version, version)?;
                if !proxy_references(
                    &tx,
                    cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                    &id,
                    0,
                )?
                .is_empty()
                {
                    return Err(WorkspaceError::ProxyInUse);
                }
                db(tx.execute("DELETE FROM proxy_profiles WHERE id=?1", [id]))?;
                DbReply::Done
            }
            DbCommand::CreateGroup(value) => {
                let base = name(&value)?;
                let label = unique_group_name(&tx, &base)?;
                let p = ConnectionGroup {
                    id: new_id(),
                    name: label,
                    version: 1,
                    sort_order: 0,
                };
                db(tx.execute("UPDATE connection_groups SET sort_order=sort_order+1", []))?;
                db(tx.execute(
                    "INSERT INTO connection_groups VALUES(?1,?2,0,1)",
                    params![p.id, p.name],
                ))?;
                bump_layout(&tx)?;
                DbReply::Group(p)
            }
            DbCommand::UpdateGroup {
                id,
                version,
                name: value,
            } => {
                let mut p = group(&tx, &id)?;
                check_version(p.version, version)?;
                let value = name(&value)?;
                changed = p.name != value;
                if changed {
                    if db(tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM connection_groups WHERE name=?1 AND id<>?2)",
                        params![value, id],
                        |r| r.get::<_, bool>(0),
                    ))? {
                        return Err(WorkspaceError::DuplicateName);
                    }
                    p.version = next_version(p.version)?;
                    p.name = value;
                    db(tx.execute(
                        "UPDATE connection_groups SET name=?1,version=?2 WHERE id=?3",
                        params![p.name, p.version as i64, id],
                    ))?;
                }
                DbReply::Group(p)
            }
            DbCommand::DeleteGroup { id, version } => {
                check_version(group(&tx, &id)?.version, version)?;
                let start: i64 = db(tx.query_row("SELECT COALESCE(MAX(group_sort_order)+1,0) FROM connection_profiles WHERE group_id IS NULL",[],|r|r.get(0)))?;
                let ids:Vec<String> = db(db(tx.prepare("SELECT id FROM connection_profiles WHERE group_id=?1 ORDER BY group_sort_order,id"))?.query_map([&id],|r|r.get(0)))?.collect::<rusqlite::Result<_>>().map_err(|_|WorkspaceError::StorageUnavailable)?;
                for (i, cid) in ids.iter().enumerate() {
                    db(tx.execute("UPDATE connection_profiles SET group_id=NULL,group_sort_order=?1 WHERE id=?2",params![start+i as i64,cid]))?;
                }
                db(tx.execute("DELETE FROM connection_groups WHERE id=?1", [id]))?;
                bump_layout(&tx)?;
                DbReply::Done
            }
            DbCommand::UpdateLayout(layout) => {
                let revision = apply_layout(&tx, &layout)?;
                DbReply::LayoutRevision(revision)
            }
            DbCommand::Save {
                id,
                expected_version,
                group_id,
                input,
                credential,
                trust: approved,
            } => {
                let input = input.normalized()?;
                let mut p = if let Some(version) = expected_version {
                    let mut p = get(&tx, cipher.ok_or(WorkspaceError::ConfigurationLocked)?, &id)?;
                    check_profile(&p, version)?;
                    if input.auth != p.input.auth && matches!(credential, CredentialChange::Keep) {
                        return Err(WorkspaceError::CredentialMissing);
                    }
                    changed = input != p.input
                        || !matches!(credential, CredentialChange::Keep)
                        || approved.is_some();
                    if changed {
                        p.version = next_version(p.version)?;
                        if !p.input.same_auth_target(&input)
                            || !matches!(credential, CredentialChange::Keep)
                        {
                            p.revision = next_version(p.revision)?;
                        }
                        if !p.input.same_endpoint(&input) {
                            db(tx.execute("DELETE FROM host_trusts WHERE connection_id=?1", [&id]))?;
                        }
                        p.input = input;
                    }
                    p
                } else {
                    if !valid_id(&id) {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    if let Some(gid) = &group_id {
                        group(&tx, gid)?;
                    }
                    db(tx.execute("UPDATE connection_profiles SET group_sort_order=group_sort_order+1 WHERE group_id IS ?1",[&group_id]))?;
                    bump_layout(&tx)?;
                    ConnectionRecord {
                        id: id.clone(),
                        group_id,
                        group_sort_order: 0,
                        created_at: Some(db(tx.query_row(
                            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                            [],
                            |r| r.get(0),
                        ))?),
                        input,
                        version: 1,
                        revision: 1,
                        credential: None,
                        credential_label: None,
                        deleting: false,
                        credential_pending: false,
                    }
                };
                match credential {
                    CredentialChange::Keep => {}
                    CredentialChange::Replace { reference, label } => {
                        p.credential = Some(reference);
                        p.credential_label = Some(label);
                    }
                    CredentialChange::Remove => {
                        p.credential = None;
                        p.credential_label = None;
                    }
                }
                if changed {
                    write_profile(
                        &tx,
                        cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                        &p,
                        expected_version.is_some(),
                    )?;
                    if let Some(key) = approved {
                        set_trust(&tx, &p, &key)?;
                    }
                }
                DbReply::Connection(p)
            }
            DbCommand::SetTrust { id, revision, key } => {
                let p = get(&tx, cipher.ok_or(WorkspaceError::ConfigurationLocked)?, &id)?;
                if p.revision != revision {
                    return Err(WorkspaceError::Conflict);
                }
                if p.deleting {
                    return Err(WorkspaceError::Busy);
                }
                changed = trust(&tx, &id)?.is_none_or(|t| t.key != key);
                if changed {
                    set_trust(&tx, &p, &key)?;
                }
                DbReply::Connection(p)
            }
            DbCommand::ForgetTrust { id, version } => {
                let p = get(&tx, cipher.ok_or(WorkspaceError::ConfigurationLocked)?, &id)?;
                check_profile(&p, version)?;
                changed =
                    db(tx.execute("DELETE FROM host_trusts WHERE connection_id=?1", [&id]))? > 0;
                if changed {
                    db(tx.execute(
                        "UPDATE connection_profiles SET version=?1,revision=?2 WHERE id=?3",
                        params![
                            next_version(p.version)? as i64,
                            next_version(p.revision)? as i64,
                            id
                        ],
                    ))?;
                }
                DbReply::Done
            }
            DbCommand::PrepareCredential(job) => {
                changed = false;
                db(tx.execute(
                    "INSERT INTO credential_jobs VALUES (?1,?2,?3,?4,?5)",
                    params![
                        job.id,
                        job.connection_id,
                        job.new_ref,
                        job.old_ref,
                        job.action
                    ],
                ))?;
                DbReply::Done
            }
            DbCommand::FinishCredential(id) => {
                changed = false;
                db(tx.execute("DELETE FROM credential_jobs WHERE id=?1", [id]))?;
                DbReply::Done
            }
            DbCommand::BeginDeletion { id, version, job } => {
                let mut p = get(&tx, cipher.ok_or(WorkspaceError::ConfigurationLocked)?, &id)?;
                check_version(p.version, version)?;
                db(tx.execute(
                    "INSERT INTO credential_jobs VALUES (?1,?2,NULL,?3,?4)",
                    params![job, id, p.credential, "delete"],
                ))?;
                p.deleting = true;
                db(tx.execute(
                    "UPDATE connection_profiles SET deleting=1 WHERE id=?1",
                    [id],
                ))?;
                DbReply::Connection(p)
            }
            DbCommand::Delete(id) => {
                let p = get(&tx, cipher.ok_or(WorkspaceError::ConfigurationLocked)?, &id)?;
                if !p.deleting {
                    return Err(WorkspaceError::Conflict);
                }
                crate::agent::remove_connection_policy(&tx, &id)?;
                db(tx.execute(
                    "DELETE FROM ui_state WHERE scope=?1",
                    [format!("connection:{id}")],
                ))?;
                db(tx.execute("DELETE FROM connection_profiles WHERE id=?1", [id]))?;
                bump_layout(&tx)?;
                DbReply::Done
            }
            _ => unreachable!(),
        };
        if changed {
            let revision = db(tx.query_row(
                "SELECT revision FROM workspace_meta WHERE id=1",
                [],
                |r| r.get::<_, i64>(0),
            ))? as u64;
            db(tx.execute(
                "UPDATE workspace_meta SET revision=?1 WHERE id=1",
                [next_version(revision)? as i64],
            ))?;
        }
        db(tx.commit())?;
        Ok(reply)
    }
}

fn list(
    c: &Connection,
    cipher: Option<&dyn ConnectionConfigCipher>,
    query: &str,
    offset: u32,
    limit: u32,
    kind: &str,
    expected: Option<u64>,
) -> Result<Catalogue, WorkspaceError> {
    if limit == 0 || limit > 50 || query.len() > 256 {
        return Err(WorkspaceError::InvalidInput);
    }
    let revision = db(
        c.query_row("SELECT revision FROM workspace_meta WHERE id=1", [], |r| {
            r.get::<_, i64>(0)
        }),
    )? as u64;
    if expected.is_some_and(|v| v != revision) {
        return Err(WorkspaceError::Conflict);
    }
    let pattern = format!(
        "%{}%",
        query
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    if kind == "connections" {
        let mut result = Catalogue {
            version: revision,
            layout_revision: layout_revision(c)?,
            ..Default::default()
        };
        let mut query = db(c.prepare("SELECT id FROM connection_profiles ORDER BY rowid"))?;
        let mut rows = db(query.query([]))?;
        let mut matched = 0u32;
        while let Some(row) = db(rows.next())? {
            let p = get(
                c,
                cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                &db(row.get::<_, String>(0))?,
            )?;
            let matches: bool = db(c.query_row(
                "SELECT ?1 LIKE ?4 ESCAPE '\\' OR ?2 LIKE ?4 ESCAPE '\\' OR ?3 LIKE ?4 ESCAPE '\\'",
                params![p.input.name, p.input.host, p.input.username, pattern],
                |r| r.get(0),
            ))?;
            if !matches {
                continue;
            }
            if matched >= offset {
                if result.connections.len() == limit as usize {
                    result.next = Some(offset.saturating_add(limit));
                    break;
                }
                result.connections.push(p);
            }
            matched = matched.saturating_add(1);
        }
        return Ok(result);
    }
    let sql = match kind {
        "groups" => {
            "SELECT 'group',id FROM connection_groups WHERE name LIKE ?1 ESCAPE '\\' ORDER BY sort_order,id LIMIT ?2 OFFSET ?3"
        }
        _ => return Err(WorkspaceError::InvalidInput),
    };
    let mut q = db(c.prepare(sql))?;
    let mut ids = db(db(q.query_map(params![pattern, limit + 1, offset], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    let next = if ids.len() > limit as usize {
        ids.pop();
        Some(offset + limit)
    } else {
        None
    };
    let mut result = Catalogue {
        version: revision,
        layout_revision: layout_revision(c)?,
        next,
        ..Default::default()
    };
    for (kind, id) in ids {
        if kind == "group" {
            result.groups.push(group(c, &id)?);
        } else {
            result.connections.push(get(
                c,
                cipher.ok_or(WorkspaceError::ConfigurationLocked)?,
                &id,
            )?);
        }
    }
    Ok(result)
}

pub(crate) fn validate_rows(c: &Connection) -> rusqlite::Result<bool> {
    let count: i64 = c.query_row("SELECT COUNT(*) FROM workspace_meta", [], |r| r.get(0))?;
    if count != 1 {
        return Ok(false);
    }
    let namespace: String =
        c.query_row("SELECT namespace FROM workspace_meta WHERE id=1", [], |r| {
            r.get(0)
        })?;
    if namespace.len() != 32
        || !namespace
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Ok(false);
    }
    let mut q = c.prepare("SELECT id,name FROM connection_groups")?;
    let mut rows = q.query([])?;
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        let label: String = r.get(1)?;
        if !valid_id(&id) || name(&label).as_deref() != Ok(label.as_str()) {
            return Ok(false);
        }
    }
    let mut q = c.prepare("SELECT * FROM proxy_profiles")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        let proxy = proxy_row(row)?;
        if proxy.validate().is_err() || proxy.credential.as_ref().is_some_and(|id| !valid_id(id)) {
            return Ok(false);
        }
    }
    let mut q = c.prepare("SELECT id,connection_id,new_ref,old_ref FROM credential_jobs")?;
    let mut rows = q.query([])?;
    while let Some(r) = rows.next()? {
        for index in 0..4 {
            let value: Option<String> = r.get(index)?;
            if value.as_ref().is_some_and(|v| !valid_id(v)) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> ConnectionInput {
        ConnectionInput {
            notes: String::new(),
            terminal: Box::default(),
            proxy_id: None,
            tunnels: Vec::new(),
            environment: Environment::None,
            name: "one".into(),
            host: "localhost".into(),
            port: 22,
            username: "test".into(),
            auth: AuthKind::Password,
        }
    }
    #[test]
    fn deleting_connection_removes_only_its_policy_atomically_and_invalidates_stale_saves() {
        use vigilshell_application_contracts::ports::{AgentDbCommand as A, AgentDbReply as R};
        use vigilshell_domain::agent::*;
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::test_support::open(root.path()).unwrap();
        let id = new_id();
        store
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: None,
                group_id: None,
                input: input(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap();
        let R::Configuration(mut config) =
            crate::agent::execute(&mut store.connection, A::Configuration).unwrap()
        else {
            panic!()
        };
        config.settings.revision = 1;
        config.entries.push(AccessSecret {
            entry: AccessEntry {
                client: None,
                id: "entry".into(),
                name: "kept".into(),
                enabled: false,
                generation: 7,
            },
            digest: "synthetic-digest".into(),
            sealed: "synthetic-sealed".into(),
        });
        config.policies = vec![
            ConnectionPolicy {
                connection_id: id.clone(),
                ..Default::default()
            },
            ConnectionPolicy {
                connection_id: "other".into(),
                ..Default::default()
            },
        ]
        .into_iter()
        .map(
            |policy| vigilshell_domain::agent::ConnectionPolicySettings {
                access_enabled: false,
                policy,
            },
        )
        .collect();
        crate::agent::execute(
            &mut store.connection,
            A::SaveConfiguration {
                configuration: config.clone(),
                expected: 0,
            },
        )
        .unwrap();
        let job = new_id();
        assert!(matches!(
            store.execute(DbCommand::BeginDeletion {
                id: id.clone(),
                version: 2,
                job: job.clone(),
            }),
            Err(WorkspaceError::Conflict)
        ));
        assert!(
            matches!(store.execute(DbCommand::CredentialJobs).unwrap(), DbReply::Jobs(jobs) if jobs.is_empty())
        );
        store
            .execute(DbCommand::BeginDeletion {
                id: id.clone(),
                version: 1,
                job: job.clone(),
            })
            .unwrap();
        store.connection.execute_batch("CREATE TRIGGER refuse_delete BEFORE DELETE ON connection_profiles BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
        assert!(store.execute(DbCommand::Delete(id.clone())).is_err());
        let R::Configuration(rolled_back) =
            crate::agent::execute(&mut store.connection, A::Configuration).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            serde_json::to_value(&rolled_back).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
        assert!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &id
            )
            .is_ok()
        );
        assert!(
            matches!(store.execute(DbCommand::CredentialJobs).unwrap(), DbReply::Jobs(jobs) if jobs.len() == 1 && jobs[0].id == job && jobs[0].action == "delete")
        );
        store
            .connection
            .execute_batch("DROP TRIGGER refuse_delete")
            .unwrap();
        store.execute(DbCommand::Delete(id.clone())).unwrap();
        store.execute(DbCommand::FinishCredential(job)).unwrap();
        let R::Configuration(current) =
            crate::agent::execute(&mut store.connection, A::Configuration).unwrap()
        else {
            panic!()
        };
        let mut expected = config.clone();
        expected.policies.remove(0);
        expected.settings.revision = 2;
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert!(matches!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &id
            ),
            Err(WorkspaceError::NotFound)
        ));
        config.settings.revision = 2;
        assert!(matches!(
            crate::agent::execute(
                &mut store.connection,
                A::SaveConfiguration {
                    configuration: config,
                    expected: 1
                }
            ),
            Err(WorkspaceError::Conflict)
        ));
    }

    #[test]
    fn credential_removal_commits_with_configuration_and_checks_version() {
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::test_support::open(root.path()).unwrap();
        let id = new_id();
        let DbReply::Connection(original) = store
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: None,
                group_id: None,
                input: input(),
                credential: CredentialChange::Replace {
                    reference: new_id(),
                    label: "private fixture".into(),
                },
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        let mut edited = original.input.clone();
        edited.notes = "keep these edits".into();
        assert!(matches!(
            store.execute(DbCommand::Save {
                id: id.clone(),
                expected_version: Some(original.version + 1),
                group_id: None,
                input: edited.clone(),
                credential: CredentialChange::Remove,
                trust: None,
            }),
            Err(WorkspaceError::Conflict)
        ));
        let DbReply::Connection(unchanged) = store.execute(DbCommand::Get(id.clone())).unwrap()
        else {
            panic!()
        };
        assert_eq!(unchanged.credential, original.credential);
        assert_eq!(unchanged.input.notes, original.input.notes);
        let DbReply::Connection(updated) = store
            .execute(DbCommand::Save {
                id,
                expected_version: Some(original.version),
                group_id: None,
                input: edited,
                credential: CredentialChange::Remove,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert!(updated.credential.is_none() && updated.credential_label.is_none());
        assert_eq!(updated.input.notes, "keep these edits");
        assert_eq!(updated.version, original.version + 1);
        assert_eq!(updated.revision, original.revision + 1);
    }

    #[test]
    fn group_removal_conflicts_and_delete_preserve_connections() {
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::test_support::open(root.path()).unwrap();
        let DbReply::Group(project) = store
            .execute(DbCommand::CreateGroup("group".into()))
            .unwrap()
        else {
            panic!()
        };
        let id = new_id();
        let DbReply::Connection(profile) = store
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: None,
                group_id: Some(project.id.clone()),
                input: input(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(profile.group_id, Some(project.id.clone()));
        assert_eq!(
            store
                .execute(DbCommand::Save {
                    id: id.clone(),
                    expected_version: Some(2),
                    group_id: None,
                    input: input(),
                    credential: CredentialChange::Keep,
                    trust: None
                })
                .unwrap_err(),
            WorkspaceError::Conflict
        );
        store
            .execute(DbCommand::DeleteGroup {
                id: project.id,
                version: 1,
            })
            .unwrap();
        assert_eq!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &id
            )
            .unwrap()
            .group_id,
            None
        );
        let job = new_id();
        store
            .execute(DbCommand::BeginDeletion {
                id: id.clone(),
                version: 1,
                job: job.clone(),
            })
            .unwrap();
        assert_eq!(
            store
                .execute(DbCommand::Save {
                    id: id.clone(),
                    expected_version: Some(1),
                    group_id: None,
                    input: input(),
                    credential: CredentialChange::Keep,
                    trust: None
                })
                .unwrap_err(),
            WorkspaceError::Busy
        );
        store.execute(DbCommand::Delete(id)).unwrap();
        store.execute(DbCommand::FinishCredential(job)).unwrap();
        assert!(matches!(
            get(
                &store.connection,
                store.configuration_cipher.as_deref().unwrap(),
                &profile.id
            ),
            Err(WorkspaceError::NotFound)
        ));
    }
}

fn proxy_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProxyRecord> {
    Ok(ProxyRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        host: row.get(3)?,
        port: row.get(4)?,
        username: row.get(5)?,
        credential: row.get(6)?,
        version: row.get::<_, i64>(7)? as u64,
    })
}
fn proxy_get(c: &Connection, id: &str) -> Result<ProxyRecord, WorkspaceError> {
    db(
        c.query_row("SELECT * FROM proxy_profiles WHERE id=?1", [id], proxy_row)
            .optional(),
    )?
    .ok_or(WorkspaceError::NotFound)
}
fn page_version(c: &Connection, expected: Option<u64>) -> Result<u64, WorkspaceError> {
    let version = db(
        c.query_row("SELECT revision FROM workspace_meta WHERE id=1", [], |r| {
            r.get::<_, i64>(0)
        }),
    )? as u64;
    if expected.is_some_and(|v| v != version) {
        return Err(WorkspaceError::Conflict);
    }
    Ok(version)
}
fn page_next<T>(rows: &mut Vec<T>, offset: u32) -> Option<u32> {
    if rows.len() > 50 {
        rows.pop();
        Some(offset.saturating_add(50))
    } else {
        None
    }
}
fn proxy_list(c: &Connection, offset: u32) -> Result<Vec<ProxyRecord>, WorkspaceError> {
    db(
        db(c.prepare("SELECT * FROM proxy_profiles ORDER BY name,id LIMIT 51 OFFSET ?1"))?
            .query_map([offset], proxy_row)
            .map_err(|_| WorkspaceError::StorageUnavailable)?
            .collect(),
    )
}
fn proxy_references(
    c: &Connection,
    cipher: &dyn ConnectionConfigCipher,
    id: &str,
    offset: u32,
) -> Result<Vec<String>, WorkspaceError> {
    let mut q = db(c.prepare("SELECT id FROM connection_profiles WHERE proxy_id=?1 ORDER BY id"))?;
    let ids = db(
        db(q.query_map([id], |r| r.get::<_, String>(0)))?.collect::<rusqlite::Result<Vec<_>>>()
    )?;
    let mut names = ids
        .iter()
        .map(|id| get(c, cipher, id).map(|p| p.input.name))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names.into_iter().skip(offset as usize).take(51).collect())
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn configuration_roundtrip_versions_and_proxy_references() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::test_support::open(dir.path()).unwrap();
        let proxy = ProxyRecord {
            id: new_id(),
            name: "proxy".into(),
            kind: "socks5".into(),
            host: "127.0.0.1".into(),
            port: 1080,
            username: String::new(),
            credential: None,
            version: 0,
        };
        let DbReply::Proxy(mut proxy) = db
            .execute(DbCommand::SaveProxy {
                proxy,
                expected_version: None,
            })
            .unwrap()
        else {
            panic!()
        };
        let id = new_id();
        let mut input = ConnectionInput {
            notes: "中文备注".into(),
            terminal: Box::default(),
            proxy_id: Some(proxy.id.clone()),
            tunnels: vec![TunnelRule {
                id: new_id(),
                name: "Forward".into(),
                kind: "local".into(),
                bind_host: "127.0.0.1".into(),
                listen_port: 12345,
                target_host: Some("127.0.0.1".into()),
                target_port: Some(22),
                enabled: true,
            }],
            environment: Environment::Development,
            name: "connection".into(),
            host: "localhost".into(),
            port: 22,
            username: "user".into(),
            auth: AuthKind::Password,
        };
        let DbReply::Connection(p) = db
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: None,
                group_id: None,
                input: input.clone(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        db.connection
            .execute(
                "UPDATE connection_profiles SET enabled=0 WHERE id=?1",
                [&id],
            )
            .unwrap();
        let loaded = get(
            &db.connection,
            db.configuration_cipher.as_deref().unwrap(),
            &id,
        )
        .unwrap();
        assert_eq!(loaded.input, input);
        assert_eq!((loaded.version, loaded.revision), (p.version, p.revision));
        let legacy: bool = db
            .connection
            .query_row(
                "SELECT enabled FROM connection_profiles WHERE id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!legacy, "reading a legacy disabled row must not rewrite it");
        let DbReply::Connection(unchanged) = db
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: Some(p.version),
                group_id: None,
                input: input.clone(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (unchanged.version, unchanged.revision),
            (p.version, p.revision)
        );
        assert!(
            !db.connection
                .query_row(
                    "SELECT enabled FROM connection_profiles WHERE id=?1",
                    [&id],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
        input.notes = "new note".into();
        let DbReply::Connection(p) = db
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: Some(p.version),
                group_id: None,
                input: input.clone(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(p.revision, 1);
        assert!(
            db.connection
                .query_row(
                    "SELECT enabled FROM connection_profiles WHERE id=?1",
                    [&id],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
        input.terminal.encoding = "gbk".into();
        let DbReply::Connection(p) = db
            .execute(DbCommand::Save {
                id: id.clone(),
                expected_version: Some(p.version),
                group_id: None,
                input: input.clone(),
                credential: CredentialChange::Keep,
                trust: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(p.revision, 2);
        assert!(matches!(
            db.execute(DbCommand::DeleteProxy {
                id: proxy.id.clone(),
                version: proxy.version
            }),
            Err(WorkspaceError::ProxyInUse)
        ));
        proxy.port = 1081;
        db.execute(DbCommand::SaveProxy {
            proxy: proxy.clone(),
            expected_version: Some(proxy.version),
        })
        .unwrap();
        assert_eq!(
            get(
                &db.connection,
                db.configuration_cipher.as_deref().unwrap(),
                &id
            )
            .unwrap()
            .revision,
            3
        );
        drop(db);
        let db = crate::test_support::open(dir.path()).unwrap();
        let saved = get(
            &db.connection,
            db.configuration_cipher.as_deref().unwrap(),
            &id,
        )
        .unwrap();
        assert!(saved.input == input);
        assert_eq!(saved.input.tunnels.len(), 1);
    }
    #[test]
    fn proxy_pages_detect_concurrent_changes_and_keep_reads_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = crate::test_support::open(dir.path()).unwrap();
        let mut saved = None;
        for i in 0..51 {
            let DbReply::Proxy(proxy) = db
                .execute(DbCommand::SaveProxy {
                    proxy: ProxyRecord {
                        id: new_id(),
                        name: format!("proxy-{i:02}"),
                        kind: "http".into(),
                        host: "localhost".into(),
                        port: 8080,
                        username: String::new(),
                        credential: None,
                        version: 0,
                    },
                    expected_version: None,
                })
                .unwrap()
            else {
                panic!()
            };
            saved = Some(proxy);
        }
        let DbReply::Proxies {
            proxies,
            version,
            next,
        } = db
            .execute(DbCommand::ListProxies {
                offset: 0,
                version: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(proxies.len(), 50);
        assert_eq!(next, Some(50));
        let DbReply::Proxies { proxies, next, .. } = db
            .execute(DbCommand::ListProxies {
                offset: 50,
                version: Some(version),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(proxies.len(), 1);
        assert_eq!(next, None);
        let proxy = saved.unwrap();
        let DbReply::Proxy(internal) = db.execute(DbCommand::GetProxy(proxy.id.clone())).unwrap()
        else {
            panic!()
        };
        assert_eq!(internal.id, proxy.id);
        assert_eq!(internal.version, proxy.version);
        db.execute(DbCommand::SaveProxy {
            proxy: proxy.clone(),
            expected_version: Some(proxy.version),
        })
        .unwrap();
        assert!(
            db.execute(DbCommand::ListProxies {
                offset: 50,
                version: Some(version)
            })
            .is_ok()
        );
        db.execute(DbCommand::DeleteProxy {
            id: proxy.id,
            version: proxy.version,
        })
        .unwrap();
        assert!(matches!(
            db.execute(DbCommand::ListProxies {
                offset: 50,
                version: Some(version)
            }),
            Err(WorkspaceError::Conflict)
        ));
    }
}

#[cfg(test)]
#[path = "workspace_catalogue_tests.rs"]
mod catalogue_tests;
