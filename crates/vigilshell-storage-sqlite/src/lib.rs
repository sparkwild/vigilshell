use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, Row, Transaction, TransactionBehavior, params};
use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, HostFingerprint, ManagedServerId, SshRoute,
        StoredHostTrust,
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
    server_workspace::{
        AddConnectionToServerResult, CreateProjectResult, CreateServerWithConnectionResult,
        DeleteManagedServerResult, DeleteProjectResult, ManagedServer, ManagedServerDisplayName,
        ManagedServerRecordVersion, Project, ProjectDisplayName, ProjectId, ProjectRecordVersion,
        ReplaceManagedServerResult, ReplaceProjectResult, ServerWorkspaceMutationRepository,
        ServerWorkspaceReadRepository, ServerWorkspaceRepositoryError,
    },
};

const FRESH_SCHEMA_VERSION: i32 = 1;
const LEGACY_SCHEMA_VERSION: i32 = 3;
const STORAGE_APPLICATION_ID: i64 = 0x5647_5332;
const STORAGE_GENERATION: &str = "vigilshell-local-storage-v2";
const MINIMUM_SQLITE_VERSION_NUMBER: i32 = 3_051_003;
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);

const FRESH_TABLES: &[&str] = &[
    "connection_profiles",
    "host_trusts",
    "managed_servers",
    "projects",
    "schema_metadata",
    "storage_runtime_state",
];

/// 打开或迁移应用 SQLite 时可公开的稳定失败分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOpenError {
    FilesystemUnsafe,
    UnsupportedLegacySchema,
    SchemaAhead,
    SchemaMismatch,
    IntegrityFailed,
    Unavailable,
}

impl fmt::Display for StorageOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FilesystemUnsafe => "storage filesystem entry is unsafe",
            Self::UnsupportedLegacySchema => "storage legacy schema is unsupported",
            Self::SchemaAhead => "storage schema is newer than this application",
            Self::SchemaMismatch => "storage schema does not match the application",
            Self::IntegrityFailed => "storage integrity verification failed",
            Self::Unavailable => "storage is unavailable",
        })
    }
}

impl Error for StorageOpenError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteStorageUnavailable;

impl fmt::Display for SqliteStorageUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("storage is unavailable")
    }
}

impl Error for SqliteStorageUnavailable {}

#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionProfileTrustSnapshot {
    profile: ConnectionProfile,
    trust: Option<StoredHostTrust>,
}

impl ConnectionProfileTrustSnapshot {
    fn new(profile: ConnectionProfile, trust: Option<StoredHostTrust>) -> Self {
        Self { profile, trust }
    }

