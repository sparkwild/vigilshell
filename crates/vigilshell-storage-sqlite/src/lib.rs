//! 新架构设置数据库；不打开或迁移旧版 Tauri 数据库。
mod connection_configuration;
mod group_layout;
pub mod recordings;
mod ui_state;
mod vault;
mod workspace;
use rusqlite::{Connection, params};
use std::{fs, path::Path};
use vigilshell_application_contracts::ports::PreferencesRepository;
use vigilshell_domain::notifications::NotificationMode;
use vigilshell_domain::{
    LocaleMode, MAX_REVISION, Preferences, PreferencesUpdate, SettingsError, ThemeMode,
    UpdatePreferencesRequest,
};
pub const DATABASE_FILENAME: &str = "app.sqlite3";
const APPLICATION_ID: u32 = 0x5653_4831;
const SCHEMA_VERSION: u32 = 6;
#[cfg(test)]
const PREFERENCES_SCHEMA: &str = "CREATE TABLE preferences (id INTEGER PRIMARY KEY CHECK(id=1), theme_mode TEXT NOT NULL CHECK(theme_mode IN ('system','light','dark')), locale_mode TEXT NOT NULL CHECK(locale_mode IN ('system','en','zh-CN')), revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991)) STRICT";

const INITIAL_MIGRATION: &str = include_str!("migrations/001_initial.sql");

fn migrations() -> rusqlite_migration::Migrations<'static> {
    rusqlite_migration::Migrations::new(vec![
        rusqlite_migration::M::up(INITIAL_MIGRATION),
        rusqlite_migration::M::up(include_str!("migrations/002_file_editor.sql")),
        rusqlite_migration::M::up(include_str!("migrations/003_monitor_display.sql")),
        rusqlite_migration::M::up(include_str!("migrations/004_monitor_presets.sql")),
        rusqlite_migration::M::up(include_str!("migrations/005_system_notifications.sql")),
        rusqlite_migration::M::up(include_str!(
            "migrations/006_remove_completion_notifications.sql"
        )),
    ])
}

pub struct SqlitePreferences {
    connection: Connection,
    _lock: Option<std::fs::File>,
    data_root: Option<std::path::PathBuf>,
    recordings: Option<recordings::RecordingStore>,
    configuration_cipher:
        Option<std::sync::Arc<dyn vigilshell_application_contracts::ports::ConnectionConfigCipher>>,
}
impl SqlitePreferences {
    /// 数据根目录归一后先取得内核独占锁，再打开应用数据库。
    pub fn open(data_dir: &Path) -> Result<Self, SettingsError> {
        if !data_dir.is_absolute() {
            return Err(SettingsError::StorageUnavailable);
        }
        if data_dir.join("backend-vault-reset.pending.json").exists() {
            return Err(SettingsError::StorageUnavailable);
        }
        fs::create_dir_all(data_dir).map_err(|_| SettingsError::StorageUnavailable)?;
        let root = fs::canonicalize(data_dir).map_err(|_| SettingsError::StorageUnavailable)?;
        let lock_path = root.join("runtime.lock");
        let database_path = root.join(DATABASE_FILENAME);
        validate_file(&lock_path)?;
        validate_file(&database_path)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|_| SettingsError::StorageUnavailable)?;
        lock.try_lock()
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let connection = Connection::open_with_flags(
            database_path,
            rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|_| SettingsError::StorageUnavailable)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let journal: String = connection
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        if journal != "delete" {
            return Err(SettingsError::StorageUnavailable);
        }
        let mut repository = Self::initialize(connection)?;
        repository._lock = Some(lock);
        repository.data_root = Some(root);
        Ok(repository)
    }
    fn initialize(mut connection: Connection) -> Result<Self, SettingsError> {
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let application_id: u32 = connection
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let integrity: String = connection
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
            .map_err(|_| SettingsError::StorageUnavailable)?;
        if integrity != "ok" {
            return Err(SettingsError::StorageUnavailable);
        }
        if version == 0 && application_id == 0 {
            let existing: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| SettingsError::StorageUnavailable)?;
            if existing != 0 {
                return Err(SettingsError::StorageUnavailable);
            }
            migrations()
                .to_latest(&mut connection)
                .map_err(|_| SettingsError::StorageUnavailable)?;
        } else if application_id == APPLICATION_ID && (1..SCHEMA_VERSION).contains(&version) {
            let mut expected =
                Connection::open_in_memory().map_err(|_| SettingsError::StorageUnavailable)?;
            migrations()
                .to_version(&mut expected, version as usize)
                .map_err(|_| SettingsError::StorageUnavailable)?;
            if !validate_against(&connection, &expected)
                .map_err(|_| SettingsError::StorageUnavailable)?
            {
                return Err(SettingsError::StorageUnavailable);
            }
            migrations()
                .to_latest(&mut connection)
                .map_err(|_| SettingsError::StorageUnavailable)?;
        } else if application_id != APPLICATION_ID || version != SCHEMA_VERSION {
            return Err(SettingsError::StorageUnavailable);
        }
        if !validate_schema(&connection).map_err(|_| SettingsError::StorageUnavailable)? {
            return Err(SettingsError::StorageUnavailable);
        }
        let repository = Self {
            connection,
            _lock: None,
            data_root: None,
            recordings: None,
            configuration_cipher: None,
        };
        repository.get()?;
        Ok(repository)
    }
}
fn validate_file(path: &Path) -> Result<(), SettingsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(SettingsError::StorageUnavailable),
    }
}

