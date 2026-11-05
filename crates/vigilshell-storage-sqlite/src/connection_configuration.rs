//! 连接正文的持久化边界；明文只在一次读写中短暂存在。
use crate::workspace::db;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use vigilshell_application_contracts::ports::ConnectionConfigCipher;
use vigilshell_domain::workspace::*;

const VERIFIER_ID: &str = "connection-configuration-verifier";
const VERIFIER: &[u8] = b"VigilShell connection configuration key check v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    input: ConnectionInput,
    credential_label: Option<String>,
}
fn seal(
    cipher: &dyn ConnectionConfigCipher,
    p: &ConnectionRecord,
) -> Result<Vec<u8>, WorkspaceError> {
    let body = Body {
        input: p.input.clone(),
        credential_label: p.credential_label.clone(),
    };
    let bytes = zeroize::Zeroizing::new(
        serde_json::to_vec(&body).map_err(|_| WorkspaceError::InvalidInput)?,
    );
    cipher.seal(&p.id, &bytes)
}
pub(super) fn read_profile(
    c: &Connection,
    cipher: &dyn ConnectionConfigCipher,
    id: &str,
) -> Result<ConnectionRecord, WorkspaceError> {
    let row = db(c.query_row("SELECT group_id,group_sort_order,created_at,version,revision,credential_ref,deleting,proxy_id,config FROM connection_profiles WHERE id=?1", [id], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)? as u64, r.get::<_, i64>(4)? as u64, r.get::<_, Option<String>>(5)?, r.get::<_, bool>(6)?, r.get::<_, Option<String>>(7)?, r.get::<_, Vec<u8>>(8)?))
    }).optional())?.ok_or(WorkspaceError::NotFound)?;
    let bytes = cipher.open(id, &row.8)?;
    let body: Body =
        serde_json::from_slice(&bytes).map_err(|_| WorkspaceError::ConfigurationCorrupt)?;
    if body.input.proxy_id != row.7 || body.input.clone().normalized().as_ref() != Ok(&body.input) {
        return Err(WorkspaceError::ConfigurationCorrupt);
    }
    Ok(ConnectionRecord {
        id: id.into(),
        group_id: row.0,
        group_sort_order: row.1,
        created_at: row.2,
        input: body.input,
        version: row.3,
        revision: row.4,
        credential: row.5,
        credential_label: body.credential_label,
        deleting: row.6,
        credential_pending: db(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM credential_jobs WHERE connection_id=?1)",
            [id],
            |r| r.get(0),
        ))?,
    })
}
pub(super) fn write_profile(
    c: &Connection,
    cipher: &dyn ConnectionConfigCipher,
    p: &ConnectionRecord,
    update: bool,
) -> Result<(), WorkspaceError> {
    let config = seal(cipher, p)?;
    if update {
        db(c.execute("UPDATE connection_profiles SET enabled=1,config=?2,version=?3,revision=?4,credential_ref=?5,proxy_id=?6 WHERE id=?1", params![p.id, config, p.version as i64, p.revision as i64, p.credential, p.input.proxy_id]))?;
    } else {
        db(c.execute("INSERT INTO connection_profiles (id,group_id,group_sort_order,created_at,version,revision,credential_ref,deleting,proxy_id,config) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![p.id, p.group_id, p.group_sort_order as i64, p.created_at, p.version as i64, p.revision as i64, p.credential, p.deleting, p.input.proxy_id, config]))?;
    }
    Ok(())
}
pub(super) fn verify(
    c: &Connection,
    cipher: &dyn ConnectionConfigCipher,
) -> Result<(), WorkspaceError> {
    let verifier: Option<Vec<u8>> = db(c.query_row(
        "SELECT verifier FROM connection_configuration WHERE id=1",
        [],
        |r| r.get(0),
    ))?;
    if let Some(verifier) = verifier {
        if cipher.open(VERIFIER_ID, &verifier)?.as_slice() != VERIFIER {
            return Err(WorkspaceError::ConfigurationCorrupt);
        }
    } else {
        let count: i64 =
            db(c.query_row("SELECT (SELECT count(*) FROM connection_profiles) + (SELECT count(*) FROM encrypted_credentials)", [], |r| r.get(0)))?;
        if count != 0 {
            return Err(WorkspaceError::ConfigurationCorrupt);
        }
        let verifier = cipher.seal(VERIFIER_ID, VERIFIER)?;
        db(c.execute(
            "UPDATE connection_configuration SET verifier=?1 WHERE id=1 AND verifier IS NULL",
            [verifier],
        ))?;
    }
    Ok(())
}
pub(super) fn validate_rows(c: &Connection) -> rusqlite::Result<bool> {
    let rows: Vec<Option<Vec<u8>>> = c
        .prepare("SELECT verifier FROM connection_configuration WHERE id=1")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if rows.len() != 1 {
        return Ok(false);
    }
    let count: i64 = c.query_row("SELECT (SELECT count(*) FROM connection_profiles) + (SELECT count(*) FROM encrypted_credentials)", [], |r| r.get(0))?;
    let initialized: bool = c.query_row(
        "SELECT initialized FROM credential_vault WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    if (count != 0 && rows[0].is_none()) || (rows[0].is_some() && !initialized) {
        return Ok(false);
    }
    let mut q = c.prepare("SELECT id,credential_ref FROM connection_profiles")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let reference: Option<String> = row.get(1)?;
        if !valid_id(&id) || reference.as_ref().is_some_and(|v| !valid_id(v)) {
            return Ok(false);
        }
    }
    Ok(true)
}