    pub fn into_parts(self) -> (ConnectionProfile, Option<StoredHostTrust>) {
        (self.profile, self.trust)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HumanWorkbenchSnapshot {
    projects: Vec<Project>,
    servers: Vec<ManagedServer>,
    connections: Vec<ConnectionProfileTrustSnapshot>,
}

impl HumanWorkbenchSnapshot {
    pub fn into_parts(
        self,
    ) -> (
        Vec<Project>,
        Vec<ManagedServer>,
        Vec<ConnectionProfileTrustSnapshot>,
    ) {
        (self.projects, self.servers, self.connections)
    }
}

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
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageOpenError> {
        let mut connection = Connection::open(path).map_err(|_| StorageOpenError::Unavailable)?;
        initialize_storage(&mut connection, true)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn open_in_memory() -> Result<Self, StorageOpenError> {
        let mut connection =
            Connection::open_in_memory().map_err(|_| StorageOpenError::Unavailable)?;
        initialize_storage(&mut connection, false)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn from_connection(mut connection: Connection) -> Result<Self, StorageOpenError> {
        initialize_storage(&mut connection, false)?;
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
            .execute_batch("PRAGMA optimize")
            .map_err(|_| SqliteStorageUnavailable)?;
        let (busy, _, _): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| SqliteStorageUnavailable)?;
        if busy != 0 {
            return Err(SqliteStorageUnavailable);
        }
        mark_clean_shutdown(&connection).map_err(|_| SqliteStorageUnavailable)?;
        let (busy, _, _): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| SqliteStorageUnavailable)?;
        if busy != 0 {
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
        read_all_connection_profiles(&connection)
    }

    pub fn load_human_workbench_snapshot(
        &self,
    ) -> Result<HumanWorkbenchSnapshot, SqliteStorageUnavailable> {
        self.load_human_workbench_snapshot_with_hook(|| {})
    }

    fn load_human_workbench_snapshot_with_hook<F>(
        &self,
        after_profiles: F,
    ) -> Result<HumanWorkbenchSnapshot, SqliteStorageUnavailable>
    where
        F: FnOnce(),
    {
        let mut connection = self.lock_connection()?;
        let transaction = connection
            .transaction()
            .map_err(|_| SqliteStorageUnavailable)?;
        let projects = read_all_projects(&transaction)?;
        let servers = read_all_servers(&transaction)?;
        let profiles = read_all_connection_profiles(&transaction)?;
        after_profiles();
        let mut trusts = read_all_host_trusts(&transaction)?;
        let connections = profiles
            .into_iter()
            .map(|profile| {
                let trust = trusts.remove(profile.id());
                ConnectionProfileTrustSnapshot::new(profile, trust)
            })
            .collect();
        if !trusts.is_empty() {
            return Err(SqliteStorageUnavailable);
        }
        transaction.commit().map_err(|_| SqliteStorageUnavailable)?;
        Ok(HumanWorkbenchSnapshot {
            projects,
            servers,
            connections,
        })
    }

    pub fn load_connection_editor_snapshot(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfileTrustSnapshot>, SqliteStorageUnavailable> {
        self.load_connection_editor_snapshot_with_hook(profile_id, || {})
    }

    fn load_connection_editor_snapshot_with_hook<F>(
        &self,
        profile_id: &ConnectionProfileId,
        after_profile: F,
    ) -> Result<Option<ConnectionProfileTrustSnapshot>, SqliteStorageUnavailable>
    where
        F: FnOnce(),
    {
        let mut connection = self.lock_connection()?;
        let transaction = connection
            .transaction()
            .map_err(|_| SqliteStorageUnavailable)?;
        let profile = read_connection_profile(&transaction, profile_id)?;
        let snapshot = match profile {
            Some(profile) => {
                after_profile();
                let trust = read_host_trust(&transaction, profile_id)?;
                Some(ConnectionProfileTrustSnapshot::new(profile, trust))
            }
            None => None,
        };
        transaction.commit().map_err(|_| SqliteStorageUnavailable)?;
        Ok(snapshot)
    }

    /// 仅供 debug/test fixture 写入完整 Connection 快照。
    #[cfg(any(test, debug_assertions))]
    pub fn upsert_connection_profile_fixture(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<(), SqliteStorageUnavailable> {
        let mut connection = self.lock_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| SqliteStorageUnavailable)?;
        transaction
            .execute(
                r#"
                INSERT INTO managed_servers(id, project_id, display_name, record_version)
                VALUES (?1, NULL, ?2, 1)
                ON CONFLICT(id) DO NOTHING
                "#,
                params![
                    profile.server_id().as_str(),
                    profile.display_name().as_str()
                ],
            )
            .map_err(|_| SqliteStorageUnavailable)?;
        transaction
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
            .map_err(|_| SqliteStorageUnavailable)?;
        transaction.commit().map_err(|_| SqliteStorageUnavailable)
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

    #[cfg(any(test, debug_assertions))]
    pub fn make_host_trust_reads_unavailable_fixture(
        &self,
    ) -> Result<(), SqliteStorageUnavailable> {
        self.lock_connection()?
            .execute_batch("DROP TABLE host_trusts;")
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
        if current_version > LEGACY_SCHEMA_VERSION {
            return Err(SqliteStorageUnavailable);
        }
        if current_version == LEGACY_SCHEMA_VERSION {
            return Ok(());
        }

        let transaction = connection
            .transaction()
            .map_err(|_| SqliteStorageUnavailable)?;

        for version in (current_version + 1)..=LEGACY_SCHEMA_VERSION {
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
                3 => {
                    validate_profiles_and_trusts(&transaction)?;
                    transaction
                        .execute_batch(
                            r#"
                            CREATE TABLE projects (
                                id TEXT PRIMARY KEY NOT NULL
                                    CHECK (length(trim(id)) > 0),
                                display_name TEXT NOT NULL
                                    CHECK (
                                        length(trim(display_name)) > 0
                                        AND length(CAST(trim(display_name) AS BLOB)) <= 128
                                    ),
                                record_version INTEGER NOT NULL
                                    CHECK (record_version > 0)
                            );

                            CREATE TABLE managed_servers (
                                id TEXT PRIMARY KEY NOT NULL
                                    CHECK (length(trim(id)) > 0),
                                project_id TEXT,
                                display_name TEXT NOT NULL
                                    CHECK (
                                        length(trim(display_name)) > 0
                                        AND length(CAST(trim(display_name) AS BLOB)) <= 128
                                    ),
                                record_version INTEGER NOT NULL
                                    CHECK (record_version > 0),
                                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE SET NULL
                            );

                            INSERT INTO managed_servers(id, project_id, display_name, record_version)
                            SELECT
                                profile.server_id,
                                NULL,
                                trim(profile.display_name),
                                1
                            FROM connection_profiles AS profile
                            WHERE profile.id = (
                                SELECT MIN(candidate.id)
                                FROM connection_profiles AS candidate
                                WHERE candidate.server_id = profile.server_id
                            )
                            ORDER BY profile.server_id ASC;

                            CREATE TABLE host_trusts_v3_backup (
                                profile_id TEXT PRIMARY KEY NOT NULL,
                                security_revision INTEGER NOT NULL,
                                fingerprint TEXT NOT NULL
                            );

                            INSERT INTO host_trusts_v3_backup(
                                profile_id,
                                security_revision,
                                fingerprint
                            )
                            SELECT profile_id, security_revision, fingerprint
                            FROM host_trusts;

                            DROP TABLE host_trusts;

                            CREATE TABLE connection_profiles_v3 (
                                id TEXT PRIMARY KEY NOT NULL,
                                server_id TEXT NOT NULL,
                                display_name TEXT NOT NULL
                                    CHECK (
                                        length(trim(display_name)) > 0
                                        AND length(CAST(trim(display_name) AS BLOB)) <= 128
                                    ),
                                record_version INTEGER NOT NULL CHECK (record_version > 0),
                                security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                                route_host TEXT NOT NULL,
                                route_port INTEGER NOT NULL CHECK (route_port > 0),
                                route_username TEXT NOT NULL,
                                credential_ref TEXT NOT NULL,
                                enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
                                FOREIGN KEY(server_id) REFERENCES managed_servers(id) ON DELETE RESTRICT
                            );

                            INSERT INTO connection_profiles_v3(
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
                            FROM connection_profiles;

                            DROP TABLE connection_profiles;
                            ALTER TABLE connection_profiles_v3 RENAME TO connection_profiles;

                            CREATE INDEX connection_profiles_server_id_id
                                ON connection_profiles(server_id, id);

                            CREATE TABLE host_trusts (
                                profile_id TEXT PRIMARY KEY NOT NULL,
                                security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                                fingerprint TEXT NOT NULL,
                                FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                            );

                            INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                            SELECT profile_id, security_revision, fingerprint
                            FROM host_trusts_v3_backup;

                            DROP TABLE host_trusts_v3_backup;
                            PRAGMA user_version = 3;
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

fn initialize_storage(
    connection: &mut Connection,
    persistent: bool,
) -> Result<(), StorageOpenError> {
    if rusqlite::version_number() < MINIMUM_SQLITE_VERSION_NUMBER {
        return Err(StorageOpenError::SchemaMismatch);
    }
    configure_preflight_connection(connection)?;
    let version =
        SqliteStorage::schema_version_for(connection).map_err(|_| StorageOpenError::Unavailable)?;
    let tables = user_tables(connection)?;
    let application_id = connection
        .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    let current_generation = tables.contains("schema_metadata") || application_id != 0;

    if current_generation {
        if application_id != STORAGE_APPLICATION_ID {
            return Err(StorageOpenError::SchemaMismatch);
        }
        if version > FRESH_SCHEMA_VERSION {
            return Err(StorageOpenError::SchemaAhead);
        }
        validate_fresh_schema(connection, &tables)?;
    } else {
        let migrated_from = if tables.is_empty() && version == 0 {
            None
        } else {
            if version > LEGACY_SCHEMA_VERSION {
                return Err(StorageOpenError::SchemaAhead);
            }
            if !legacy_tables_match(version, &tables) {
                return Err(StorageOpenError::UnsupportedLegacySchema);
            }
            validate_quick_integrity(connection)?;
            Some(version)
        };
        connection
            .pragma_update(None, "auto_vacuum", "INCREMENTAL")
            .map_err(|_| StorageOpenError::Unavailable)?;
        if migrated_from.is_some() {
            connection
                .execute_batch("VACUUM")
                .map_err(|_| StorageOpenError::Unavailable)?;
        }
        SqliteStorage::run_migrations(connection)
            .map_err(|_| StorageOpenError::UnsupportedLegacySchema)?;
        install_fresh_generation(connection, migrated_from)?;
        validate_fresh_schema(connection, &user_tables(connection)?)?;
    }

    configure_runtime_connection(connection, persistent)?;
    validate_runtime_pragmas(connection, persistent)?;
    validate_quick_integrity(connection)?;
    begin_runtime_session(connection)?;
    Ok(())
}

fn configure_preflight_connection(connection: &Connection) -> Result<(), StorageOpenError> {
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|_| StorageOpenError::Unavailable)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|_| StorageOpenError::Unavailable)?;
    connection
        .pragma_update(None, "trusted_schema", false)
        .map_err(|_| StorageOpenError::Unavailable)?;
    connection
        .pragma_update(None, "cell_size_check", true)
        .map_err(|_| StorageOpenError::Unavailable)
}

fn configure_runtime_connection(
    connection: &Connection,
    persistent: bool,
) -> Result<(), StorageOpenError> {
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|_| StorageOpenError::Unavailable)?;
    if persistent {
        let journal_mode = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|_| StorageOpenError::Unavailable)?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StorageOpenError::SchemaMismatch);
        }
    }
    Ok(())
}

fn validate_runtime_pragmas(
    connection: &Connection,
    persistent: bool,
) -> Result<(), StorageOpenError> {
    let foreign_keys = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    let synchronous = connection
        .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    let trusted_schema = connection
        .query_row("PRAGMA trusted_schema", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    let cell_size_check = connection
        .query_row("PRAGMA cell_size_check", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    let auto_vacuum = connection
        .query_row("PRAGMA auto_vacuum", [], |row| row.get::<_, i64>(0))
        .map_err(|_| StorageOpenError::Unavailable)?;
    if foreign_keys != 1
        || synchronous != 2
        || trusted_schema != 0
        || cell_size_check != 1
        || auto_vacuum != 2
    {
        return Err(StorageOpenError::SchemaMismatch);
    }
    if persistent {
        let journal_mode = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .map_err(|_| StorageOpenError::Unavailable)?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StorageOpenError::SchemaMismatch);
        }
    }
    Ok(())
}

fn install_fresh_generation(
    connection: &mut Connection,
    migrated_from: Option<i32>,
) -> Result<(), StorageOpenError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .execute_batch(
            r#"
            CREATE TABLE schema_metadata (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                storage_generation TEXT NOT NULL,
                created_build TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
                migrated_from_user_version INTEGER
            );

            CREATE TABLE storage_runtime_state (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                clean_shutdown INTEGER NOT NULL CHECK (clean_shutdown IN (0, 1)),
                last_started_at_ms INTEGER NOT NULL CHECK (last_started_at_ms >= 0),
                last_clean_shutdown_at_ms INTEGER,
                revision INTEGER NOT NULL CHECK (revision > 0)
            );
            "#,
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .execute(
            "INSERT INTO schema_metadata(
                singleton_id, storage_generation, created_build, created_at_ms,
                migrated_from_user_version
             ) VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                STORAGE_GENERATION,
                env!("CARGO_PKG_VERSION"),
                current_unix_millis(),
                migrated_from
            ],
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .execute(
            "INSERT INTO storage_runtime_state(
                singleton_id, clean_shutdown, last_started_at_ms,
                last_clean_shutdown_at_ms, revision
             ) VALUES (1, 1, 0, NULL, 1)",
            [],
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .pragma_update(None, "application_id", STORAGE_APPLICATION_ID)
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .pragma_update(None, "user_version", FRESH_SCHEMA_VERSION)
        .map_err(|_| StorageOpenError::Unavailable)?;
    transaction
        .commit()
        .map_err(|_| StorageOpenError::Unavailable)
}

fn validate_fresh_schema(
    connection: &Connection,
    tables: &BTreeSet<String>,
) -> Result<(), StorageOpenError> {
    let expected = FRESH_TABLES
        .iter()
        .map(|table| (*table).to_owned())
        .collect::<BTreeSet<_>>();
    if tables != &expected {
        return Err(StorageOpenError::SchemaMismatch);
    }
    let version =
        SqliteStorage::schema_version_for(connection).map_err(|_| StorageOpenError::Unavailable)?;
    if version != FRESH_SCHEMA_VERSION {
        return Err(if version > FRESH_SCHEMA_VERSION {
            StorageOpenError::SchemaAhead
        } else {
            StorageOpenError::SchemaMismatch
        });
    }
    let generation = connection
        .query_row(
            "SELECT storage_generation FROM schema_metadata WHERE singleton_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| StorageOpenError::SchemaMismatch)?;
    if generation != STORAGE_GENERATION {
        return Err(StorageOpenError::SchemaMismatch);
    }
    let metadata_rows = connection
        .query_row("SELECT COUNT(*) FROM schema_metadata", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| StorageOpenError::SchemaMismatch)?;
    let runtime_rows = connection
        .query_row("SELECT COUNT(*) FROM storage_runtime_state", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| StorageOpenError::SchemaMismatch)?;
    if metadata_rows != 1 || runtime_rows != 1 {
        return Err(StorageOpenError::SchemaMismatch);
    }
    Ok(())
}

fn validate_quick_integrity(connection: &Connection) -> Result<(), StorageOpenError> {
    let quick = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|_| StorageOpenError::IntegrityFailed)?;
    if !quick.eq_ignore_ascii_case("ok") {
        return Err(StorageOpenError::IntegrityFailed);
    }
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| StorageOpenError::IntegrityFailed)?;
    if statement
        .query([])
        .map_err(|_| StorageOpenError::IntegrityFailed)?
        .next()
        .map_err(|_| StorageOpenError::IntegrityFailed)?
        .is_some()
    {
        return Err(StorageOpenError::IntegrityFailed);
    }
    Ok(())
}

fn validate_full_integrity(connection: &Connection) -> Result<(), StorageOpenError> {
    let integrity = connection
        .query_row("PRAGMA integrity_check(1)", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| StorageOpenError::IntegrityFailed)?;
    if integrity.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(StorageOpenError::IntegrityFailed)
    }
}

fn begin_runtime_session(connection: &Connection) -> Result<(), StorageOpenError> {
    let clean_shutdown = connection
        .query_row(
            "SELECT clean_shutdown FROM storage_runtime_state WHERE singleton_id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| StorageOpenError::SchemaMismatch)?;
    if clean_shutdown != 1 {
        validate_full_integrity(connection)?;
    }
    let changed = connection
        .execute(
            "UPDATE storage_runtime_state
             SET clean_shutdown = 0, last_started_at_ms = ?1, revision = revision + 1
             WHERE singleton_id = 1",
            [current_unix_millis()],
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    if changed != 1 {
        return Err(StorageOpenError::SchemaMismatch);
    }
    Ok(())
}

fn mark_clean_shutdown(connection: &Connection) -> Result<(), StorageOpenError> {
    let changed = connection
        .execute(
            "UPDATE storage_runtime_state
             SET clean_shutdown = 1, last_clean_shutdown_at_ms = ?1, revision = revision + 1
             WHERE singleton_id = 1",
            [current_unix_millis()],
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    if changed != 1 {
        return Err(StorageOpenError::SchemaMismatch);
    }
    Ok(())
}

fn user_tables(connection: &Connection) -> Result<BTreeSet<String>, StorageOpenError> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name ASC",
        )
        .map_err(|_| StorageOpenError::Unavailable)?;
    let mut rows = statement
        .query([])
        .map_err(|_| StorageOpenError::Unavailable)?;
    let mut tables = BTreeSet::new();
    while let Some(row) = rows.next().map_err(|_| StorageOpenError::Unavailable)? {
        tables.insert(
            row.get::<_, String>(0)
                .map_err(|_| StorageOpenError::Unavailable)?,
        );
    }
    Ok(tables)
}

fn legacy_tables_match(version: i32, tables: &BTreeSet<String>) -> bool {
    let expected = match version {
        1 | 2 => ["connection_profiles", "host_trusts"].as_slice(),
        3 => [
            "connection_profiles",
            "host_trusts",
            "managed_servers",
            "projects",
        ]
        .as_slice(),
        _ => return false,
    };
    tables
        == &expected
            .iter()
            .map(|table| (*table).to_owned())
            .collect::<BTreeSet<_>>()
}

fn current_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

impl ConnectionProfileRepository for SqliteStorage {
    fn find(
        &self,
        profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ConnectionRepositoryError::Unavailable)?;
        read_connection_profile(&connection, profile_id)
            .map_err(|_| ConnectionRepositoryError::Unavailable)
    }
}

impl ServerWorkspaceReadRepository for SqliteStorage {
    fn find_project(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<Project>, ServerWorkspaceRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, display_name, record_version
                FROM projects
                WHERE id = ?1
                "#,
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut rows = statement
            .query(params![project_id.as_str()])
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        match rows
            .next()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?
        {
            Some(row) => decode_project(row)
                .map(Some)
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable),
            None => Ok(None),
        }
    }

