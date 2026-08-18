use std::{
    error::Error,
    fmt,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use rusqlite::{Connection, Row, Transaction, TransactionBehavior, params};
use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, HostFingerprint, SshRoute, StoredHostTrust,
    },
    connection_mutation::{
        ConnectionMutationRepositoryError, ConnectionProfileMutationRepository,
        CreateConnectionProfileResult, DeleteConnectionProfileResult,
        ReplaceConnectionProfileResult,
    },
    host_trust::{
        HostTrustMutationRepository, HostTrustMutationRepositoryError, StoreHostTrustResult,
    },
    human_terminal::{
        ConnectionProfileRepository, ConnectionRepositoryError, HostTrustRepository,
        HostTrustRepositoryError,
    },
};

const CURRENT_SCHEMA_VERSION: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteStorageUnavailable;

impl fmt::Display for SqliteStorageUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("storage is unavailable")
    }
}

impl Error for SqliteStorageUnavailable {}

#[derive(Clone)]
pub struct SqliteStorage {
    connection: Arc<Mutex<Connection>>,
}

impl fmt::Debug for SqliteStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteStorage")
            .finish_non_exhaustive()
    }
}

impl SqliteStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, SqliteStorageUnavailable> {
        let connection = Connection::open(path).map_err(|_| SqliteStorageUnavailable)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, SqliteStorageUnavailable> {
        let connection = Connection::open_in_memory().map_err(|_| SqliteStorageUnavailable)?;
        Self::from_connection(connection)
    }

    pub fn from_connection(mut connection: Connection) -> Result<Self, SqliteStorageUnavailable> {
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|_| SqliteStorageUnavailable)?;

        Self::run_migrations(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn schema_version(&self) -> Result<i32, SqliteStorageUnavailable> {
        let connection = self.lock_connection()?;
        Self::schema_version_for(&connection)
    }

    /// 在显式退出前确认没有悬空事务，并将 SQLite 页缓存写入持久存储。
    pub fn flush_for_shutdown(&self) -> Result<(), SqliteStorageUnavailable> {
        let connection = self
            .connection
            .try_lock()
            .map_err(|_| SqliteStorageUnavailable)?;
        if !connection.is_autocommit() {
            return Err(SqliteStorageUnavailable);
        }
        connection
            .cache_flush()
            .map_err(|_| SqliteStorageUnavailable)
    }

    pub fn list_connection_profiles(
        &self,
    ) -> Result<Vec<ConnectionProfile>, SqliteStorageUnavailable> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT
                    id,
                    server_id,
                    display_name,
                    record_version,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                FROM connection_profiles
                ORDER BY id ASC
                "#,
            )
            .map_err(|_| SqliteStorageUnavailable)?;
        let mut rows = statement.query(()).map_err(|_| SqliteStorageUnavailable)?;
        let mut profiles = Vec::new();