// 仅接纳当前版本的规范表结构，不把同名的外来表当作应用存储。
fn validate_schema(connection: &Connection) -> rusqlite::Result<bool> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(INITIAL_MIGRATION)?;
    expected.execute_batch(include_str!("migrations/002_file_editor.sql"))?;
    expected.execute_batch(include_str!("migrations/003_monitor_display.sql"))?;
    expected.execute_batch(include_str!("migrations/004_monitor_presets.sql"))?;
    expected.execute_batch(include_str!("migrations/005_system_notifications.sql"))?;
    expected.execute_batch(include_str!(
        "migrations/006_remove_completion_notifications.sql"
    ))?;
    if !validate_against(connection, &expected)? {
        return Ok(false);
    }
    let marker: Vec<(i64, i64)> = connection
        .prepare("SELECT id,generation FROM schema_baseline")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(marker == [(1, 8)]
        && agent::validate_rows(connection)?
        && vault::validate_rows(connection)?
        && workspace::validate_rows(connection)?
        && connection_configuration::validate_rows(connection)?
        && ui_state::validate_rows(connection)?)
}
fn validate_against(connection: &Connection, expected: &Connection) -> rusqlite::Result<bool> {
    fn objects(c: &Connection) -> rusqlite::Result<Vec<(String, String, String, String)>> {
        let mut q = c.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name")?;
        q.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, String>(3)?
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        })?
        .collect()
    }
    let rows: i64 = connection.query_row("SELECT COUNT(*) FROM preferences", [], |r| r.get(0))?;
    let foreign_keys_ok = connection
        .prepare("PRAGMA foreign_key_check")?
        .query([])?
        .next()?
        .is_none();
    Ok(objects(connection)? == objects(expected)? && rows == 1 && foreign_keys_ok)
}

