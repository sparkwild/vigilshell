//! 显式配置转换仅处理调用方已打开的数据库；正常启动与 Agent 执行不调用此模块。
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use vigilshell_domain::{
    MAX_REVISION,
    agent::{AccessSecret, AgentSettings, ConnectionPolicy, ConnectionPolicySettings},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionError {
    StorageUnavailable,
    UnsupportedStorage,
    InvalidConfiguration,
    RevisionLimit,
    ActiveTasks,
    Conflict,
}

type Result<T> = std::result::Result<T, ConversionError>;
mod unique_json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversionPreview {
    pub source_revision: u64,
    pub target_revision: u64,
    pub connection_ids: Vec<String>,
}

/// 不序列化源正文或令牌；只能由已核对的配置生成，提交前再次比较原文和版本。
pub struct ConfigurationConversion {
    preview: ConversionPreview,
    source_digest: [u8; 32],
    target_body: String,
}

impl ConfigurationConversion {
    pub fn preview(&self) -> &ConversionPreview {
        &self.preview
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionOutcome {
    Applied,
    AlreadyConverted,
    Unchanged,
}

fn storage<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|_| ConversionError::StorageUnavailable)
}

fn validate_storage(c: &Connection) -> Result<()> {
    let application: u32 = storage(c.pragma_query_value(None, "application_id", |r| r.get(0)))?;
    let version: u32 = storage(c.pragma_query_value(None, "user_version", |r| r.get(0)))?;
    let integrity: String = storage(c.query_row("PRAGMA quick_check(1)", [], |r| r.get(0)))?;
    if application != super::APPLICATION_ID
        || version != super::SCHEMA_VERSION
        || integrity != "ok"
        || !storage(super::validate_schema(c))?
    {
        return Err(ConversionError::UnsupportedStorage);
    }
    Ok(())
}

fn idle(c: &Connection) -> Result<()> {
    let active: bool = storage(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_tasks WHERE active=1)",
        [],
        |r| r.get(0),
    ))?;
    if active {
        return Err(ConversionError::ActiveTasks);
    }
    Ok(())
}