        while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
            profiles.push(decode_connection_profile(row)?);
        }

        Ok(profiles)
    }

    /// 仅供 debug/test fixture 写入完整 Connection 快照。
    #[cfg(any(test, debug_assertions))]
    pub fn upsert_connection_profile_fixture(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<(), SqliteStorageUnavailable> {
        self.lock_connection()?
            .execute(
                r#"
                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    display_name,
                    record_version,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(id) DO UPDATE SET
                    server_id = excluded.server_id,
                    display_name = excluded.display_name,
                    record_version = excluded.record_version,
                    security_revision = excluded.security_revision,
                    route_host = excluded.route_host,
                    route_port = excluded.route_port,
                    route_username = excluded.route_username,
                    credential_ref = excluded.credential_ref,
                    enabled = excluded.enabled
                "#,
                params![
                    profile.id().as_str(),
                    profile.server_id().as_str(),
                    profile.display_name().as_str(),
                    i64::try_from(profile.record_version().get())
                        .map_err(|_| SqliteStorageUnavailable)?,
                    i64::try_from(profile.security_revision().get())
                        .map_err(|_| SqliteStorageUnavailable)?,
                    profile.route().host(),
                    profile.route().port(),
                    profile.route().username(),
                    profile.credential_ref().as_str(),
                    profile.is_enabled(),
                ],
            )
            .map(|_| ())
            .map_err(|_| SqliteStorageUnavailable)
    }

    /// 仅供 debug/test fixture 写入 Host Trust 快照。
    #[cfg(any(test, debug_assertions))]
    pub fn upsert_host_trust_fixture(
        &self,
        trust: &StoredHostTrust,
    ) -> Result<(), SqliteStorageUnavailable> {
        self.lock_connection()?
            .execute(
                r#"
                INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(profile_id) DO UPDATE SET
                    security_revision = excluded.security_revision,
                    fingerprint = excluded.fingerprint
                "#,
                params![
                    trust.profile_id().as_str(),
                    i64::try_from(trust.security_revision().get())
                        .map_err(|_| SqliteStorageUnavailable)?,
                    trust.fingerprint().as_str(),
                ],
            )
            .map(|_| ())
            .map_err(|_| SqliteStorageUnavailable)
    }

    fn lock_connection(&self) -> Result<MutexGuard<'_, Connection>, SqliteStorageUnavailable> {
        self.connection.lock().map_err(|_| SqliteStorageUnavailable)
    }

    fn schema_version_for(connection: &Connection) -> Result<i32, SqliteStorageUnavailable> {
        connection
            .query_row("PRAGMA user_version", (), |row| row.get::<_, i64>(0))
            .map_err(|_| SqliteStorageUnavailable)
            .and_then(|value| i32::try_from(value).map_err(|_| SqliteStorageUnavailable))
    }

    fn run_migrations(connection: &mut Connection) -> Result<(), SqliteStorageUnavailable> {
        let current_version = Self::schema_version_for(connection)?;
        if current_version > CURRENT_SCHEMA_VERSION {
            return Err(SqliteStorageUnavailable);
        }
        if current_version == CURRENT_SCHEMA_VERSION {
            return Ok(());
        }

        let transaction = connection
            .transaction()
            .map_err(|_| SqliteStorageUnavailable)?;

        for version in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
            match version {
                1 => {
                    transaction
                        .execute_batch(
                            r#"
                            CREATE TABLE connection_profiles (
                                id TEXT PRIMARY KEY,
                                server_id TEXT NOT NULL,
                                security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                                route_host TEXT NOT NULL,
                                route_port INTEGER NOT NULL CHECK (route_port > 0),
                                route_username TEXT NOT NULL,
                                credential_ref TEXT NOT NULL,
                                enabled INTEGER NOT NULL CHECK (enabled IN (0,1))
                            );

                            CREATE TABLE host_trusts (
                                profile_id TEXT PRIMARY KEY NOT NULL,
                                security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                                fingerprint TEXT NOT NULL,
                                FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                            );

                            PRAGMA user_version = 1;
                            "#
                        )
                        .map_err(|_| SqliteStorageUnavailable)?;
                }
                2 => {
                    transaction
                        .execute_batch(
                            r#"
                            ALTER TABLE connection_profiles
                                ADD COLUMN display_name TEXT NOT NULL
                                DEFAULT 'Connection'
                                CHECK (length(trim(display_name)) > 0);

                            UPDATE connection_profiles
                            SET display_name = CASE
                                WHEN length(trim(server_id)) > 0
                                    AND length(CAST(trim(server_id) AS BLOB)) <= 128
                                THEN trim(server_id)
                                ELSE 'Connection'
                            END;

                            ALTER TABLE connection_profiles
                                ADD COLUMN record_version INTEGER NOT NULL
                                DEFAULT 1
                                CHECK (record_version > 0);

                            PRAGMA user_version = 2;
                            "#,
                        )
                        .map_err(|_| SqliteStorageUnavailable)?;
                }
                _ => return Err(SqliteStorageUnavailable),
            }
        }

        transaction.commit().map_err(|_| SqliteStorageUnavailable)?;

        Ok(())
    }
}

impl ConnectionProfileRepository for SqliteStorage {
    fn find(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ConnectionRepositoryError::Unavailable)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT
                    id,
                    server_id,
                    display_name,
                    record_version,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                FROM connection_profiles
                WHERE id = ?1
                "#,
            )
            .map_err(|_| ConnectionRepositoryError::Unavailable)?;

        let mut rows = statement
            .query(params![profile_id.as_str()])
            .map_err(|_| ConnectionRepositoryError::Unavailable)?;

        match rows
            .next()
            .map_err(|_| ConnectionRepositoryError::Unavailable)?
        {
            Some(row) => decode_connection_profile(row)
                .map(Some)
                .map_err(|_| ConnectionRepositoryError::Unavailable),
            None => Ok(None),
        }
    }
}

