//! 密码库只在既有 SQLite 所有者线程执行；这里从不接收明文。
use crate::workspace::db;
use rusqlite::{Connection, OptionalExtension, params};

use vigilshell_application_contracts::ports::{
    DbCommand, DbReply, EncryptedCredential, VaultMetadata,
};
use vigilshell_domain::workspace::*;

pub(crate) fn execute(
    c: &mut Connection,
    command: &DbCommand,
) -> Option<Result<DbReply, WorkspaceError>> {
    Some(match command {
        DbCommand::VaultMetadata => db(c.query_row(
            "SELECT namespace,key_version,initialized FROM workspace_meta CROSS JOIN credential_vault WHERE workspace_meta.id=1 AND credential_vault.id=1",
            [], |r| Ok(VaultMetadata { namespace: r.get(0)?, key_version: r.get(1)?, initialized: r.get(2)? })
        )).map(DbReply::VaultMetadata),
        DbCommand::InitializeVault { namespace, key_version } => {
            db(c.execute("UPDATE credential_vault SET initialized=1 WHERE id=1 AND key_version=?1 AND EXISTS(SELECT 1 FROM workspace_meta WHERE namespace=?2)", params![key_version,namespace]))
                .and_then(|n| if n == 1 { Ok(DbReply::Done) } else { Err(WorkspaceError::CredentialFormatUnsupported) })
        }
        DbCommand::ReadEncryptedCredential(reference) => (|| {
            let row = db(c.query_row("SELECT owner_id,purpose,format_version,key_version,nonce,ciphertext FROM encrypted_credentials WHERE reference=?1", [reference], |r| {
                Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            }).optional())?.ok_or(WorkspaceError::CredentialMissing)?;
            Ok(DbReply::EncryptedCredential(EncryptedCredential {
                reference: reference.clone(), context: CredentialContext { owner_id: row.0, purpose: CredentialPurpose::parse(&row.1)? },
                format_version: row.2, key_version: row.3, nonce: row.4, ciphertext: row.5,
            }))
        })(),
        DbCommand::WriteEncryptedCredential(record) => db(c.execute(
            "INSERT INTO encrypted_credentials SELECT ?1,?2,?3,?4,?5,?6,?7 WHERE EXISTS(SELECT 1 FROM credential_vault WHERE id=1 AND initialized=1 AND key_version=?5)",
            params![record.reference,record.context.owner_id,record.context.purpose.code(),record.format_version,record.key_version,record.nonce,record.ciphertext]
        )).and_then(|n| if n == 1 { Ok(DbReply::Done) } else { Err(WorkspaceError::VaultKeyMissing) }),
        DbCommand::RemoveEncryptedCredential(reference) => db(c.execute("DELETE FROM encrypted_credentials WHERE reference=?1", [reference])).map(|_| DbReply::Done),
        _ => return None,
    })
}

pub(crate) fn validate_rows(c: &Connection) -> rusqlite::Result<bool> {
    let metadata: Vec<(i64, i64, bool)> = c
        .prepare("SELECT id,key_version,initialized FROM credential_vault")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if metadata.len() != 1 || metadata[0].0 != 1 || metadata[0].1 <= 0 {
        return Ok(false);
    }
    let count: i64 = c.query_row("SELECT count(*) FROM encrypted_credentials", [], |r| {
        r.get(0)
    })?;
    Ok(metadata[0].2 || count == 0)
}
