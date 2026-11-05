//! Agent 配置及审计沿用唯一 SQLite 工作线程，不打开第二个数据库。
use crate::workspace::db;
use rusqlite::{Connection, params};
use vigilshell_application_contracts::ports::{AgentDbCommand as C, AgentDbReply as R};
use vigilshell_domain::{agent::*, workspace::WorkspaceError};

fn encode<T: serde::Serialize>(v: &T) -> Result<String, WorkspaceError> {
    serde_json::to_string(v).map_err(|_| WorkspaceError::StorageUnavailable)
}
fn configuration(c: &Connection) -> Result<AgentConfiguration, WorkspaceError> {
    let (revision, body): (i64, String) = db(c.query_row(
        "SELECT revision,body FROM agent_configuration WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))?;
    let raw: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| WorkspaceError::AgentConfigurationUnavailable)?;
    if raw
        .get("policies")
        .and_then(|v| v.as_array())
        .is_some_and(|ps| ps.iter().any(|p| p.get("accessEnabled").is_none()))
    {
        return Err(WorkspaceError::PolicyUpgradeRequired);
    }
    let config: AgentConfiguration =
        serde_json::from_str(&body).map_err(|_| WorkspaceError::AgentConfigurationUnavailable)?;
    if revision < 0
        || config.settings.revision != revision as u64
        || config.settings.revision > vigilshell_domain::MAX_REVISION
        || config.settings.endpoint_revision > vigilshell_domain::MAX_REVISION
        || !(1..=365).contains(&config.settings.retention_days)
        || !(16..=1024).contains(&config.settings.storage_mib)
        || config
            .policies
            .iter()
            .any(|policy| policy.validate().is_err())
    {
        return Err(WorkspaceError::AgentConfigurationUnavailable);
    }
    Ok(config)
}
// 与连接删除共用事务；推进 CAS 版本，防止已打开的权限页写回被删除的策略。
pub(crate) fn remove_connection_policy(
    c: &Connection,
    connection_id: &str,
) -> Result<(), WorkspaceError> {
    let mut config = match configuration(c) {
        Ok(config) => config,
        // 不可解析的可选配置原文保留；连接删除不能变成隐式配置重建。
        Err(
            WorkspaceError::AgentConfigurationUnavailable | WorkspaceError::PolicyUpgradeRequired,
        ) => return Ok(()),
        Err(error) => return Err(error),
    };
    let previous = config.policies.len();
    config.policies.retain(|p| p.connection_id != connection_id);
    if config.policies.len() == previous {
        return Ok(());
    }
    config.settings.revision =
        vigilshell_domain::workspace::next_version(config.settings.revision)?;
    db(c.execute(
        "UPDATE agent_configuration SET revision=?1,body=?2 WHERE id=1",
        params![config.settings.revision as i64, encode(&config)?],
    ))?;
    Ok(())
}
pub(crate) fn validate_rows(c: &Connection) -> rusqlite::Result<bool> {
    // 开库只验证必需的存储结构；Agent 配置语义失败由其读取入口单独拒绝。
    let rows = c
        .prepare("SELECT id,revision FROM agent_configuration")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(
        matches!(rows.as_slice(), [(1, revision)] if *revision >= 0 && *revision as u64 <= vigilshell_domain::MAX_REVISION),
    )
}
pub(crate) fn execute(c: &mut Connection, command: C) -> Result<R, WorkspaceError> {
    match command {
        C::Recording(_) => Err(WorkspaceError::InvalidInput),
        C::Observations => {
            let mut query = db(c.prepare("SELECT entry_id,generation,endpoint,first_at,last_at,endpoint_revision FROM agent_access_observations ORDER BY last_at DESC LIMIT 256"))?;
            let records = db(query.query_map([], |r| {
                Ok(AccessObservation {
                    endpoint_revision: r.get::<_, i64>(5)? as u64,
                    entry_id: r.get(0)?,
                    generation: r.get::<_, i64>(1)? as u64,
                    endpoint: r.get(2)?,
                    first_at: r.get::<_, i64>(3)? as u64,
                    last_at: r.get::<_, i64>(4)? as u64,
                })
            }))?;
            Ok(R::Observations(db(
                records.collect::<rusqlite::Result<Vec<_>>>()
            )?))
        }
        C::Observe { observation: o } => {
            db(c.execute("INSERT INTO agent_access_observations VALUES(?1,?2,?3,?6,?4,?5) ON CONFLICT(entry_id,generation,endpoint,endpoint_revision) DO UPDATE SET last_at=MAX(last_at,excluded.last_at)", params![o.entry_id,o.generation as i64,o.endpoint,o.first_at as i64,o.last_at as i64,o.endpoint_revision as i64]))?;
            db(c.execute("DELETE FROM agent_access_observations WHERE rowid NOT IN (SELECT rowid FROM agent_access_observations ORDER BY last_at DESC LIMIT 256)", []))?;
            Ok(R::Done)
        }
        C::OutputRange { session, task } => {
            let (start,end):(Option<i64>,Option<i64>)=db(c.query_row("SELECT min(sequence),max(sequence) FROM agent_recording_events WHERE session_id=?1 AND task_id=?2 AND kind='output'",params![session,task],|row|Ok((row.get(0)?,row.get(1)?))))?;
            Ok(R::OutputRange {
                start: start.unwrap_or(0).max(0) as u64,
                end: end.unwrap_or(0).max(0) as u64,
            })
        }
        C::Configuration => Ok(R::Configuration(configuration(c)?)),
        C::SaveConfiguration {
            configuration,
            expected,
        } => {
            // 正常保存不能覆盖损坏配置；恢复／转换必须通过单独的显式维护流程。
            self::configuration(c)?;
            if configuration.settings.revision != expected + 1
                || configuration.policies.iter().any(|p| p.validate().is_err())
            {
                return Err(WorkspaceError::InvalidInput);
            }
            if encode(&configuration)?.len() > 512 * 1024 {
                return Err(WorkspaceError::InvalidInput);
            }
            let changed = db(c.execute(
                "UPDATE agent_configuration SET revision=?1,body=?2 WHERE id=1 AND revision=?3",
                params![
                    configuration.settings.revision as i64,
                    encode(&configuration)?,
                    expected as i64
                ],
            ))?;
            if changed != 1 {
                return Err(WorkspaceError::Conflict);
            }
            Ok(R::Done)
        }
        C::SaveAuditTask { task } => {
            if task.phase.active()
                || task.operation.is_some()
                || task.execution_state.is_some()
                || task.policy_snapshot.is_some()
            {
                return Err(WorkspaceError::InvalidInput);
            }
            db(c.execute("INSERT INTO agent_tasks(id,entry_id,connection_id,created_at,active,request_key,fingerprint,body) VALUES(?1,?2,?3,?4,0,NULL,NULL,?5)",params![task.id,task.entry_id,task.connection_id,task.created_at as i64,encode(&task)?]))?;
            Ok(R::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_is_cas_and_defaults_deny() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("migrations/001_initial.sql"))
            .unwrap();
        let mut config = configuration(&c).unwrap();
        assert!(!config.settings.enabled);
        config.settings.revision = 1;
        execute(
            &mut c,
            C::SaveConfiguration {
                configuration: config.clone(),
                expected: 0,
            },
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                C::SaveConfiguration {
                    configuration: config,
                    expected: 0
                }
            )
            .is_err()
        );
        assert!(validate_rows(&c).unwrap());
    }
}