impl ConnectionProfileMutationRepository for SqliteStorage {
    fn create(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<CreateConnectionProfileResult, ConnectionMutationRepositoryError> {
        if profile.record_version() != ConnectionRecordVersion::INITIAL
            || profile.security_revision() != ConnectionSecurityRevision::INITIAL
        {
            return Err(ConnectionMutationRepositoryError::Unavailable);
        }

        let record_version = mutation_counter(profile.record_version().get())?;
        let security_revision = mutation_counter(profile.security_revision().get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let inserted = transaction.execute(
            r#"
            INSERT INTO connection_profiles(
                id,
                server_id,
                display_name,
                record_version,
                security_revision,
                route_host,
                route_port,
                route_username,
                credential_ref,
                enabled
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                profile.id().as_str(),
                profile.server_id().as_str(),
                profile.display_name().as_str(),
                record_version,
                security_revision,
                profile.route().host(),
                profile.route().port(),
                profile.route().username(),
                profile.credential_ref().as_str(),
                profile.is_enabled(),
            ],
        );

        match inserted {
            Ok(1) => {
                transaction
                    .commit()
                    .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
                Ok(CreateConnectionProfileResult::Created)
            }
            Ok(_) => {
                transaction
                    .rollback()
                    .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
                Err(ConnectionMutationRepositoryError::Unavailable)
            }
            Err(error) => {
                let is_conflict = is_profile_id_conflict(&error);
                transaction
                    .rollback()
                    .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
                if is_conflict {
                    Ok(CreateConnectionProfileResult::Conflict)
                } else {
                    Err(ConnectionMutationRepositoryError::Unavailable)
                }
            }
        }
    }

    fn replace_if_version(
        &self,
        profile: &ConnectionProfile,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<ReplaceConnectionProfileResult, ConnectionMutationRepositoryError> {
        if expected_record_version.checked_next() != Some(profile.record_version()) {
            return Err(ConnectionMutationRepositoryError::Unavailable);
        }

        let record_version = mutation_counter(profile.record_version().get())?;
        let security_revision = mutation_counter(profile.security_revision().get())?;
        let expected_record_version = mutation_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let replaced = transaction
            .execute(
                r#"
                UPDATE connection_profiles
                SET
                    server_id = ?2,
                    display_name = ?3,
                    record_version = ?4,
                    security_revision = ?5,
                    route_host = ?6,
                    route_port = ?7,
                    route_username = ?8,
                    credential_ref = ?9,
                    enabled = ?10
                WHERE id = ?1 AND record_version = ?11
                "#,
                params![
                    profile.id().as_str(),
                    profile.server_id().as_str(),
                    profile.display_name().as_str(),
                    record_version,
                    security_revision,
                    profile.route().host(),
                    profile.route().port(),
                    profile.route().username(),
                    profile.credential_ref().as_str(),
                    profile.is_enabled(),
                    expected_record_version,
                ],
            )
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;

        let outcome = match replaced {
            1 => ReplaceConnectionProfileResult::Replaced,
            0 if profile_exists(&transaction, profile.id())? => {
                ReplaceConnectionProfileResult::Conflict
            }
            0 => ReplaceConnectionProfileResult::NotFound,
            _ => return Err(ConnectionMutationRepositoryError::Unavailable),
        };
        transaction
            .commit()
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        Ok(outcome)
    }

    fn delete_if_version(
        &self,
        profile_id: &ConnectionProfileId,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<DeleteConnectionProfileResult, ConnectionMutationRepositoryError> {
        let expected_record_version = mutation_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        let deleted = transaction
            .execute(
                r#"
                DELETE FROM connection_profiles
                WHERE id = ?1 AND record_version = ?2
                "#,
                params![profile_id.as_str(), expected_record_version],
            )
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;

        let outcome = match deleted {
            1 => DeleteConnectionProfileResult::Deleted,
            0 if profile_exists(&transaction, profile_id)? => {
                DeleteConnectionProfileResult::Conflict
            }
            0 => DeleteConnectionProfileResult::NotFound,
            _ => return Err(ConnectionMutationRepositoryError::Unavailable),
        };
        transaction
            .commit()
            .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
        Ok(outcome)
    }
}

impl HostTrustRepository for SqliteStorage {
    fn find(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<StoredHostTrust>, HostTrustRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| HostTrustRepositoryError::Unavailable)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT
                    profile_id,
                    security_revision,
                    fingerprint
                FROM host_trusts
                WHERE profile_id = ?1
                "#,
            )
            .map_err(|_| HostTrustRepositoryError::Unavailable)?;

        let mut rows = statement
            .query(params![profile_id.as_str()])
            .map_err(|_| HostTrustRepositoryError::Unavailable)?;

        match rows
            .next()
            .map_err(|_| HostTrustRepositoryError::Unavailable)?
        {
            Some(row) => decode_host_trust(row)
                .map(Some)
                .map_err(|_| HostTrustRepositoryError::Unavailable),
            None => Ok(None),
        }
    }
}

impl HostTrustMutationRepository for SqliteStorage {
    fn replace_if_security_revision(
        &self,
        trust: &StoredHostTrust,
        expected_security_revision: ConnectionSecurityRevision,
    ) -> Result<StoreHostTrustResult, HostTrustMutationRepositoryError> {
        if trust.security_revision() != expected_security_revision {
            return Err(HostTrustMutationRepositoryError::Unavailable);
        }

        let expected_security_revision = host_trust_counter(expected_security_revision.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| HostTrustMutationRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| HostTrustMutationRepositoryError::Unavailable)?;
        let stored = transaction
            .execute(
                r#"
                INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                SELECT id, security_revision, ?3
                FROM connection_profiles
                WHERE id = ?1
                    AND security_revision = ?2
                    AND enabled = 1
                ON CONFLICT(profile_id) DO UPDATE SET
                    security_revision = excluded.security_revision,
                    fingerprint = excluded.fingerprint
                "#,
                params![
                    trust.profile_id().as_str(),
                    expected_security_revision,
                    trust.fingerprint().as_str(),
                ],
            )
            .map_err(|_| HostTrustMutationRepositoryError::Unavailable)?;

        let outcome = match stored {
            1 => StoreHostTrustResult::Stored,
            0 => transaction
                .query_row(
                    r#"
                    SELECT security_revision, enabled
                    FROM connection_profiles
                    WHERE id = ?1
                    "#,
                    params![trust.profile_id().as_str()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .map_or_else(
                    |error| match error {
                        rusqlite::Error::QueryReturnedNoRows => Ok(StoreHostTrustResult::NotFound),
                        _ => Err(HostTrustMutationRepositoryError::Unavailable),
                    },
                    |(current_revision, enabled)| {
                        if enabled == 0 {
                            Ok(StoreHostTrustResult::Disabled)
                        } else if enabled == 1 && current_revision != expected_security_revision {
                            Ok(StoreHostTrustResult::Stale)
                        } else {
                            Err(HostTrustMutationRepositoryError::Unavailable)
                        }
                    },
                )?,
            _ => return Err(HostTrustMutationRepositoryError::Unavailable),
        };
        transaction
            .commit()
            .map_err(|_| HostTrustMutationRepositoryError::Unavailable)?;
        Ok(outcome)
    }
}

fn mutation_counter(value: u64) -> Result<i64, ConnectionMutationRepositoryError> {
    i64::try_from(value).map_err(|_| ConnectionMutationRepositoryError::Unavailable)
}

fn host_trust_counter(value: u64) -> Result<i64, HostTrustMutationRepositoryError> {
    i64::try_from(value).map_err(|_| HostTrustMutationRepositoryError::Unavailable)
}

fn profile_exists(
    transaction: &Transaction<'_>,
    profile_id: &ConnectionProfileId,
) -> Result<bool, ConnectionMutationRepositoryError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM connection_profiles WHERE id = ?1)",
            params![profile_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|_| ConnectionMutationRepositoryError::Unavailable)
}

fn is_profile_id_conflict(error: &rusqlite::Error) -> bool {
    error.sqlite_extended_error_code() == Some(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY)
}

fn decode_bool(value: i64) -> Result<bool, SqliteStorageUnavailable> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(SqliteStorageUnavailable),
    }
}