fn read(connection: &Connection) -> Result<Preferences, SettingsError> {
    let value=connection.query_row("SELECT theme_mode,locale_mode,revision,terminal_font_size,terminal_line_height,terminal_scrollback,terminal_cursor_style,terminal_cursor_blink,connection_open_behavior,files_show_hidden,files_show_tree,files_columns,download_mode,download_directory,monitor_sample_seconds,monitor_full_seconds,terminal_copy_on_select,terminal_theme_mode,terminal_font_family,sidebar_hover_preview,files_auto_open,remote_directory_mode,system_notification_mode,attention_required_notification_enabled,file_editor_application,monitor_display,execution_issue_notification_enabled FROM preferences WHERE id=1",[],|r| {
        Ok(Preferences {
            theme:match r.get::<_,String>(0)?.as_str() {"system"=>ThemeMode::System,"light"=>ThemeMode::Light,"dark"=>ThemeMode::Dark,_=>return Err(rusqlite::Error::InvalidQuery)},
            locale:match r.get::<_,String>(1)?.as_str() {"system"=>LocaleMode::System,"en"=>LocaleMode::English,"zh-CN"=>LocaleMode::Chinese,_=>return Err(rusqlite::Error::InvalidQuery)},
            revision:u64::try_from(r.get::<_,i64>(2)?).map_err(|_|rusqlite::Error::InvalidQuery)?,
            terminal_font_size: r.get(3)?,
            terminal_line_height: r.get(4)?,
            terminal_scrollback: r.get(5)?,
            terminal_cursor_style: r.get(6)?,
            terminal_cursor_blink: r.get(7)?,
            connection_open_behavior: r.get(8)?,
            files_show_hidden: r.get(9)?,
            files_show_tree: r.get(10)?,
            files_columns: serde_json::from_str(&r.get::<_,String>(11)?).map_err(|_|rusqlite::Error::InvalidQuery)?,
            download_mode: r.get(12)?,
            download_directory: r.get(13)?,
            monitor_sample_seconds: r.get(14)?,
            monitor_full_seconds: r.get(15)?,
            terminal_copy_on_select: r.get(16)?,
            terminal_theme_mode: r.get(17)?,
            terminal_font_family: r.get(18)?,
            sidebar_hover_preview: r.get(19)?,
            files_auto_open: r.get(20)?,
            remote_directory_mode: r.get(21)?,
            system_notification_mode: NotificationMode::parse(&r.get::<_,String>(22)?).ok_or(rusqlite::Error::InvalidQuery)?,
            attention_required_notification_enabled: r.get(23)?,
            execution_issue_notification_enabled: r.get(26)?,
            file_editor_application: r.get(24)?,
            monitor_display: r.get(25)?,

        })
    }).map_err(|_|SettingsError::StorageUnavailable)?;
    if value.revision > MAX_REVISION {
        return Err(SettingsError::StorageUnavailable);
    }
    value
        .validate()
        .map_err(|_| SettingsError::StorageUnavailable)?;
    Ok(value)
}

impl PreferencesRepository for SqlitePreferences {
    fn ui_state(
        &self,
        scope: &str,
    ) -> Result<vigilshell_domain::ui_state::UiStateRecord, SettingsError> {
        ui_state::read_state(&self.connection, scope)
    }
    fn update_ui_state(
        &mut self,
        request: vigilshell_domain::ui_state::UiStateUpdate,
    ) -> Result<vigilshell_domain::ui_state::UiStateRecord, SettingsError> {
        ui_state::update_state(&mut self.connection, request)
    }