    fn find_server(
        &self,
        server_id: &ManagedServerId,
    ) -> Result<Option<ManagedServer>, ServerWorkspaceRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, project_id, display_name, record_version
                FROM managed_servers
                WHERE id = ?1
                "#,
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut rows = statement
            .query(params![server_id.as_str()])
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        match rows
            .next()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?
        {
            Some(row) => decode_managed_server(row)
                .map(Some)
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable),
            None => Ok(None),
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>, ServerWorkspaceRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        read_all_projects(&connection).map_err(|_| ServerWorkspaceRepositoryError::Unavailable)
    }

    fn list_servers(&self) -> Result<Vec<ManagedServer>, ServerWorkspaceRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        read_all_servers(&connection).map_err(|_| ServerWorkspaceRepositoryError::Unavailable)
    }

    fn list_connection_profiles(
        &self,
        server_id: &ManagedServerId,
    ) -> Result<Vec<ConnectionProfile>, ServerWorkspaceRepositoryError> {
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
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
                WHERE server_id = ?1
                ORDER BY display_name COLLATE NOCASE ASC, id ASC
                "#,
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut rows = statement
            .query(params![server_id.as_str()])
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let mut profiles = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?
        {
            profiles.push(
                decode_connection_profile(row)
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?,
            );
        }
        Ok(profiles)
    }
}