fn decode_connection_profile(row: &Row<'_>) -> Result<ConnectionProfile, SqliteStorageUnavailable> {
    let id = ConnectionProfileId::parse(
        row.get::<_, String>(0)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let server_id = vigilshell_core::connection::ManagedServerId::parse(
        row.get::<_, String>(1)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let display_name = ConnectionDisplayName::parse(
        row.get::<_, String>(2)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let record_version_raw = row.get::<_, i64>(3).map_err(|_| SqliteStorageUnavailable)?;
    let record_version = ConnectionRecordVersion::new(
        u64::try_from(record_version_raw).map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let security_revision_raw = row.get::<_, i64>(4).map_err(|_| SqliteStorageUnavailable)?;
    let security_revision = ConnectionSecurityRevision::new(
        u64::try_from(security_revision_raw).map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let route = SshRoute::new(
        row.get::<_, String>(5)
            .map_err(|_| SqliteStorageUnavailable)?,
        row.get::<_, u16>(6).map_err(|_| SqliteStorageUnavailable)?,
        row.get::<_, String>(7)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let credential_ref = CredentialRef::parse(
        row.get::<_, String>(8)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let enabled = decode_bool(row.get::<_, i64>(9).map_err(|_| SqliteStorageUnavailable)?)
        .map_err(|_| SqliteStorageUnavailable)?;

    Ok(ConnectionProfile::new(
        id,
        server_id,
        display_name,
        record_version,
        security_revision,
        route,
        credential_ref,
        enabled,
    ))
}

fn decode_host_trust(row: &Row<'_>) -> Result<StoredHostTrust, SqliteStorageUnavailable> {
    let profile_id = ConnectionProfileId::parse(
        row.get::<_, String>(0)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let security_revision_raw = row.get::<_, i64>(1).map_err(|_| SqliteStorageUnavailable)?;
    let security_revision = ConnectionSecurityRevision::new(
        u64::try_from(security_revision_raw).map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let fingerprint = HostFingerprint::parse(
        row.get::<_, String>(2)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;

    Ok(StoredHostTrust::new(
        profile_id,
        security_revision,
        fingerprint,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    use vigilshell_core::{
        connection::{
            ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
            ConnectionSecurityRevision, CredentialRef, HostFingerprint, ManagedServerId, SshRoute,
            StoredHostTrust,
        },
        human_terminal::{ConnectionProfileRepository, HostTrustRepository},
    };

    #[test]
    fn storage_is_safe_to_share_with_the_tauri_runtime() {
        fn assert_send_and_sync<T: Send + Sync>() {}

        assert_send_and_sync::<SqliteStorage>();
    }

    #[test]
    fn cloned_storage_handles_share_the_same_database_connection() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let cloned = storage.clone();
        let profile = sample_profile("profile-clone", 1, true);

        storage
            .upsert_connection_profile_fixture(&profile)
            .expect("write through original handle");

        assert_eq!(
            ConnectionProfileRepository::find(&cloned, profile.id())
                .expect("read through cloned handle"),
            Some(profile)
        );
    }

    #[test]
    fn storage_debug_output_does_not_reveal_its_database_path() {
        let path = temporary_database_path("private-database-path");
        let storage = SqliteStorage::new(&path).expect("initialize schema");
        let debug = format!("{storage:?}");

        assert_eq!(debug, "SqliteStorage { .. }");
        assert!(!debug.contains(path.to_string_lossy().as_ref()));

        std::fs::remove_file(&path).expect("remove test database");
    }

    #[test]
    fn creates_fresh_v2_schema_and_is_idempotent() {
        let path = temporary_database_path("migrations");
        if path.exists() {
            std::fs::remove_file(&path).expect("remove old temporary database");
        }

        let storage = SqliteStorage::new(&path).expect("initialize schema");
        assert_eq!(storage.schema_version().expect("read user version"), 2);
        let connection = storage.lock_connection().expect("lock fresh database");
        let columns = connection
            .prepare("PRAGMA table_info(connection_profiles)")
            .and_then(|mut statement| {
                let rows = statement.query_map((), |row| row.get::<_, String>(1))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .expect("read fresh schema columns");
        drop(connection);
        assert!(columns.iter().any(|column| column == "display_name"));
        assert!(columns.iter().any(|column| column == "record_version"));

        drop(storage);
        let reopened = SqliteStorage::new(&path).expect("reopen schema");
        assert_eq!(reopened.schema_version().expect("read user version"), 2);

        drop(reopened);
        std::fs::remove_file(&path).expect("remove test database");
    }

    #[test]
    fn migrates_v1_profiles_with_safe_v2_defaults() {
        let connection = Connection::open_in_memory().expect("open v1 database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;

                CREATE TABLE connection_profiles (
                    id TEXT PRIMARY KEY,
                    server_id TEXT NOT NULL,
                    security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                    route_host TEXT NOT NULL,
                    route_port INTEGER NOT NULL CHECK (route_port > 0),
                    route_username TEXT NOT NULL,
                    credential_ref TEXT NOT NULL,
                    enabled INTEGER NOT NULL CHECK (enabled IN (0,1))
                );

                CREATE TABLE host_trusts (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                    fingerprint TEXT NOT NULL,
                    FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                );

                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (
                    'legacy-profile',
                    'legacy-server',
                    1,
                    'legacy.example.invalid',
                    22,
                    'operator',
                    'legacy-credential',
                    1
                );

                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (
                    'legacy-empty-server',
                    '   ',
                    1,
                    'legacy-empty.example.invalid',
                    22,
                    'operator',
                    'legacy-empty-credential',
                    1
                );

                INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                VALUES ('legacy-profile', 1, 'SHA256:legacy');

                PRAGMA user_version = 1;
                "#,
            )
            .expect("create v1 schema");
        let long_server_id = "s".repeat(129);
        connection
            .execute(
                r#"
                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    "legacy-long-server",
                    long_server_id,
                    1,
                    "legacy-long.example.invalid",
                    22,
                    "operator",
                    "legacy-long-credential",
                    1,
                ],
            )
            .expect("insert long v1 server id");

        let storage = SqliteStorage::from_connection(connection).expect("migrate v1 database");
        let connection = storage.lock_connection().expect("lock migrated database");
        let (display_name, record_version) = connection
            .query_row(
                r#"
                SELECT display_name, record_version
                FROM connection_profiles
                WHERE id = 'legacy-profile'
                "#,
                (),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .expect("read migrated profile");
        let trust_count = connection
            .query_row(
                "SELECT COUNT(*) FROM host_trusts WHERE profile_id = 'legacy-profile'",
                (),
                |row| row.get::<_, i64>(0),
            )
            .expect("read migrated trust");
        let fallback_display_name = connection
            .query_row(
                r#"
                SELECT display_name
                FROM connection_profiles
                WHERE id = 'legacy-empty-server'
                "#,
                (),
                |row| row.get::<_, String>(0),
            )
            .expect("read fallback display name");
        let long_id_display_name = connection
            .query_row(
                r#"
                SELECT display_name
                FROM connection_profiles
                WHERE id = 'legacy-long-server'
                "#,
                (),
                |row| row.get::<_, String>(0),
            )
            .expect("read long id display name");
        drop(connection);

        assert_eq!(storage.schema_version().expect("read user version"), 2);
        assert_eq!(display_name, "legacy-server");
        assert_eq!(fallback_display_name, "Connection");
        assert_eq!(long_id_display_name, "Connection");
        assert_eq!(record_version, 1);
        assert_eq!(trust_count, 1);
    }

    #[test]
    fn product_create_is_insert_only_and_duplicate_is_a_conflict() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let original = sample_profile("create-conflict", 1, true);
        let duplicate = profile_with_versions(
            "create-conflict",
            "duplicate display name",
            1,
            1,
            "duplicate.example.invalid",
            false,
        );

        assert_eq!(
            ConnectionProfileMutationRepository::create(&storage, &original)
                .expect("create profile"),
            CreateConnectionProfileResult::Created
        );
        assert_eq!(
            ConnectionProfileMutationRepository::create(&storage, &duplicate)
                .expect("settle duplicate create"),
            CreateConnectionProfileResult::Conflict
        );
        assert_eq!(
            ConnectionProfileRepository::find(&storage, original.id()).expect("read original"),
            Some(original)
        );
    }

    #[test]
    fn conditional_replace_distinguishes_conflict_and_missing() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let original = sample_profile("replace-profile", 1, true);
        let replacement = profile_with_versions(
            "replace-profile",
            "renamed profile",
            2,
            1,
            "host.example.invalid",
            false,
        );
        let missing_replacement = profile_with_versions(
            "missing-replace-profile",
            "missing profile",
            2,
            1,
            "missing.example.invalid",
            true,
        );
        let version_one = ConnectionRecordVersion::new(1).expect("valid record version");

        assert_eq!(
            ConnectionProfileMutationRepository::create(&storage, &original)
                .expect("create profile"),
            CreateConnectionProfileResult::Created
        );
        assert_eq!(
            ConnectionProfileMutationRepository::replace_if_version(
                &storage,
                &replacement,
                version_one,
            )
            .expect("replace current profile"),
            ReplaceConnectionProfileResult::Replaced
        );
        assert_eq!(
            ConnectionProfileRepository::find(&storage, replacement.id())
                .expect("read replacement"),
            Some(replacement.clone())
        );
        assert_eq!(
            ConnectionProfileMutationRepository::replace_if_version(
                &storage,
                &replacement,
                version_one,
            )
            .expect("settle stale replacement"),
            ReplaceConnectionProfileResult::Conflict
        );
        assert_eq!(
            ConnectionProfileMutationRepository::replace_if_version(
                &storage,
                &missing_replacement,
                version_one,
            )
            .expect("settle missing replacement"),
            ReplaceConnectionProfileResult::NotFound
        );
    }

    #[test]
    fn conditional_replace_rejects_a_caller_chosen_version_jump() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let original = sample_profile("version-jump", 1, true);
        let jumped = profile_with_versions(
            "version-jump",
            "jumped version",
            3,
            1,
            "host.example.invalid",
            true,
        );

        ConnectionProfileMutationRepository::create(&storage, &original).expect("create profile");
        assert_eq!(
            ConnectionProfileMutationRepository::replace_if_version(
                &storage,
                &jumped,
                ConnectionRecordVersion::new(1).expect("valid record version"),
            )
            .expect_err("version jump must fail closed"),
            ConnectionMutationRepositoryError::Unavailable
        );
        assert_eq!(
            ConnectionProfileRepository::find(&storage, original.id()).expect("read original"),
            Some(original)
        );
    }

    #[test]
    fn conditional_delete_cascades_trust_and_distinguishes_outcomes() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile = sample_profile("delete-profile", 1, true);
        let trust = sample_trust(profile.id(), 1);
        let version_one = ConnectionRecordVersion::new(1).expect("valid record version");
        let version_two = ConnectionRecordVersion::new(2).expect("valid record version");

        ConnectionProfileMutationRepository::create(&storage, &profile).expect("create profile");
        storage
            .upsert_host_trust_fixture(&trust)
            .expect("insert trust fixture");

        assert_eq!(
            ConnectionProfileMutationRepository::delete_if_version(
                &storage,
                profile.id(),
                version_two,
            )
            .expect("settle stale delete"),
            DeleteConnectionProfileResult::Conflict
        );
        assert_eq!(
            ConnectionProfileMutationRepository::delete_if_version(
                &storage,
                profile.id(),
                version_one,
            )
            .expect("delete current profile"),
            DeleteConnectionProfileResult::Deleted
        );
        assert_eq!(
            ConnectionProfileRepository::find(&storage, profile.id())
                .expect("read deleted profile"),
            None
        );
        assert_eq!(
            HostTrustRepository::find(&storage, profile.id()).expect("read cascaded trust"),
            None
        );
        assert_eq!(
            ConnectionProfileMutationRepository::delete_if_version(
                &storage,
                profile.id(),
                version_one,
            )
            .expect("settle missing delete"),
            DeleteConnectionProfileResult::NotFound
        );
    }

    #[test]
    fn security_revision_replacement_leaves_existing_trust_stale() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let original = sample_profile("security-revision", 1, true);
        let trust = sample_trust(original.id(), 1);
        let replacement = profile_with_versions(
            "security-revision",
            "security revision",
            2,
            2,
            "changed.example.invalid",
            true,
        );

        ConnectionProfileMutationRepository::create(&storage, &original).expect("create profile");
        storage
            .upsert_host_trust_fixture(&trust)
            .expect("insert trust fixture");
        assert_eq!(
            ConnectionProfileMutationRepository::replace_if_version(
                &storage,
                &replacement,
                ConnectionRecordVersion::new(1).expect("valid record version"),
            )
            .expect("replace profile"),
            ReplaceConnectionProfileResult::Replaced
        );

        let observed_trust = HostTrustRepository::find(&storage, replacement.id())
            .expect("read trust")
            .expect("trust remains stored");
        assert!(!observed_trust.matches(&replacement));
    }

    #[test]
    fn host_trust_mutation_inserts_and_replaces_at_the_exact_security_revision() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile = sample_profile("trust-mutation", 3, true);
        storage
            .upsert_connection_profile_fixture(&profile)
            .expect("insert connection profile");

        let first = sample_trust_with_fingerprint(
            profile.id(),
            3,
            "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &first,
                profile.security_revision(),
            )
            .expect("store first trust"),
            StoreHostTrustResult::Stored
        );

        let replacement = sample_trust_with_fingerprint(
            profile.id(),
            3,
            "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ",
        );
        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &replacement,
                profile.security_revision(),
            )
            .expect("replace trust"),
            StoreHostTrustResult::Stored
        );
        assert_eq!(
            HostTrustRepository::find(&storage, profile.id()).expect("read replaced trust"),
            Some(replacement)
        );
    }

    #[test]
    fn host_trust_mutation_allows_display_only_record_changes() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let original = profile_with_versions(
            "display-only-trust",
            "Original name",
            1,
            4,
            "host.example.invalid",
            true,
        );
        storage
            .upsert_connection_profile_fixture(&original)
            .expect("insert connection profile");
        let renamed = profile_with_versions(
            "display-only-trust",
            "Renamed connection",
            2,
            4,
            "host.example.invalid",
            true,
        );
        storage
            .upsert_connection_profile_fixture(&renamed)
            .expect("replace display name fixture");
        let trust = sample_trust(renamed.id(), 4);

        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &trust,
                ConnectionSecurityRevision::new(4).expect("valid revision"),
            )
            .expect("store trust after display-only change"),
            StoreHostTrustResult::Stored
        );
    }

    #[test]
    fn host_trust_mutation_fails_closed_for_stale_disabled_and_missing_profiles() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let stale = sample_profile("stale-trust", 2, true);
        let disabled = sample_profile("disabled-trust", 1, false);
        storage
            .upsert_connection_profile_fixture(&stale)
            .expect("insert stale fixture");
        storage
            .upsert_connection_profile_fixture(&disabled)
            .expect("insert disabled fixture");

        let stale_trust = sample_trust(stale.id(), 1);
        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &stale_trust,
                ConnectionSecurityRevision::new(1).expect("valid revision"),
            )
            .expect("settle stale trust"),
            StoreHostTrustResult::Stale
        );
        let disabled_trust = sample_trust(disabled.id(), 1);
        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &disabled_trust,
                ConnectionSecurityRevision::new(1).expect("valid revision"),
            )
            .expect("settle disabled trust"),
            StoreHostTrustResult::Disabled
        );
        let missing = ConnectionProfileId::parse("missing-trust").expect("valid profile id");
        let missing_trust = sample_trust(&missing, 1);
        assert_eq!(
            HostTrustMutationRepository::replace_if_security_revision(
                &storage,
                &missing_trust,
                ConnectionSecurityRevision::new(1).expect("valid revision"),
            )
            .expect("settle missing trust"),
            StoreHostTrustResult::NotFound
        );

        for profile_id in [stale.id(), disabled.id(), &missing] {
            assert_eq!(
                HostTrustRepository::find(&storage, profile_id).expect("read rejected trust"),
                None
            );
        }
    }

    #[test]
    fn corrupted_record_version_fails_closed() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile = sample_profile("corrupt-record-version", 1, true);
        ConnectionProfileMutationRepository::create(&storage, &profile).expect("create profile");

        let connection = storage.lock_connection().expect("lock storage");
        connection
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .expect("disable checks for corruption fixture");
        connection
            .execute(
                "UPDATE connection_profiles SET record_version = 0 WHERE id = ?1",
                params![profile.id().as_str()],
            )
            .expect("corrupt record version");
        connection
            .execute_batch("PRAGMA ignore_check_constraints = OFF;")
            .expect("restore check constraints");
        drop(connection);

        assert_eq!(
            ConnectionProfileRepository::find(&storage, profile.id())
                .expect_err("corrupt record must fail closed"),
            ConnectionRepositoryError::Unavailable
        );
        assert_eq!(
            storage
                .list_connection_profiles()
                .expect_err("corrupt list must fail closed"),
            SqliteStorageUnavailable
        );
    }

    #[test]
    fn reads_profile_and_trust_after_upsert() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile = sample_profile("readback-demo", 1, true);
        let trust = sample_trust(profile.id(), 1);

        storage
            .upsert_connection_profile_fixture(&profile)
            .expect("insert connection profile");
        storage
            .upsert_host_trust_fixture(&trust)
            .expect("insert trust decision");

        assert_eq!(
            ConnectionProfileRepository::find(&storage, profile.id()).expect("read profile"),
            Some(profile.clone())
        );
        assert_eq!(
            HostTrustRepository::find(&storage, profile.id()).expect("read trust"),
            Some(trust.clone())
        );
    }

    #[test]
    fn returns_missing_records_as_none() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let missing = ConnectionProfileId::parse("missing").expect("valid profile id");

        assert_eq!(
            ConnectionProfileRepository::find(&storage, &missing).expect("read missing profile"),
            None
        );
        assert_eq!(
            HostTrustRepository::find(&storage, &missing).expect("read missing trust"),
            None
        );
    }

    #[test]
    fn lists_connection_profiles_in_stable_id_order() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile_c = sample_profile("charlie", 1, true);
        let profile_a = sample_profile("alpha", 1, true);
        let profile_b = sample_profile("bravo", 1, false);

        for profile in [&profile_c, &profile_a, &profile_b] {
            storage
                .upsert_connection_profile_fixture(profile)
                .expect("insert connection profile");
        }

        let observed = storage
            .list_connection_profiles()
            .expect("list connection profiles");
        let observed_ids = observed
            .iter()
            .map(|profile| profile.id().as_str())
            .collect::<Vec<_>>();

        assert_eq!(observed_ids, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn corrupted_profile_in_list_fails_closed() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    display_name,
                    record_version,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    "bad-list-profile",
                    "server",
                    "bad list profile",
                    1,
                    1,
                    "",
                    22,
                    "operator",
                    "cred",
                    1,
                ],
            )
            .expect("seed bad row");

        assert_eq!(
            storage
                .list_connection_profiles()
                .expect_err("bad row should fail closed"),
            SqliteStorageUnavailable
        );
    }

    #[test]
    fn keeps_stale_trust_records() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile_v1 = sample_profile("stale-trust", 1, true);
        let trust_v1 = sample_trust(profile_v1.id(), 1);
        let profile_v2 = ConnectionProfile::new(
            profile_v1.id().clone(),
            ManagedServerId::parse("server").expect("valid managed server id"),
            ConnectionDisplayName::parse("stale trust").expect("valid display name"),
            ConnectionRecordVersion::new(2).expect("valid record version"),
            ConnectionSecurityRevision::new(2).expect("valid revision"),
            SshRoute::new("host.example.invalid", 22, "operator").expect("valid route"),
            CredentialRef::parse("cred").expect("valid credential ref"),
            true,
        );

        storage
            .upsert_connection_profile_fixture(&profile_v1)
            .expect("insert v1 profile");
        storage
            .upsert_host_trust_fixture(&trust_v1)
            .expect("insert v1 trust");
        storage
            .upsert_connection_profile_fixture(&profile_v2)
            .expect("insert v2 profile");

        let observed = HostTrustRepository::find(&storage, profile_v1.id())
            .expect("read trust record")
            .expect("trust exists");
        assert_eq!(
            observed.security_revision(),
            ConnectionSecurityRevision::new(1).expect("valid revision")
        );
        assert_eq!(observed.fingerprint(), trust_v1.fingerprint());
    }

    #[test]
    fn corrupted_profile_records_fail_closed() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO connection_profiles(
                    id,
                    server_id,
                    display_name,
                    record_version,
                    security_revision,
                    route_host,
                    route_port,
                    route_username,
                    credential_ref,
                    enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    "bad-profile",
                    "server",
                    "bad profile",
                    1,
                    1,
                    "",
                    22,
                    "operator",
                    "cred",
                    1,
                ],
            )
            .expect("seed bad row");

        let result = ConnectionProfileRepository::find(
            &storage,
            &ConnectionProfileId::parse("bad-profile").expect("valid profile id"),
        )
        .expect_err("bad row should fail closed");
        assert!(matches!(
            result,
            vigilshell_core::human_terminal::ConnectionRepositoryError::Unavailable
        ));
    }

    #[test]
    fn corrupted_trust_records_fail_closed_and_do_not_leak_details() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        let profile = sample_profile("trusted", 1, true);
        storage
            .upsert_connection_profile_fixture(&profile)
            .expect("seed profile");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                VALUES (?1, ?2, ?3)
                "#,
                params!["trusted", 1, ""],
            )
            .expect("seed corrupted trust");

        let result = HostTrustRepository::find(&storage, profile.id())
            .expect_err("bad trust should fail closed");
        let display = format!("{}", result);
        let debug = format!("{:?}", result);

        assert_eq!(
            result,
            vigilshell_core::human_terminal::HostTrustRepositoryError::Unavailable
        );
        assert!(!display.contains("trusted"));
        assert!(!display.contains("host.example.invalid"));
        assert!(!display.contains("operator"));
        assert!(!display.contains("cred"));
        assert!(!debug.contains("trusted"));
        assert!(!debug.contains("host.example.invalid"));
        assert!(!debug.contains("operator"));
        assert!(!debug.contains("cred"));
    }

    #[test]
    fn shutdown_flush_preserves_committed_state() {
        let database_path = temporary_database_path("vigilshell-shutdown-flush");
        let profile = sample_profile("shutdown-flush", 1, true);

        {
            let storage = SqliteStorage::new(&database_path).expect("open file storage");
            storage
                .upsert_connection_profile_fixture(&profile)
                .expect("commit profile");
            storage.flush_for_shutdown().expect("flush storage");
        }

        let reopened = SqliteStorage::new(&database_path).expect("reopen file storage");
        let observed = ConnectionProfileRepository::find(&reopened, profile.id())
            .expect("read profile")
            .expect("profile should persist");
        assert_eq!(observed, profile);

        fs::remove_file(database_path).expect("remove temporary database");
    }

    #[test]
    fn shutdown_flush_fails_closed_with_an_open_transaction() {
        let storage = SqliteStorage::open_in_memory().expect("open in-memory storage");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute_batch("BEGIN IMMEDIATE")
            .expect("open transaction");

        assert_eq!(storage.flush_for_shutdown(), Err(SqliteStorageUnavailable));

        storage
            .lock_connection()
            .expect("lock storage")
            .execute_batch("ROLLBACK")
            .expect("rollback transaction");
    }

    fn sample_profile(id: &str, revision: u64, enabled: bool) -> ConnectionProfile {
        profile_with_versions(
            id,
            &format!("{id} display name"),
            1,
            revision,
            "host.example.invalid",
            enabled,
        )
    }

    fn profile_with_versions(
        id: &str,
        display_name: &str,
        record_version: u64,
        security_revision: u64,
        host: &str,
        enabled: bool,
    ) -> ConnectionProfile {
        ConnectionProfile::new(
            ConnectionProfileId::parse(id).expect("valid profile id"),
            ManagedServerId::parse("server").expect("valid managed server id"),
            ConnectionDisplayName::parse(display_name).expect("valid display name"),
            ConnectionRecordVersion::new(record_version).expect("valid record version"),
            ConnectionSecurityRevision::new(security_revision).expect("valid security revision"),
            SshRoute::new(host, 22, "operator").expect("valid route"),
            CredentialRef::parse("cred").expect("valid credential ref"),
            enabled,
        )
    }

    fn sample_trust(profile_id: &ConnectionProfileId, revision: u64) -> StoredHostTrust {
        sample_trust_with_fingerprint(
            profile_id,
            revision,
            "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
    }

    fn sample_trust_with_fingerprint(
        profile_id: &ConnectionProfileId,
        revision: u64,
        fingerprint: &str,
    ) -> StoredHostTrust {
        StoredHostTrust::new(
            profile_id.clone(),
            ConnectionSecurityRevision::new(revision).expect("valid revision"),
            HostFingerprint::parse(fingerprint).expect("valid fingerprint"),
        )
    }

    fn temporary_database_path(prefix: &str) -> PathBuf {
        let mut path = env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_millis();
        path.push(format!("{prefix}-{now}-{}.db", current_process_id()));
        path
    }

    fn current_process_id() -> u32 {
        std::process::id()
    }
}