    fn get(&self) -> Result<Preferences, SettingsError> {
        read(&self.connection)
    }
    fn update(
        &mut self,
        request: UpdatePreferencesRequest,
    ) -> Result<PreferencesUpdate, SettingsError> {
        request.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| SettingsError::StorageUnavailable)?;
        let result = request.merge(&read(&tx)?)?;
        if !result.applied {
            return Ok(result);
        }
        let theme = match result.preferences.theme {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        };
        let locale = match result.preferences.locale {
            LocaleMode::System => "system",
            LocaleMode::English => "en",
            LocaleMode::Chinese => "zh-CN",
        };
        tx.execute("UPDATE preferences SET theme_mode=?1,locale_mode=?2,revision=?3,terminal_font_size=?4,terminal_line_height=?5,terminal_scrollback=?6,terminal_cursor_style=?7,terminal_cursor_blink=?8,connection_open_behavior=?9,files_show_hidden=?10,files_show_tree=?11,files_columns=?12,download_mode=?13,download_directory=?14,monitor_sample_seconds=?15,monitor_full_seconds=?16,terminal_copy_on_select=?17,terminal_theme_mode=?18,terminal_font_family=?19,sidebar_hover_preview=?20,files_auto_open=?21,remote_directory_mode=?22,system_notification_mode=?23,attention_required_notification_enabled=?24,file_editor_application=?25,monitor_display=?26,execution_issue_notification_enabled=?27 WHERE id=1", params![theme,locale,result.preferences.revision as i64,&result.preferences.terminal_font_size,&result.preferences.terminal_line_height,&result.preferences.terminal_scrollback,&result.preferences.terminal_cursor_style,&result.preferences.terminal_cursor_blink,&result.preferences.connection_open_behavior,&result.preferences.files_show_hidden,&result.preferences.files_show_tree,serde_json::to_string(&result.preferences.files_columns).map_err(|_|SettingsError::InvalidRequest)?,&result.preferences.download_mode,&result.preferences.download_directory,&result.preferences.monitor_sample_seconds,&result.preferences.monitor_full_seconds,&result.preferences.terminal_copy_on_select,&result.preferences.terminal_theme_mode,&result.preferences.terminal_font_family,&result.preferences.sidebar_hover_preview,&result.preferences.files_auto_open,&result.preferences.remote_directory_mode,result.preferences.system_notification_mode.as_str(),result.preferences.attention_required_notification_enabled,&result.preferences.file_editor_application,&result.preferences.monitor_display,result.preferences.execution_issue_notification_enabled]).map_err(|_|SettingsError::StorageUnavailable)?;
        tx.commit().map_err(|_| SettingsError::StorageUnavailable)?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use vigilshell_domain::PreferencesPatch;
    fn req(
        theme: ThemeMode,
        locale: LocaleMode,
        expected_revision: u64,
    ) -> UpdatePreferencesRequest {
        UpdatePreferencesRequest {
            patch: PreferencesPatch {
                theme_mode: Some(theme),
                locale_mode: Some(locale),
                ..Default::default()
            },
            base: PreferencesPatch {
                theme_mode: Some(ThemeMode::System),
                locale_mode: Some(LocaleMode::System),
                ..Default::default()
            },
            expected_revision,
        }
    }
    #[test]
    fn removing_completion_notifications_preserves_preferences_and_encrypted_data() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations().to_version(&mut connection, 5).unwrap();
        connection.execute_batch("UPDATE preferences SET system_notification_mode='always', attention_required_notification_enabled=0, execution_issue_notification_enabled=0, theme_mode='dark', terminal_font_size=16, revision=23;
            UPDATE credential_vault SET initialized=1;
            UPDATE connection_configuration SET verifier=zeroblob(48);
            INSERT INTO connection_profiles(id,group_sort_order,version,revision,credential_ref,config) VALUES('11111111-1111-4111-8111-111111111111',0,1,1,'22222222-2222-4222-8222-222222222222',zeroblob(64));
            INSERT INTO encrypted_credentials VALUES('22222222-2222-4222-8222-222222222222','11111111-1111-4111-8111-111111111111','ssh_password',1,1,zeroblob(24),zeroblob(32));").unwrap();
        let repository = SqlitePreferences::initialize(connection).unwrap();
        let preferences = repository.get().unwrap();
        assert_eq!(
            preferences.system_notification_mode,
            NotificationMode::Always
        );
        assert!(!preferences.attention_required_notification_enabled);
        assert!(!preferences.execution_issue_notification_enabled);
        assert_eq!(preferences.theme, ThemeMode::Dark);
        assert_eq!(preferences.terminal_font_size, 16);
        assert_eq!(preferences.revision, 23);
        let retired: u32 = repository.connection.query_row(
            "SELECT count(*) FROM pragma_table_info('preferences') WHERE name='task_completion_notification_enabled'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(retired, 0);
        let (config, ciphertext): (Vec<u8>, Vec<u8>) = repository.connection.query_row(
            "SELECT config,ciphertext FROM connection_profiles JOIN encrypted_credentials ON credential_ref=reference", [], |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(config, vec![0; 64]);
        assert_eq!(ciphertext, vec![0; 32]);
        let mut reopened = SqlitePreferences::initialize(repository.connection).unwrap();
        assert_eq!(reopened.get().unwrap(), preferences);
        let updated = reopened
            .update(UpdatePreferencesRequest {
                expected_revision: 23,
                patch: vigilshell_domain::PreferencesPatch {
                    execution_issue_notification_enabled: Some(true),
                    ..Default::default()
                },
                base: vigilshell_domain::PreferencesPatch {
                    execution_issue_notification_enabled: Some(false),
                    ..Default::default()
                },
            })
            .unwrap();
        assert!(updated.preferences.execution_issue_notification_enabled);
        assert!(!updated.preferences.attention_required_notification_enabled);
        assert_eq!(
            SqlitePreferences::initialize(reopened.connection)
                .unwrap()
                .get()
                .unwrap(),
            updated.preferences
        );
    }

    #[test]
    fn notification_migration_resets_only_notification_choices_once() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations().to_version(&mut connection, 4).unwrap();
        connection.execute("UPDATE preferences SET task_completion_notification_mode='off', attention_required_notification_enabled=0, theme_mode='dark', revision=9", []).unwrap();
        let repository = SqlitePreferences::initialize(connection).unwrap();
        let values = repository.get().unwrap();
        assert_eq!(values.system_notification_mode, NotificationMode::Unfocused);
        assert!(
            values.attention_required_notification_enabled
                && values.execution_issue_notification_enabled
        );
        assert_eq!(values.theme, ThemeMode::Dark);
        assert_eq!(values.revision, 9);
        repository.connection.execute("UPDATE preferences SET system_notification_mode='off', execution_issue_notification_enabled=0", []).unwrap();
        let reopened = SqlitePreferences::initialize(repository.connection)
            .unwrap()
            .get()
            .unwrap();
        assert_eq!(reopened.system_notification_mode, NotificationMode::Off);
        assert!(!reopened.execution_issue_notification_enabled);
    }

    #[test]
    fn baseline_and_failed_future_migration_preserve_current_schema() {
        use rusqlite_migration::{M, Migrations};
        migrations().validate().unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut connection).unwrap();
        connection
            .execute("UPDATE preferences SET revision = 7", [])
            .unwrap();
        let broken = Migrations::new(vec![
            M::up(INITIAL_MIGRATION),
            M::up(include_str!("migrations/002_file_editor.sql")),
            M::up(include_str!("migrations/003_monitor_display.sql")),
            M::up(include_str!("migrations/004_monitor_presets.sql")),
            M::up(include_str!("migrations/005_system_notifications.sql")),
            M::up(include_str!(
                "migrations/006_remove_completion_notifications.sql"
            )),
            M::up("CREATE TABLE transient(value TEXT); THIS IS NOT SQL;"),
        ]);
        assert!(broken.to_latest(&mut connection).is_err());
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let revision: i64 = connection
            .query_row("SELECT revision FROM preferences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(revision, 7);
        let extra: u32 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='transient'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(extra, 0);
    }

    #[test]
    fn editor_preference_migration_preserves_existing_values() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(INITIAL_MIGRATION).unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection
            .execute("UPDATE preferences SET revision=17,theme_mode='dark'", [])
            .unwrap();
        let repository = SqlitePreferences::initialize(connection).unwrap();
        let value = repository.get().unwrap();
        assert_eq!(value.revision, 17);
        assert_eq!(value.theme, ThemeMode::Dark);
        assert!(value.file_editor_application.is_empty());
        assert_eq!(
            repository
                .connection
                .pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }
    #[test]
    fn retired_monitor_mode_does_not_affect_preferences_or_other_values() {
        let mut repository =
            SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        repository.connection.execute(
            "UPDATE preferences SET monitor_auto_refresh=0, monitor_sample_seconds=15, monitor_full_seconds=120, revision=7, terminal_font_size=16",
            [],
        ).unwrap();
        let preferences = repository.get().unwrap();
        assert_eq!(preferences.monitor_sample_seconds, 15);
        assert_eq!(preferences.monitor_full_seconds, 120);
        assert_eq!(preferences.terminal_font_size, 16);
        assert_eq!(preferences.revision, 7);
        let updated = repository
            .update(UpdatePreferencesRequest {
                patch: PreferencesPatch {
                    monitor_sample_seconds: Some(15),
                    ..Default::default()
                },
                base: PreferencesPatch {
                    monitor_sample_seconds: Some(15),
                    ..Default::default()
                },
                expected_revision: 7,
            })
            .unwrap();
        assert_eq!(updated.preferences.monitor_sample_seconds, 15);
        assert_eq!(updated.preferences.monitor_full_seconds, 120);
        assert_eq!(updated.preferences.terminal_font_size, 16);
        assert_eq!(updated.preferences.revision, 7);
        assert_eq!(repository.get().unwrap(), updated.preferences);
    }

    #[test]
    fn monitor_display_defaults_on_upgrade_and_survives_reopen() {
        let root = TestDirectory::new();
        let mut connection = Connection::open(root.path().join(DATABASE_FILENAME)).unwrap();
        migrations().to_version(&mut connection, 2).unwrap();
        connection
            .execute(
                "UPDATE preferences SET revision=17,theme_mode='dark',monitor_sample_seconds=10",
                [],
            )
            .unwrap();
        drop(connection);
        let mut repository = SqlitePreferences::open(root.path()).unwrap();
        let before = repository.get().unwrap();
        assert_eq!(before.monitor_display, "bar");
        assert_eq!(before.revision, 17);
        assert_eq!(before.theme, ThemeMode::Dark);
        assert_eq!(before.monitor_sample_seconds, 5);
        let request = UpdatePreferencesRequest {
            patch: PreferencesPatch {
                monitor_display: Some("chart".into()),
                ..Default::default()
            },
            base: PreferencesPatch {
                monitor_display: Some("bar".into()),
                ..Default::default()
            },
            expected_revision: 17,
        };
        let after = repository.update(request.clone()).unwrap().preferences;
        assert_eq!(after.monitor_display, "chart");
        assert_eq!(after.revision, 18);
        assert!(!repository.update(request.clone()).unwrap().applied);
        let mut invalid = request;
        invalid.patch.monitor_display = Some("unknown".into());
        assert_eq!(
            repository.update(invalid),
            Err(SettingsError::InvalidRequest)
        );
        drop(repository);
        let reopened = SqlitePreferences::open(root.path()).unwrap();
        assert_eq!(reopened.get().unwrap(), after);
    }

    #[test]
    fn monitor_presets_upgrade_old_periods_without_changing_other_preferences() {
        for (old, sample, full) in [(5, 5, 60), (10, 5, 60), (30, 15, 120), (60, 15, 120)] {
            let mut connection = Connection::open_in_memory().unwrap();
            migrations().to_version(&mut connection, 3).unwrap();
            connection.execute("UPDATE preferences SET monitor_sample_seconds=?1, monitor_full_seconds=300, monitor_display='chart', revision=7", [old]).unwrap();
            let repository = SqlitePreferences::initialize(connection).unwrap();
            let p = repository.get().unwrap();
            assert_eq!(
                (p.monitor_sample_seconds, p.monitor_full_seconds),
                (sample, full)
            );
            assert_eq!(p.monitor_display, "chart");
            assert_eq!(p.revision, 7);
        }
    }

    #[test]
    fn defaults_conflicts_and_transaction_failure() {
        let mut r = SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(r.get().unwrap(), Preferences::default());
        let saved = r
            .update(req(ThemeMode::Dark, LocaleMode::Chinese, 0))
            .unwrap();
        assert_eq!(saved.preferences.revision, 1);
        assert_eq!(
            r.update(req(ThemeMode::Light, LocaleMode::English, 0)),
            Err(SettingsError::Conflict(Box::new(saved.preferences.clone())))
        );
        assert_eq!(r.get().unwrap(), saved.preferences);
        r.connection
            .pragma_update(None, "query_only", true)
            .unwrap();
        assert_eq!(
            r.update(req(ThemeMode::Light, LocaleMode::English, 1)),
            Err(SettingsError::StorageUnavailable)
        );
        assert_eq!(r.get().unwrap(), saved.preferences);
    }
    struct TestDirectory(std::path::PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "vigilshell-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn persists_and_releases_exclusive_directory_lock() {
        let root = TestDirectory::new();
        let mut repository = SqlitePreferences::open(root.path()).unwrap();
        repository
            .update(req(ThemeMode::Light, LocaleMode::English, 0))
            .unwrap();
        assert!(SqlitePreferences::open(root.path()).is_err());
        drop(repository);
        let repository = SqlitePreferences::open(root.path()).unwrap();
        assert_eq!(repository.get().unwrap().locale, LocaleMode::English);
        assert_eq!(repository.get().unwrap().revision, 1);
    }
    #[test]
    fn rejects_corrupt_and_foreign_files_without_modification() {
        for corrupt in [false, true] {
            let root = TestDirectory::new();
            let path = root.path().join(DATABASE_FILENAME);
            if corrupt {
                fs::write(&path, b"not a sqlite database").unwrap();
            } else {
                let db = Connection::open(&path).unwrap();
                db.execute_batch("CREATE TABLE unrelated(value TEXT)")
                    .unwrap();
            }
            let original = fs::read(&path).unwrap();
            assert!(SqlitePreferences::open(root.path()).is_err());
            assert_eq!(fs::read(path).unwrap(), original);
        }
    }
    #[test]
    fn rejects_unknown_identity_version_schema_and_missing_preferences() {
        for change in [
            "PRAGMA application_id=0x57444731",
            "PRAGMA user_version=1",
            "PRAGMA user_version=2",
            "PRAGMA user_version=7",
            "DELETE FROM schema_baseline",
            "CREATE TABLE extra(value TEXT)",
            "ALTER TABLE preferences ADD COLUMN extra TEXT",
            "DELETE FROM preferences",
        ] {
            let root = TestDirectory::new();
            let repository = SqlitePreferences::open(root.path()).unwrap();
            repository.connection.execute_batch(change).unwrap();
            drop(repository);
            let path = root.path().join(DATABASE_FILENAME);
            let original = fs::read(&path).unwrap();
            assert!(SqlitePreferences::open(root.path()).is_err());
            assert_eq!(fs::read(path).unwrap(), original);
        }
        let root = TestDirectory::new();
        let db = Connection::open(root.path().join(DATABASE_FILENAME)).unwrap();
        db.execute_batch(&PREFERENCES_SCHEMA.replace(" STRICT", ""))
            .unwrap();
        db.execute_batch("INSERT INTO preferences VALUES(1,'system','system',0)")
            .unwrap();
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        drop(db);
        assert!(SqlitePreferences::open(root.path()).is_err());
    }
    #[test]
    fn rejects_relative_roots_and_non_file_entries() {
        assert!(SqlitePreferences::open(Path::new("relative-data")).is_err());
        for name in [DATABASE_FILENAME, "runtime.lock"] {
            let root = TestDirectory::new();
            fs::create_dir(root.path().join(name)).unwrap();
            assert!(SqlitePreferences::open(root.path()).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_files_and_locks_directory_aliases() {
        use std::os::unix::fs::symlink;
        for name in [DATABASE_FILENAME, "runtime.lock"] {
            let root = TestDirectory::new();
            let target = root.path().join("unrelated");
            fs::write(&target, b"preserve").unwrap();
            symlink(&target, root.path().join(name)).unwrap();
            assert!(SqlitePreferences::open(root.path()).is_err());
            assert_eq!(fs::read(target).unwrap(), b"preserve");
        }
        let temp = TestDirectory::new();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let alias = temp.path().join("alias");
        symlink(&root, &alias).unwrap();
        let owner = SqlitePreferences::open(&root).unwrap();
        assert!(SqlitePreferences::open(&alias).is_err());
        drop(owner);
        assert!(SqlitePreferences::open(&alias).is_ok());
    }
}

mod agent;
pub mod agent_configuration_conversion;
#[cfg(test)]
mod agent_configuration_tests;
mod agent_history;

mod execution_tasks;

#[cfg(test)]
mod test_support;