impl ServerWorkspaceMutationRepository for SqliteStorage {
    fn create_project(
        &self,
        project: &Project,
    ) -> Result<CreateProjectResult, ServerWorkspaceRepositoryError> {
        if project.record_version() != ProjectRecordVersion::INITIAL {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let version = workspace_counter(project.record_version().get())?;
        let connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        match connection.execute(
            r#"
            INSERT INTO projects(id, display_name, record_version)
            VALUES (?1, ?2, ?3)
            "#,
            params![
                project.id().as_str(),
                project.display_name().as_str(),
                version
            ],
        ) {
            Ok(1) => Ok(CreateProjectResult::Created),
            Ok(_) => Err(ServerWorkspaceRepositoryError::Unavailable),
            Err(error) if is_primary_key_conflict(&error) => Ok(CreateProjectResult::Conflict),
            Err(_) => Err(ServerWorkspaceRepositoryError::Unavailable),
        }
    }

    fn replace_project_if_version(
        &self,
        project: &Project,
        expected_record_version: ProjectRecordVersion,
    ) -> Result<ReplaceProjectResult, ServerWorkspaceRepositoryError> {
        if expected_record_version.checked_next() != Some(project.record_version()) {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let version = workspace_counter(project.record_version().get())?;
        let expected = workspace_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let replaced = transaction
            .execute(
                r#"
                UPDATE projects
                SET display_name = ?2, record_version = ?3
                WHERE id = ?1 AND record_version = ?4
                "#,
                params![
                    project.id().as_str(),
                    project.display_name().as_str(),
                    version,
                    expected
                ],
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let outcome = match replaced {
            1 => Ok(ReplaceProjectResult::Replaced),
            0 if project_exists(&transaction, project.id())? => Ok(ReplaceProjectResult::Conflict),
            0 => Ok(ReplaceProjectResult::NotFound),
            _ => Err(ServerWorkspaceRepositoryError::Unavailable),
        }?;
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(outcome)
    }

    fn delete_project_and_unassign_servers(
        &self,
        project_id: &ProjectId,
        expected_record_version: ProjectRecordVersion,
    ) -> Result<DeleteProjectResult, ServerWorkspaceRepositoryError> {
        let expected = workspace_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let current = transaction.query_row(
            "SELECT record_version FROM projects WHERE id = ?1",
            params![project_id.as_str()],
            |row| row.get::<_, i64>(0),
        );
        match current {
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                transaction
                    .rollback()
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
                return Ok(DeleteProjectResult::NotFound);
            }
            Err(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
            Ok(version) if version != expected => {
                transaction
                    .rollback()
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
                return Ok(DeleteProjectResult::Conflict);
            }
            Ok(_) => {}
        }
        let invalid_server_version: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM managed_servers
                    WHERE project_id = ?1
                        AND (typeof(record_version) != 'integer' OR record_version <= 0)
                )
                "#,
                params![project_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if invalid_server_version {
            transaction
                .rollback()
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let exhausted: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM managed_servers
                    WHERE project_id = ?1 AND record_version = ?2
                )
                "#,
                params![project_id.as_str(), i64::MAX],
                |row| row.get(0),
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if exhausted {
            transaction
                .rollback()
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
            return Ok(DeleteProjectResult::VersionExhausted);
        }
        let unassigned = transaction
            .execute(
                r#"
                UPDATE managed_servers
                SET project_id = NULL, record_version = record_version + 1
                WHERE project_id = ?1
                "#,
                params![project_id.as_str()],
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let deleted = transaction
            .execute(
                "DELETE FROM projects WHERE id = ?1 AND record_version = ?2",
                params![project_id.as_str(), expected],
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if deleted != 1 {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(DeleteProjectResult::Deleted {
            unassigned_servers: u64::try_from(unassigned)
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?,
        })
    }

    fn create_server_with_connection(
        &self,
        server: &ManagedServer,
        profile: &ConnectionProfile,
    ) -> Result<CreateServerWithConnectionResult, ServerWorkspaceRepositoryError> {
        if server.record_version() != ManagedServerRecordVersion::INITIAL
            || profile.record_version() != ConnectionRecordVersion::INITIAL
            || profile.security_revision() != ConnectionSecurityRevision::INITIAL
            || profile.server_id() != server.id()
        {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let server_version = workspace_counter(server.record_version().get())?;
        let profile_version = workspace_counter(profile.record_version().get())?;
        let security_revision = workspace_counter(profile.security_revision().get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if let Some(project_id) = server.project_id()
            && !project_exists(&transaction, project_id)?
        {
            transaction
                .rollback()
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
            return Ok(CreateServerWithConnectionResult::ProjectNotFound);
        }
        match insert_server(&transaction, server, server_version) {
            Ok(1) => {}
            Ok(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
            Err(error) if is_primary_key_conflict(&error) => {
                transaction
                    .rollback()
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
                return Ok(CreateServerWithConnectionResult::ServerConflict);
            }
            Err(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
        }
        match insert_profile(&transaction, profile, profile_version, security_revision) {
            Ok(1) => {}
            Ok(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
            Err(error) if is_primary_key_conflict(&error) => {
                transaction
                    .rollback()
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
                return Ok(CreateServerWithConnectionResult::ProfileConflict);
            }
            Err(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
        }
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(CreateServerWithConnectionResult::Created)
    }

    fn add_connection_to_server(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<AddConnectionToServerResult, ServerWorkspaceRepositoryError> {
        if profile.record_version() != ConnectionRecordVersion::INITIAL
            || profile.security_revision() != ConnectionSecurityRevision::INITIAL
        {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let record_version = workspace_counter(profile.record_version().get())?;
        let security_revision = workspace_counter(profile.security_revision().get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if !server_exists(&transaction, profile.server_id())? {
            transaction
                .rollback()
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
            return Ok(AddConnectionToServerResult::ServerNotFound);
        }
        match insert_profile(&transaction, profile, record_version, security_revision) {
            Ok(1) => {}
            Ok(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
            Err(error) if is_primary_key_conflict(&error) => {
                transaction
                    .rollback()
                    .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
                return Ok(AddConnectionToServerResult::ProfileConflict);
            }
            Err(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
        }
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(AddConnectionToServerResult::Created)
    }

    fn replace_server_if_version(
        &self,
        server: &ManagedServer,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Result<ReplaceManagedServerResult, ServerWorkspaceRepositoryError> {
        if expected_record_version.checked_next() != Some(server.record_version()) {
            return Err(ServerWorkspaceRepositoryError::Unavailable);
        }
        let version = workspace_counter(server.record_version().get())?;
        let expected = workspace_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        if let Some(project_id) = server.project_id()
            && !project_exists(&transaction, project_id)?
        {
            transaction
                .rollback()
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
            return Ok(ReplaceManagedServerResult::ProjectNotFound);
        }
        let replaced = transaction
            .execute(
                r#"
                UPDATE managed_servers
                SET project_id = ?2, display_name = ?3, record_version = ?4
                WHERE id = ?1 AND record_version = ?5
                "#,
                params![
                    server.id().as_str(),
                    server.project_id().map(ProjectId::as_str),
                    server.display_name().as_str(),
                    version,
                    expected,
                ],
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let outcome = match replaced {
            1 => ReplaceManagedServerResult::Replaced,
            0 if server_exists(&transaction, server.id())? => ReplaceManagedServerResult::Conflict,
            0 => ReplaceManagedServerResult::NotFound,
            _ => return Err(ServerWorkspaceRepositoryError::Unavailable),
        };
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(outcome)
    }

    fn delete_server_if_empty(
        &self,
        server_id: &ManagedServerId,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Result<DeleteManagedServerResult, ServerWorkspaceRepositoryError> {
        let expected = workspace_counter(expected_record_version.get())?;
        let mut connection = self
            .lock_connection()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let deleted = transaction
            .execute(
                r#"
                DELETE FROM managed_servers
                WHERE id = ?1
                    AND record_version = ?2
                    AND NOT EXISTS(
                        SELECT 1 FROM connection_profiles WHERE server_id = ?1
                    )
                "#,
                params![server_id.as_str(), expected],
            )
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        let outcome = match deleted {
            1 => DeleteManagedServerResult::Deleted,
            0 => match server_version_and_profile_count(&transaction, server_id)? {
                None => DeleteManagedServerResult::NotFound,
                Some((version, _)) if version != expected => DeleteManagedServerResult::Conflict,
                Some((_, count)) if count > 0 => DeleteManagedServerResult::NotEmpty,
                Some(_) => return Err(ServerWorkspaceRepositoryError::Unavailable),
            },
            _ => return Err(ServerWorkspaceRepositoryError::Unavailable),
        };
        transaction
            .commit()
            .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
        Ok(outcome)
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
        match transaction.execute(
            r#"
            INSERT INTO managed_servers(id, project_id, display_name, record_version)
            VALUES (?1, NULL, ?2, 1)
            "#,
            params![
                profile.server_id().as_str(),
                profile.display_name().as_str()
            ],
        ) {
            Ok(1) => {}
            Ok(_) => return Err(ConnectionMutationRepositoryError::Unavailable),
            Err(error) if is_primary_key_conflict(&error) => {
                transaction
                    .rollback()
                    .map_err(|_| ConnectionMutationRepositoryError::Unavailable)?;
                return Ok(CreateConnectionProfileResult::Conflict);
            }
            Err(_) => return Err(ConnectionMutationRepositoryError::Unavailable),
        }
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
        read_host_trust(&connection, profile_id).map_err(|_| HostTrustRepositoryError::Unavailable)
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

fn validate_profiles_and_trusts(
    transaction: &Transaction<'_>,
) -> Result<(), SqliteStorageUnavailable> {
    let mut profiles = transaction
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
    let mut rows = profiles.query(()).map_err(|_| SqliteStorageUnavailable)?;
    while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
        decode_connection_profile(row)?;
    }
    drop(rows);
    drop(profiles);

    let orphaned_trust: bool = transaction
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM host_trusts AS trust
                LEFT JOIN connection_profiles AS profile ON profile.id = trust.profile_id
                WHERE profile.id IS NULL
            )
            "#,
            (),
            |row| row.get(0),
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    if orphaned_trust {
        return Err(SqliteStorageUnavailable);
    }
    let future_trust: bool = transaction
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM host_trusts AS trust
                JOIN connection_profiles AS profile ON profile.id = trust.profile_id
                WHERE trust.security_revision > profile.security_revision
            )
            "#,
            (),
            |row| row.get(0),
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    if future_trust {
        return Err(SqliteStorageUnavailable);
    }

    let mut trusts = transaction
        .prepare(
            r#"
            SELECT profile_id, security_revision, fingerprint
            FROM host_trusts
            ORDER BY profile_id ASC
            "#,
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = trusts.query(()).map_err(|_| SqliteStorageUnavailable)?;
    while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
        decode_host_trust(row)?;
    }
    Ok(())
}

fn insert_server(
    transaction: &Transaction<'_>,
    server: &ManagedServer,
    record_version: i64,
) -> rusqlite::Result<usize> {
    transaction.execute(
        r#"
        INSERT INTO managed_servers(id, project_id, display_name, record_version)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        params![
            server.id().as_str(),
            server.project_id().map(ProjectId::as_str),
            server.display_name().as_str(),
            record_version,
        ],
    )
}

fn insert_profile(
    transaction: &Transaction<'_>,
    profile: &ConnectionProfile,
    record_version: i64,
    security_revision: i64,
) -> rusqlite::Result<usize> {
    transaction.execute(
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
    )
}

fn project_exists(
    connection: &Connection,
    project_id: &ProjectId,
) -> Result<bool, ServerWorkspaceRepositoryError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            params![project_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)
}

fn server_exists(
    connection: &Connection,
    server_id: &ManagedServerId,
) -> Result<bool, ServerWorkspaceRepositoryError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_servers WHERE id = ?1)",
            params![server_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)
}

fn server_version_and_profile_count(
    connection: &Connection,
    server_id: &ManagedServerId,
) -> Result<Option<(i64, i64)>, ServerWorkspaceRepositoryError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                server.record_version,
                COUNT(profile.id)
            FROM managed_servers AS server
            LEFT JOIN connection_profiles AS profile ON profile.server_id = server.id
            WHERE server.id = ?1
            GROUP BY server.id, server.record_version
            "#,
        )
        .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
    let mut rows = statement
        .query(params![server_id.as_str()])
        .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?;
    match rows
        .next()
        .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?
    {
        Some(row) => Ok(Some((
            row.get(0)
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?,
            row.get(1)
                .map_err(|_| ServerWorkspaceRepositoryError::Unavailable)?,
        ))),
        None => Ok(None),
    }
}

fn read_all_projects(connection: &Connection) -> Result<Vec<Project>, SqliteStorageUnavailable> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, display_name, record_version
            FROM projects
            ORDER BY display_name COLLATE NOCASE ASC, id ASC
            "#,
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = statement.query(()).map_err(|_| SqliteStorageUnavailable)?;
    let mut projects = Vec::new();
    while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
        projects.push(decode_project(row)?);
    }
    Ok(projects)
}

fn read_all_servers(
    connection: &Connection,
) -> Result<Vec<ManagedServer>, SqliteStorageUnavailable> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, display_name, record_version
            FROM managed_servers
            ORDER BY display_name COLLATE NOCASE ASC, id ASC
            "#,
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = statement.query(()).map_err(|_| SqliteStorageUnavailable)?;
    let mut servers = Vec::new();
    while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
        servers.push(decode_managed_server(row)?);
    }
    Ok(servers)
}

fn read_all_connection_profiles(
    connection: &Connection,
) -> Result<Vec<ConnectionProfile>, SqliteStorageUnavailable> {
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

fn read_connection_profile(
    connection: &Connection,
    profile_id: &ConnectionProfileId,
) -> Result<Option<ConnectionProfile>, SqliteStorageUnavailable> {
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
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = statement
        .query(params![profile_id.as_str()])
        .map_err(|_| SqliteStorageUnavailable)?;
    match rows.next().map_err(|_| SqliteStorageUnavailable)? {
        Some(row) => decode_connection_profile(row).map(Some),
        None => Ok(None),
    }
}

fn read_all_host_trusts(
    connection: &Connection,
) -> Result<BTreeMap<ConnectionProfileId, StoredHostTrust>, SqliteStorageUnavailable> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT profile_id, security_revision, fingerprint
            FROM host_trusts
            ORDER BY profile_id ASC
            "#,
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = statement.query(()).map_err(|_| SqliteStorageUnavailable)?;
    let mut trusts = BTreeMap::new();
    while let Some(row) = rows.next().map_err(|_| SqliteStorageUnavailable)? {
        let trust = decode_host_trust(row)?;
        if trusts.insert(trust.profile_id().clone(), trust).is_some() {
            return Err(SqliteStorageUnavailable);
        }
    }
    Ok(trusts)
}

fn read_host_trust(
    connection: &Connection,
    profile_id: &ConnectionProfileId,
) -> Result<Option<StoredHostTrust>, SqliteStorageUnavailable> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT profile_id, security_revision, fingerprint
            FROM host_trusts
            WHERE profile_id = ?1
            "#,
        )
        .map_err(|_| SqliteStorageUnavailable)?;
    let mut rows = statement
        .query(params![profile_id.as_str()])
        .map_err(|_| SqliteStorageUnavailable)?;
    match rows.next().map_err(|_| SqliteStorageUnavailable)? {
        Some(row) => decode_host_trust(row).map(Some),
        None => Ok(None),
    }
}

fn workspace_counter(value: u64) -> Result<i64, ServerWorkspaceRepositoryError> {
    i64::try_from(value).map_err(|_| ServerWorkspaceRepositoryError::Unavailable)
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
    is_primary_key_conflict(error)
}

fn is_primary_key_conflict(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_extended_error_code(),
        Some(rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY | rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
    )
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

fn decode_project(row: &Row<'_>) -> Result<Project, SqliteStorageUnavailable> {
    let id = ProjectId::parse(
        row.get::<_, String>(0)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let display_name = ProjectDisplayName::parse(
        row.get::<_, String>(1)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let version = row.get::<_, i64>(2).map_err(|_| SqliteStorageUnavailable)?;
    let record_version =
        ProjectRecordVersion::new(u64::try_from(version).map_err(|_| SqliteStorageUnavailable)?)
            .map_err(|_| SqliteStorageUnavailable)?;
    Ok(Project::new(id, display_name, record_version))
}

fn decode_managed_server(row: &Row<'_>) -> Result<ManagedServer, SqliteStorageUnavailable> {
    let id = ManagedServerId::parse(
        row.get::<_, String>(0)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let project_id = row
        .get::<_, Option<String>>(1)
        .map_err(|_| SqliteStorageUnavailable)?
        .map(ProjectId::parse)
        .transpose()
        .map_err(|_| SqliteStorageUnavailable)?;
    let display_name = ManagedServerDisplayName::parse(
        row.get::<_, String>(2)
            .map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    let version = row.get::<_, i64>(3).map_err(|_| SqliteStorageUnavailable)?;
    let record_version = ManagedServerRecordVersion::new(
        u64::try_from(version).map_err(|_| SqliteStorageUnavailable)?,
    )
    .map_err(|_| SqliteStorageUnavailable)?;
    Ok(ManagedServer::new(
        id,
        project_id,
        display_name,
        record_version,
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
    fn human_workbench_snapshot_holds_one_mutex_and_one_sqlite_read_snapshot() {
        let path = temporary_database_path("human-workbench-snapshot");
        let storage = SqliteStorage::new(&path).expect("open storage");
        assert_eq!(
            storage
                .lock_connection()
                .expect("lock storage")
                .query_row("PRAGMA journal_mode = WAL", (), |row| row
                    .get::<_, String>(0))
                .expect("enable WAL"),
            "wal"
        );
        let project = sample_project("project-snapshot", "Original project", 1);
        assert_eq!(
            ServerWorkspaceMutationRepository::create_project(&storage, &project)
                .expect("create project"),
            CreateProjectResult::Created
        );
        let server = sample_server(
            "server-snapshot",
            Some(project.id().clone()),
            "Original server",
            1,
        );
        let profile = profile_for_server("profile-snapshot", server.id(), "Original route");
        assert_eq!(
            ServerWorkspaceMutationRepository::create_server_with_connection(
                &storage, &server, &profile,
            )
            .expect("create server and profile"),
            CreateServerWithConnectionResult::Created
        );
        storage
            .upsert_host_trust_fixture(&sample_trust(profile.id(), 1))
            .expect("seed trust");

        let writer = Connection::open(&path).expect("open concurrent writer");
        writer
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable writer foreign keys");
        let lock_probe = storage.clone();
        let snapshot = storage
            .load_human_workbench_snapshot_with_hook(|| {
                assert!(matches!(
                    lock_probe.connection.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                writer
                    .execute_batch(
                        r#"
                        BEGIN IMMEDIATE;
                        UPDATE projects
                        SET display_name = 'Changed project', record_version = 2
                        WHERE id = 'project-snapshot';
                        UPDATE managed_servers
                        SET display_name = 'Changed server', record_version = 2
                        WHERE id = 'server-snapshot';
                        UPDATE connection_profiles
                        SET display_name = 'Changed route', record_version = 2,
                            security_revision = 2
                        WHERE id = 'profile-snapshot';
                        UPDATE host_trusts
                        SET security_revision = 2
                        WHERE profile_id = 'profile-snapshot';
                        COMMIT;
                        "#,
                    )
                    .expect("commit concurrent update");
            })
            .expect("load workbench snapshot");
        let (projects, servers, connections) = snapshot.into_parts();
        let (observed_profile, observed_trust) = connections
            .into_iter()
            .next()
            .expect("snapshot profile")
            .into_parts();
        assert_eq!(projects, vec![project]);
        assert_eq!(servers, vec![server]);
        assert_eq!(observed_profile, profile);
        assert!(
            observed_trust
                .as_ref()
                .is_some_and(|trust| trust.matches(&observed_profile))
        );

        let (updated_projects, updated_servers, updated_connections) = storage
            .load_human_workbench_snapshot()
            .expect("load committed state")
            .into_parts();
        let (updated_profile, updated_trust) = updated_connections
            .into_iter()
            .next()
            .expect("updated profile")
            .into_parts();
        assert_eq!(
            updated_projects[0].display_name().as_str(),
            "Changed project"
        );
        assert_eq!(updated_servers[0].display_name().as_str(), "Changed server");
        assert_eq!(updated_profile.display_name().as_str(), "Changed route");
        assert!(
            updated_trust
                .as_ref()
                .is_some_and(|trust| trust.matches(&updated_profile))
        );

        drop(writer);
        drop(storage);
        fs::remove_file(path).expect("remove snapshot database");
    }

    #[test]
    fn connection_editor_snapshot_holds_one_mutex_and_one_sqlite_read_snapshot() {
        let path = temporary_database_path("connection-editor-snapshot");
        let storage = SqliteStorage::new(&path).expect("open storage");
        assert_eq!(
            storage
                .lock_connection()
                .expect("lock storage")
                .query_row("PRAGMA journal_mode = WAL", (), |row| row
                    .get::<_, String>(0))
                .expect("enable WAL"),
            "wal"
        );
        let profile = sample_profile("editor-snapshot", 1, true);
        storage
            .upsert_connection_profile_fixture(&profile)
            .expect("seed profile");
        storage
            .upsert_host_trust_fixture(&sample_trust(profile.id(), 1))
            .expect("seed trust");

        let writer = Connection::open(&path).expect("open concurrent writer");
        writer
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable writer foreign keys");
        let lock_probe = storage.clone();
        let snapshot = storage
            .load_connection_editor_snapshot_with_hook(profile.id(), || {
                assert!(matches!(
                    lock_probe.connection.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                writer
                    .execute_batch(
                        r#"
                        BEGIN IMMEDIATE;
                        UPDATE connection_profiles
                        SET display_name = 'Changed editor', record_version = 2,
                            security_revision = 2
                        WHERE id = 'editor-snapshot';
                        UPDATE host_trusts
                        SET security_revision = 2
                        WHERE profile_id = 'editor-snapshot';
                        COMMIT;
                        "#,
                    )
                    .expect("commit concurrent editor update");
            })
            .expect("load editor snapshot")
            .expect("profile exists");
        let (observed_profile, observed_trust) = snapshot.into_parts();
        assert_eq!(observed_profile, profile);
        assert!(
            observed_trust
                .as_ref()
                .is_some_and(|trust| trust.matches(&observed_profile))
        );

        let (updated_profile, updated_trust) = storage
            .load_connection_editor_snapshot(profile.id())
            .expect("load committed editor state")
            .expect("profile exists")
            .into_parts();
        assert_eq!(updated_profile.display_name().as_str(), "Changed editor");
        assert!(
            updated_trust
                .as_ref()
                .is_some_and(|trust| trust.matches(&updated_profile))
        );

        drop(writer);
        drop(storage);
        fs::remove_file(path).expect("remove snapshot database");
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
    fn creates_fresh_generation_schema_and_is_idempotent() {
        let path = temporary_database_path("migrations");
        if path.exists() {
            std::fs::remove_file(&path).expect("remove old temporary database");
        }

        let storage = SqliteStorage::new(&path).expect("initialize schema");
        assert_eq!(
            storage.schema_version().expect("read user version"),
            FRESH_SCHEMA_VERSION
        );
        let connection = storage.lock_connection().expect("lock fresh database");
        let columns = connection
            .prepare("PRAGMA table_info(connection_profiles)")
            .and_then(|mut statement| {
                let rows = statement.query_map((), |row| row.get::<_, String>(1))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .expect("read fresh schema columns");
        assert!(columns.iter().any(|column| column == "display_name"));
        assert!(columns.iter().any(|column| column == "record_version"));
        let server_columns = connection
            .prepare("PRAGMA table_info(managed_servers)")
            .and_then(|mut statement| {
                let rows = statement.query_map((), |row| row.get::<_, String>(1))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .expect("read managed server columns");
        assert_eq!(
            server_columns,
            vec!["id", "project_id", "display_name", "record_version"]
        );
        assert_eq!(
            connection
                .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
                .expect("application id"),
            STORAGE_APPLICATION_ID
        );
        assert_eq!(
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .expect("journal mode"),
            "wal"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
                .expect("synchronous"),
            2
        );
        assert_eq!(
            connection
                .query_row("PRAGMA auto_vacuum", [], |row| row.get::<_, i64>(0))
                .expect("auto vacuum"),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT storage_generation FROM schema_metadata WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("storage generation"),
            STORAGE_GENERATION
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT clean_shutdown FROM storage_runtime_state WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("dirty runtime marker"),
            0
        );
        drop(connection);

        storage.flush_for_shutdown().expect("clean shutdown");
        assert_eq!(
            storage
                .lock_connection()
                .expect("lock clean database")
                .query_row(
                    "SELECT clean_shutdown FROM storage_runtime_state WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("clean runtime marker"),
            1
        );

        drop(storage);
        let reopened = SqliteStorage::new(&path).expect("reopen schema");
        assert_eq!(
            reopened.schema_version().expect("read user version"),
            FRESH_SCHEMA_VERSION
        );

        drop(reopened);
        std::fs::remove_file(&path).expect("remove test database");
    }

    #[test]
    fn rejects_unknown_legacy_and_ahead_generation_without_rewriting_them() {
        let unknown_path = temporary_database_path("unknown-legacy");
        let unknown = Connection::open(&unknown_path).expect("open unknown database");
        unknown
            .execute_batch(
                "CREATE TABLE unexpected(id INTEGER PRIMARY KEY); PRAGMA user_version = 1;",
            )
            .expect("create unknown database");
        drop(unknown);
        assert!(matches!(
            SqliteStorage::new(&unknown_path),
            Err(StorageOpenError::UnsupportedLegacySchema)
        ));
        let reopened = Connection::open(&unknown_path).expect("reopen unknown database");
        assert_eq!(
            reopened
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("unknown version"),
            1
        );
        drop(reopened);
        fs::remove_file(unknown_path).expect("remove unknown database");

        let ahead_path = temporary_database_path("ahead-generation");
        let ahead = SqliteStorage::new(&ahead_path).expect("create current generation");
        ahead
            .lock_connection()
            .expect("lock ahead database")
            .pragma_update(None, "user_version", FRESH_SCHEMA_VERSION + 1)
            .expect("set ahead version");
        drop(ahead);
        assert!(matches!(
            SqliteStorage::new(&ahead_path),
            Err(StorageOpenError::SchemaAhead)
        ));
        fs::remove_file(ahead_path).expect("remove ahead database");
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
                    'legacy-fallback-server',
                    1,
                    'legacy-empty.example.invalid',
                    22,
                    'operator',
                    'legacy-empty-credential',
                    1
                );

                INSERT INTO host_trusts(profile_id, security_revision, fingerprint)
                VALUES (
                    'legacy-profile',
                    1,
                    'SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
                );

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

        assert_eq!(
            storage.schema_version().expect("read user version"),
            FRESH_SCHEMA_VERSION
        );
        assert_eq!(display_name, "legacy-server");
        assert_eq!(fallback_display_name, "legacy-fallback-server");
        assert_eq!(long_id_display_name, "Connection");
        assert_eq!(record_version, 1);
        assert_eq!(trust_count, 1);
    }

    #[test]
    fn migrates_v2_shared_server_deterministically_and_preserves_trust() {
        let connection = Connection::open_in_memory().expect("open v2 database");
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
                    enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
                    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
                    record_version INTEGER NOT NULL CHECK (record_version > 0)
                );
                CREATE TABLE host_trusts (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                    fingerprint TEXT NOT NULL,
                    FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                );
                INSERT INTO connection_profiles VALUES (
                    'profile-z', 'shared-server', 2, 'z.example.invalid', 22, 'z-user',
                    'credential-z', 1, 'Zulu route', 4
                );
                INSERT INTO connection_profiles VALUES (
                    'profile-a', 'shared-server', 1, 'a.example.invalid', 2200, 'a-user',
                    'credential-a', 0, 'Alpha route', 3
                );
                INSERT INTO host_trusts VALUES (
                    'profile-z', 2, 'SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
                );
                PRAGMA user_version = 2;
                "#,
            )
            .expect("create v2 database");

        let storage = SqliteStorage::from_connection(connection).expect("migrate v2 database");
        let server = ServerWorkspaceReadRepository::find_server(
            &storage,
            &ManagedServerId::parse("shared-server").expect("server id"),
        )
        .expect("read migrated server")
        .expect("server exists");
        assert_eq!(server.display_name().as_str(), "Alpha route");
        assert_eq!(server.record_version(), ManagedServerRecordVersion::INITIAL);
        assert_eq!(server.project_id(), None);
        let profiles =
            ServerWorkspaceReadRepository::list_connection_profiles(&storage, server.id())
                .expect("list migrated profiles");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id().as_str(), "profile-a");
        assert_eq!(profiles[1].id().as_str(), "profile-z");
        assert!(
            HostTrustRepository::find(
                &storage,
                &ConnectionProfileId::parse("profile-z").expect("profile id")
            )
            .expect("read migrated trust")
            .is_some()
        );
    }

    #[test]
    fn adopts_valid_v3_schema_into_fresh_generation_without_changing_records() {
        let mut connection = Connection::open_in_memory().expect("open v3 database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .expect("enable foreign keys");
        SqliteStorage::run_migrations(&mut connection).expect("create legacy v3 schema");
        connection
            .execute(
                "INSERT INTO projects(id, display_name, record_version) VALUES (?1, ?2, 1)",
                params!["project-v3", "Project v3"],
            )
            .expect("seed project");

        let storage = SqliteStorage::from_connection(connection).expect("adopt v3 generation");
        let connection = storage.lock_connection().expect("lock adopted database");
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("user version"),
            i64::from(FRESH_SCHEMA_VERSION)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT migrated_from_user_version FROM schema_metadata WHERE singleton_id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("migration source"),
            i64::from(LEGACY_SCHEMA_VERSION)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT display_name FROM projects WHERE id = 'project-v3'",
                    [],
                    |row| { row.get::<_, String>(0) }
                )
                .expect("project"),
            "Project v3"
        );
    }

    #[test]
    fn invalid_v2_server_identity_rolls_back_without_partial_v3_state() {
        let path = temporary_database_path("invalid-v2-migration");
        let connection = Connection::open(&path).expect("open v2 database");
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
                    enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
                    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
                    record_version INTEGER NOT NULL CHECK (record_version > 0)
                );
                CREATE TABLE host_trusts (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                    fingerprint TEXT NOT NULL,
                    FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                );
                INSERT INTO connection_profiles VALUES (
                    'invalid-profile', '   ', 1, 'host.example.invalid', 22, 'operator',
                    'credential', 1, 'Invalid route', 1
                );
                PRAGMA user_version = 2;
                "#,
            )
            .expect("create invalid v2 database");
        drop(connection);

        assert!(matches!(
            SqliteStorage::new(&path),
            Err(StorageOpenError::UnsupportedLegacySchema)
        ));
        let reopened = Connection::open(&path).expect("reopen rolled-back database");
        assert_eq!(
            reopened
                .query_row("PRAGMA user_version", (), |row| row.get::<_, i64>(0))
                .expect("read version"),
            2
        );
        assert_eq!(
            reopened
                .query_row(
                    r#"
                    SELECT COUNT(*)
                    FROM sqlite_master
                    WHERE type = 'table' AND name IN ('projects', 'managed_servers')
                    "#,
                    (),
                    |row| row.get::<_, i64>(0),
                )
                .expect("count v3 tables"),
            0
        );
        drop(reopened);
        fs::remove_file(path).expect("remove invalid database");
    }

    #[test]
    fn future_host_trust_revision_rolls_back_v2_migration() {
        let path = temporary_database_path("future-trust-migration");
        let connection = Connection::open(&path).expect("open v2 database");
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
                    enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
                    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
                    record_version INTEGER NOT NULL CHECK (record_version > 0)
                );
                CREATE TABLE host_trusts (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    security_revision INTEGER NOT NULL CHECK (security_revision > 0),
                    fingerprint TEXT NOT NULL,
                    FOREIGN KEY(profile_id) REFERENCES connection_profiles(id) ON DELETE CASCADE
                );
                INSERT INTO connection_profiles VALUES (
                    'profile', 'server', 2, 'host.example.invalid', 22, 'operator',
                    'credential', 1, 'Route', 1
                );
                INSERT INTO host_trusts VALUES (
                    'profile', 3, 'SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
                );
                PRAGMA user_version = 2;
                "#,
            )
            .expect("create future trust database");
        drop(connection);

        assert!(matches!(
            SqliteStorage::new(&path),
            Err(StorageOpenError::UnsupportedLegacySchema)
        ));
        let reopened = Connection::open(&path).expect("reopen database");
        assert_eq!(
            reopened
                .query_row("PRAGMA user_version", (), |row| row.get::<_, i64>(0))
                .expect("read version"),
            2
        );
        assert_eq!(
            reopened
                .query_row("SELECT COUNT(*) FROM host_trusts", (), |row| {
                    row.get::<_, i64>(0)
                })
                .expect("read trust count"),
            1
        );
        drop(reopened);
        fs::remove_file(path).expect("remove database");
    }

    #[test]
    fn v3_primary_keys_explicitly_reject_null_identities() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let connection = storage.lock_connection().expect("lock storage");
        assert!(
            connection
                .execute(
                    "INSERT INTO projects(id, display_name, record_version) VALUES (NULL, 'P', 1)",
                    (),
                )
                .is_err()
        );
        assert!(
            connection
                .execute(
                    r#"
                    INSERT INTO managed_servers(id, project_id, display_name, record_version)
                    VALUES (NULL, NULL, 'S', 1)
                    "#,
                    (),
                )
                .is_err()
        );
        connection
            .execute(
                r#"
                INSERT INTO managed_servers(id, project_id, display_name, record_version)
                VALUES ('server', NULL, 'Server', 1)
                "#,
                (),
            )
            .expect("seed server");
        assert!(
            connection
                .execute(
                    r#"
                    INSERT INTO connection_profiles(
                        id, server_id, display_name, record_version, security_revision,
                        route_host, route_port, route_username, credential_ref, enabled
                    )
                    VALUES (NULL, 'server', 'Route', 1, 1, 'host', 22, 'user', 'credential', 1)
                    "#,
                    (),
                )
                .is_err()
        );
    }

    #[test]
    fn project_delete_unassigns_servers_and_advances_each_server_version() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let project = sample_project("project-main", "Main project", 1);
        assert_eq!(
            ServerWorkspaceMutationRepository::create_project(&storage, &project)
                .expect("create project"),
            CreateProjectResult::Created
        );
        for suffix in ["a", "b"] {
            let server = sample_server(
                &format!("server-{suffix}"),
                Some(project.id().clone()),
                &format!("Server {suffix}"),
                1,
            );
            let profile = profile_for_server(
                &format!("profile-{suffix}"),
                server.id(),
                &format!("Connection {suffix}"),
            );
            assert_eq!(
                ServerWorkspaceMutationRepository::create_server_with_connection(
                    &storage, &server, &profile
                )
                .expect("create server and profile"),
                CreateServerWithConnectionResult::Created
            );
        }

        assert_eq!(
            ServerWorkspaceMutationRepository::delete_project_and_unassign_servers(
                &storage,
                project.id(),
                ProjectRecordVersion::INITIAL,
            )
            .expect("delete project"),
            DeleteProjectResult::Deleted {
                unassigned_servers: 2
            }
        );
        assert!(
            ServerWorkspaceReadRepository::find_project(&storage, project.id())
                .expect("read deleted project")
                .is_none()
        );
        for suffix in ["a", "b"] {
            let server = ServerWorkspaceReadRepository::find_server(
                &storage,
                &ManagedServerId::parse(format!("server-{suffix}")).expect("server id"),
            )
            .expect("read server")
            .expect("server remains");
            assert_eq!(server.project_id(), None);
            assert_eq!(
                server.record_version(),
                ManagedServerRecordVersion::new(2).expect("version")
            );
        }
    }

    #[test]
    fn project_delete_version_exhaustion_is_zero_mutation() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let project = sample_project("project-main", "Main project", 1);
        ServerWorkspaceMutationRepository::create_project(&storage, &project)
            .expect("create project");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO managed_servers(id, project_id, display_name, record_version)
                VALUES ('server-max', 'project-main', 'Max server', ?1)
                "#,
                params![i64::MAX],
            )
            .expect("seed exhausted server");

        assert_eq!(
            ServerWorkspaceMutationRepository::delete_project_and_unassign_servers(
                &storage,
                project.id(),
                ProjectRecordVersion::INITIAL,
            )
            .expect("settle exhausted delete"),
            DeleteProjectResult::VersionExhausted
        );
        let server = ServerWorkspaceReadRepository::find_server(
            &storage,
            &ManagedServerId::parse("server-max").expect("server id"),
        )
        .expect("read server")
        .expect("server remains");
        assert_eq!(server.project_id(), Some(project.id()));
        assert_eq!(server.record_version().get(), i64::MAX as u64);
        assert!(
            ServerWorkspaceReadRepository::find_project(&storage, project.id())
                .expect("read project")
                .is_some()
        );
    }

    #[test]
    fn project_delete_rejects_corrupted_server_version_without_mutation() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let project = sample_project("project-main", "Main project", 1);
        ServerWorkspaceMutationRepository::create_project(&storage, &project)
            .expect("create project");
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO managed_servers(id, project_id, display_name, record_version)
                VALUES ('server-corrupt', 'project-main', 'Corrupt server', 'abc')
                "#,
                (),
            )
            .expect("seed corrupt server version");

        assert_eq!(
            ServerWorkspaceMutationRepository::delete_project_and_unassign_servers(
                &storage,
                project.id(),
                ProjectRecordVersion::INITIAL,
            ),
            Err(ServerWorkspaceRepositoryError::Unavailable)
        );
        let connection = storage.lock_connection().expect("lock storage");
        assert_eq!(
            connection
                .query_row(
                    "SELECT project_id, record_version FROM managed_servers WHERE id = 'server-corrupt'",
                    (),
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .expect("read unchanged server"),
            ("project-main".to_owned(), "abc".to_owned())
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM projects WHERE id = 'project-main'",
                    (),
                    |row| { row.get::<_, i64>(0) }
                )
                .expect("read project count"),
            1
        );
    }

    #[test]
    fn server_lifecycle_is_atomic_and_empty_delete_is_explicit() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let server = sample_server("server-main", None, "Main server", 1);
        let first = profile_for_server("profile-first", server.id(), "First route");
        assert_eq!(
            ServerWorkspaceMutationRepository::create_server_with_connection(
                &storage, &server, &first
            )
            .expect("create server and first profile"),
            CreateServerWithConnectionResult::Created
        );
        let second = profile_for_server("profile-second", server.id(), "Second route");
        assert_eq!(
            ServerWorkspaceMutationRepository::add_connection_to_server(&storage, &second)
                .expect("add second profile"),
            AddConnectionToServerResult::Created
        );
        assert_eq!(
            ServerWorkspaceMutationRepository::delete_server_if_empty(
                &storage,
                server.id(),
                ManagedServerRecordVersion::INITIAL,
            )
            .expect("reject non-empty delete"),
            DeleteManagedServerResult::NotEmpty
        );
        for profile in [&first, &second] {
            assert_eq!(
                ConnectionProfileMutationRepository::delete_if_version(
                    &storage,
                    profile.id(),
                    ConnectionRecordVersion::INITIAL,
                )
                .expect("delete profile"),
                DeleteConnectionProfileResult::Deleted
            );
        }
        assert_eq!(
            ServerWorkspaceMutationRepository::delete_server_if_empty(
                &storage,
                server.id(),
                ManagedServerRecordVersion::INITIAL,
            )
            .expect("delete empty server"),
            DeleteManagedServerResult::Deleted
        );
        assert!(
            ServerWorkspaceReadRepository::find_server(&storage, server.id())
                .expect("read deleted server")
                .is_none()
        );
    }

    #[test]
    fn atomic_server_create_rolls_back_when_profile_identity_conflicts() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let existing_server = sample_server("server-existing", None, "Existing", 1);
        let existing_profile =
            profile_for_server("profile-shared", existing_server.id(), "Existing route");
        assert_eq!(
            ServerWorkspaceMutationRepository::create_server_with_connection(
                &storage,
                &existing_server,
                &existing_profile,
            )
            .expect("create existing fixture"),
            CreateServerWithConnectionResult::Created
        );

        let new_server = sample_server("server-new", None, "New", 1);
        let conflicting_profile =
            profile_for_server("profile-shared", new_server.id(), "Conflicting route");
        assert_eq!(
            ServerWorkspaceMutationRepository::create_server_with_connection(
                &storage,
                &new_server,
                &conflicting_profile,
            )
            .expect("settle profile conflict"),
            CreateServerWithConnectionResult::ProfileConflict
        );
        assert!(
            ServerWorkspaceReadRepository::find_server(&storage, new_server.id())
                .expect("read rolled-back server")
                .is_none()
        );
    }

    #[test]
    fn conditional_project_and_server_replace_preserve_exact_versions() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let project_one = sample_project("project-one", "One", 1);
        let project_two = sample_project("project-two", "Two", 1);
        for project in [&project_one, &project_two] {
            assert_eq!(
                ServerWorkspaceMutationRepository::create_project(&storage, project)
                    .expect("create project"),
                CreateProjectResult::Created
            );
        }
        let renamed_project = sample_project("project-one", "Renamed", 2);
        assert_eq!(
            ServerWorkspaceMutationRepository::replace_project_if_version(
                &storage,
                &renamed_project,
                ProjectRecordVersion::INITIAL,
            )
            .expect("rename project"),
            ReplaceProjectResult::Replaced
        );
        assert_eq!(
            ServerWorkspaceMutationRepository::replace_project_if_version(
                &storage,
                &renamed_project,
                ProjectRecordVersion::INITIAL,
            )
            .expect("settle stale project update"),
            ReplaceProjectResult::Conflict
        );

        let server = sample_server("server-main", Some(project_one.id().clone()), "Server", 1);
        let profile = profile_for_server("profile-main", server.id(), "Route");
        assert_eq!(
            ServerWorkspaceMutationRepository::create_server_with_connection(
                &storage, &server, &profile
            )
            .expect("create server"),
            CreateServerWithConnectionResult::Created
        );
        let moved = sample_server(
            "server-main",
            Some(project_two.id().clone()),
            "Moved server",
            2,
        );
        assert_eq!(
            ServerWorkspaceMutationRepository::replace_server_if_version(
                &storage,
                &moved,
                ManagedServerRecordVersion::INITIAL,
            )
            .expect("move server"),
            ReplaceManagedServerResult::Replaced
        );
        assert_eq!(
            ServerWorkspaceReadRepository::find_server(&storage, server.id())
                .expect("read moved server"),
            Some(moved.clone())
        );
        let missing_project = sample_server(
            "server-main",
            Some(ProjectId::parse("project-missing").expect("project id")),
            "Missing project",
            3,
        );
        assert_eq!(
            ServerWorkspaceMutationRepository::replace_server_if_version(
                &storage,
                &missing_project,
                ManagedServerRecordVersion::new(2).expect("server version"),
            )
            .expect("settle missing project"),
            ReplaceManagedServerResult::ProjectNotFound
        );
        assert_eq!(
            ServerWorkspaceReadRepository::find_server(&storage, server.id())
                .expect("read unchanged server"),
            Some(moved)
        );
    }

    #[test]
    fn add_connection_to_missing_server_is_zero_mutation() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let profile = profile_for_server(
            "profile-missing-server",
            &ManagedServerId::parse("server-missing").expect("server id"),
            "Missing server route",
        );
        assert_eq!(
            ServerWorkspaceMutationRepository::add_connection_to_server(&storage, &profile)
                .expect("settle missing server"),
            AddConnectionToServerResult::ServerNotFound
        );
        assert!(
            ConnectionProfileRepository::find(&storage, profile.id())
                .expect("read missing profile")
                .is_none()
        );
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
    fn legacy_profile_create_cannot_attach_to_an_existing_server() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_server_row(&storage, "server", "Existing server");
        let profile = sample_profile("legacy-create", 1, true);

        assert_eq!(
            ConnectionProfileMutationRepository::create(&storage, &profile)
                .expect("settle server conflict"),
            CreateConnectionProfileResult::Conflict
        );
        assert!(
            ConnectionProfileRepository::find(&storage, profile.id())
                .expect("read profile")
                .is_none()
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
        seed_server_row(&storage, "server", "Server");
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
        seed_server_row(&storage, "server", "Server");
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

    fn sample_project(id: &str, display_name: &str, version: u64) -> Project {
        Project::new(
            ProjectId::parse(id).expect("valid project id"),
            ProjectDisplayName::parse(display_name).expect("valid project name"),
            ProjectRecordVersion::new(version).expect("valid project version"),
        )
    }

    fn sample_server(
        id: &str,
        project_id: Option<ProjectId>,
        display_name: &str,
        version: u64,
    ) -> ManagedServer {
        ManagedServer::new(
            ManagedServerId::parse(id).expect("valid server id"),
            project_id,
            ManagedServerDisplayName::parse(display_name).expect("valid server name"),
            ManagedServerRecordVersion::new(version).expect("valid server version"),
        )
    }

    fn profile_for_server(
        id: &str,
        server_id: &ManagedServerId,
        display_name: &str,
    ) -> ConnectionProfile {
        ConnectionProfile::new(
            ConnectionProfileId::parse(id).expect("valid profile id"),
            server_id.clone(),
            ConnectionDisplayName::parse(display_name).expect("valid display name"),
            ConnectionRecordVersion::INITIAL,
            ConnectionSecurityRevision::INITIAL,
            SshRoute::new("host.example.invalid", 22, "operator").expect("valid route"),
            CredentialRef::parse(format!("credential-{id}")).expect("valid credential ref"),
            true,
        )
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

    fn seed_server_row(storage: &SqliteStorage, id: &str, display_name: &str) {
        storage
            .lock_connection()
            .expect("lock storage")
            .execute(
                r#"
                INSERT INTO managed_servers(id, project_id, display_name, record_version)
                VALUES (?1, NULL, ?2, 1)
                "#,
                params![id, display_name],
            )
            .expect("seed managed server");
    }
}