fn configuration(c: &Connection) -> Result<(u64, String)> {
    let (revision, body): (i64, String) = storage(c.query_row(
        "SELECT revision,body FROM agent_configuration WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))?;
    let revision = u64::try_from(revision).map_err(|_| ConversionError::InvalidConfiguration)?;
    if revision > MAX_REVISION || body.len() > 512 * 1024 {
        return Err(ConversionError::InvalidConfiguration);
    }
    Ok((revision, body))
}

/// 只读准备；保留设置、令牌、接入代次和非目标字段。已知 v1 权限定向转为 v2，并将现有访问状态与权限分离。
/// 后续维护调用者必须先停止应用并持有数据目录锁；此函数不打开路径或获得额外权限。
pub fn prepare(c: &Connection) -> Result<ConfigurationConversion> {
    validate_storage(c)?;
    idle(c)?;
    let (revision, body) = configuration(c)?;
    // Value 会采用重复键的最后一个值，转换前必须拒绝歧义，避免丢令牌或改变授权。
    unique_json::validate(&body).map_err(|_| ConversionError::InvalidConfiguration)?;
    let mut document: Value =
        serde_json::from_str(&body).map_err(|_| ConversionError::InvalidConfiguration)?;
    let settings: AgentSettings = serde_json::from_value(
        document
            .get("settings")
            .cloned()
            .ok_or(ConversionError::InvalidConfiguration)?,
    )
    .map_err(|_| ConversionError::InvalidConfiguration)?;
    if settings.revision != revision
        || settings.endpoint_revision > MAX_REVISION
        || !(1..=365).contains(&settings.retention_days)
        || !(16..=1024).contains(&settings.storage_mib)
    {
        return Err(ConversionError::InvalidConfiguration);
    }
    // 仅验证形状，不重新编码或重建接入项。
    let _: Vec<AccessSecret> = serde_json::from_value(
        document
            .get("entries")
            .cloned()
            .ok_or(ConversionError::InvalidConfiguration)?,
    )
    .map_err(|_| ConversionError::InvalidConfiguration)?;
    let policies = document
        .get_mut("policies")
        .and_then(Value::as_array_mut)
        .ok_or(ConversionError::InvalidConfiguration)?;
    let mut connections = std::collections::HashSet::new();
    let mut converted = Vec::new();
    for value in policies {
        let old_access = !value
            .as_object()
            .ok_or(ConversionError::InvalidConfiguration)?
            .contains_key("accessEnabled");

        let version = value.pointer("/execution/version").and_then(Value::as_u64);
        match version {
            Some(1) => {
                let execution = value
                    .get_mut("execution")
                    .and_then(Value::as_object_mut)
                    .ok_or(ConversionError::InvalidConfiguration)?;
                // One-off input-shape validation only. The v1 object never becomes an execution policy.
                if execution
                    .get("commandNetwork")
                    .and_then(Value::as_bool)
                    .is_none()
                    || execution.contains_key("scriptReview")
                {
                    return Err(ConversionError::InvalidConfiguration);
                }
                execution.remove("commandNetwork");
                execution.insert("version".into(), 2.into());
                execution.insert("scriptReview".into(), false.into());
                let current: ConnectionPolicy = serde_json::from_value(value.clone())
                    .map_err(|_| ConversionError::InvalidConfiguration)?;
                current
                    .validate()
                    .map_err(|_| ConversionError::InvalidConfiguration)?;
                if current.revision == MAX_REVISION {
                    return Err(ConversionError::RevisionLimit);
                }
                value["revision"] = (current.revision + 1).into();
                converted.push(current.connection_id);
            }
            Some(2) => {}
            _ => return Err(ConversionError::InvalidConfiguration),
        }
        if old_access {
            let enabled =
                value.pointer("/execution/level").and_then(Value::as_str) != Some("disabled");
            if !enabled {
                value["execution"]["level"] = "readOnly".into();
            }
            value["accessEnabled"] = enabled.into();
            let id = value["connectionId"]
                .as_str()
                .ok_or(ConversionError::InvalidConfiguration)?
                .to_owned();
            if !converted.contains(&id) {
                let old = value["revision"]
                    .as_u64()
                    .ok_or(ConversionError::InvalidConfiguration)?;
                value["revision"] = old
                    .checked_add(1)
                    .filter(|v| *v <= MAX_REVISION)
                    .ok_or(ConversionError::RevisionLimit)?
                    .into();
                converted.push(id);
            }
        }
        let current: ConnectionPolicySettings = serde_json::from_value(value.clone())
            .map_err(|_| ConversionError::InvalidConfiguration)?;
        current
            .validate()
            .map_err(|_| ConversionError::InvalidConfiguration)?;
        if !connections.insert(current.connection_id.clone()) {
            return Err(ConversionError::InvalidConfiguration);
        }
    }
    let target_revision = if converted.is_empty() {
        revision
    } else {
        revision
            .checked_add(1)
            .filter(|v| *v <= MAX_REVISION)
            .ok_or(ConversionError::RevisionLimit)?
    };
    let target_body = if converted.is_empty() {
        body.clone()
    } else {
        document["settings"]["revision"] = target_revision.into();
        serde_json::to_string(&document).map_err(|_| ConversionError::InvalidConfiguration)?
    };
    if target_body.len() > 512 * 1024 {
        return Err(ConversionError::InvalidConfiguration);
    }
    Ok(ConfigurationConversion {
        preview: ConversionPreview {
            source_revision: revision,
            target_revision,
            connection_ids: converted,
        },
        source_digest: Sha256::digest(body.as_bytes()).into(),
        target_body,
    })
}

/// 原文或版本有任何变化即拒绝；提交失败由 SQLite 事务回滚，提交后重试不再推进版本。
pub fn apply(c: &mut Connection, plan: &ConfigurationConversion) -> Result<ConversionOutcome> {
    let tx = storage(c.transaction_with_behavior(TransactionBehavior::Immediate))?;
    validate_storage(&tx)?;
    idle(&tx)?;
    let (revision, body) = configuration(&tx)?;
    if revision == plan.preview.target_revision && body == plan.target_body {
        return Ok(if plan.preview.connection_ids.is_empty() {
            ConversionOutcome::Unchanged
        } else {
            ConversionOutcome::AlreadyConverted
        });
    }
    let digest: [u8; 32] = Sha256::digest(body.as_bytes()).into();
    if revision != plan.preview.source_revision || digest != plan.source_digest {
        return Err(ConversionError::Conflict);
    }
    let changed = storage(tx.execute(
        "UPDATE agent_configuration SET revision=?1,body=?2 WHERE id=1 AND revision=?3 AND body=?4",
        params![
            plan.preview.target_revision as i64,
            plan.target_body,
            revision as i64,
            body
        ],
    ))?;
    if changed != 1 {
        return Err(ConversionError::Conflict);
    }
    storage(tx.commit())?;
    Ok(ConversionOutcome::Applied)
}

#[cfg(test)]
mod tests;

/// Explicit Dev-only maintenance; never invoked by normal application startup.
#[cfg(target_os = "macos")]
pub fn run_dev(directory: &std::path::Path) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::{
        fs::{self, OpenOptions},
        os::unix::fs::OpenOptionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };
    if directory.join("backend-vault-reset.pending.json").exists() {
        return Err("Finish pending maintenance first".into());
    }
    let path = directory.join("app.sqlite3");
    let lock_path = directory.join("runtime.lock");
    for file in [&path, &lock_path] {
        if !fs::symlink_metadata(file)?.file_type().is_file() {
            return Err("Expected regular Dev files".into());
        }
    }
    let lock = OpenOptions::new().read(true).write(true).open(lock_path)?;
    lock.try_lock()?;
    let mut db = Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let plan = prepare(&db).map_err(|e| format!("Policy conversion refused: {e:?}"))?;
    println!(
        "Policy conversion: {} connections, revision {} -> {}",
        plan.preview.connection_ids.len(),
        plan.preview.source_revision,
        plan.preview.target_revision
    );
    if plan.preview.connection_ids.is_empty() {
        return Ok(());
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let backup = directory.join(format!("app.before-policy-settings-{stamp}.sqlite3"));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&backup)?;
    db.execute(
        "VACUUM INTO ?1",
        [backup.to_str().ok_or("Invalid backup path")?],
    )?;
    println!("Recovery copy: {}", backup.display());
    apply(&mut db, &plan).map_err(|e| format!("Policy conversion failed: {e:?}"))?;
    if !prepare(&db)
        .map_err(|e| format!("Verification failed: {e:?}"))?
        .preview
        .connection_ids
        .is_empty()
    {
        return Err("Policy conversion verification failed".into());
    }
    println!("Policy configuration upgraded; historical data unchanged");
    Ok(())
}
