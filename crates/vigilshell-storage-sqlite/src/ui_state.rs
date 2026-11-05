//! 展示状态与偏好共用 SQLite 工作线程，但拥有独立 revision。
use super::*;
use rusqlite::OptionalExtension;
use vigilshell_domain::ui_state::{UiStateData, UiStateRecord, UiStateUpdate, valid_scope};
pub(super) fn read_state(db: &Connection, scope: &str) -> Result<UiStateRecord, SettingsError> {
    if !valid_scope(scope) {
        return Err(SettingsError::InvalidRequest);
    }
    let row: Option<(i64, String)> = db
        .query_row(
            "SELECT revision,data FROM ui_state WHERE scope=?1",
            [scope],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|_| SettingsError::StorageUnavailable)?;
    match row {
        None => Ok(UiStateRecord {
            scope: scope.into(),
            revision: 0,
            data: None,
        }),
        Some((revision, json)) => {
            let revision =
                u64::try_from(revision).map_err(|_| SettingsError::StorageUnavailable)?;
            let data =
                serde_json::from_str(&json).map_err(|_| SettingsError::StorageUnavailable)?;
            let validated = UiStateUpdate {
                scope: scope.into(),
                expected_revision: revision,
                data,
            };
            validated
                .validate()
                .map_err(|_| SettingsError::StorageUnavailable)?;
            Ok(UiStateRecord {
                scope: scope.into(),
                revision,
                data: Some(validated.data),
            })
        }
    }
}
pub(super) fn validate_rows(db: &Connection) -> rusqlite::Result<bool> {
    let scopes = db
        .prepare("SELECT scope FROM ui_state")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(scopes.iter().all(|scope| read_state(db, scope).is_ok()))
}
pub(super) fn update_state(
    db: &mut Connection,
    request: UiStateUpdate,
) -> Result<UiStateRecord, SettingsError> {
    request.validate()?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| SettingsError::StorageUnavailable)?;
    if let UiStateData::Directory {
        connection_revision,
        ..
    } = &request.data
    {
        let id = request
            .scope
            .strip_prefix("connection:")
            .ok_or(SettingsError::InvalidRequest)?;
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM connection_profiles WHERE id=?1 AND deleting=0",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| SettingsError::StorageUnavailable)?;
        if revision != Some(*connection_revision as i64) {
            return Err(SettingsError::InvalidRequest);
        }
    }
    let current = read_state(&tx, &request.scope)?;
    if current.data.as_ref() == Some(&request.data) {
        return Ok(current);
    }
    if current.revision != request.expected_revision {
        return Err(SettingsError::InvalidRequest);
    }
    if current.revision == MAX_REVISION {
        return Err(SettingsError::RevisionLimit);
    }
    let revision = current.revision + 1;
    let json = serde_json::to_string(&request.data).map_err(|_| SettingsError::InvalidRequest)?;
    tx.execute("INSERT INTO ui_state(scope,revision,data) VALUES(?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET revision=excluded.revision,data=excluded.data",params![request.scope,revision as i64,json]).map_err(|_|SettingsError::StorageUnavailable)?;
    tx.commit().map_err(|_| SettingsError::StorageUnavailable)?;
    Ok(UiStateRecord {
        scope: request.scope,
        revision,
        data: Some(request.data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pickers(value: u16) -> UiStateData {
        UiStateData::Pickers {
            upload: format!("/tmp/{value}"),
            download: String::new(),
        }
    }
    #[test]
    fn independent_revisions_and_stale_writes_never_change_preferences() {
        let mut repo =
            SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let before = repo.get().unwrap();
        let saved = repo
            .update_ui_state(UiStateUpdate {
                scope: "pickers".into(),
                expected_revision: 0,
                data: pickers(220),
            })
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(repo.get().unwrap(), before);
        assert!(
            repo.update_ui_state(UiStateUpdate {
                scope: "pickers".into(),
                expected_revision: 0,
                data: pickers(240)
            })
            .is_err()
        );
        assert_eq!(repo.ui_state("pickers").unwrap(), saved);
        assert_eq!(
            repo.update_ui_state(UiStateUpdate {
                scope: "pickers".into(),
                expected_revision: 1,
                data: pickers(220)
            })
            .unwrap()
            .revision,
            1
        );
        assert!(
            repo.update_ui_state(UiStateUpdate {
                scope: "window".into(),
                expected_revision: 0,
                data: pickers(220)
            })
            .is_err()
        );
    }
    #[test]
    fn cannot_restore_deleted_or_changed_connection_state() {
        let mut repo =
            SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let id = "00000000-0000-0000-0000-000000000003";
        let request = UiStateUpdate {
            scope: format!("connection:{id}"),
            expected_revision: 0,
            data: UiStateData::Directory {
                connection_revision: 1,
                path: "/home/test".into(),
                scroll: 20,
                expanded: vec![],
            },
        };
        assert!(repo.update_ui_state(request.clone()).is_err());
        repo.connection.execute("INSERT INTO connection_profiles(id,group_sort_order,version,revision,config) VALUES(?1,0,1,2,zeroblob(48))",[id]).unwrap();
        assert!(repo.update_ui_state(request).is_err());
    }

    #[test]
    fn unknown_layout_records_are_rejected_without_implicit_cleanup() {
        let repo = SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        repo.connection
            .execute("INSERT INTO ui_state VALUES('window',9,'{}')", [])
            .unwrap();
        assert!(SqlitePreferences::initialize(repo.connection).is_err());
    }
    #[test]
    fn six_preferences_roundtrip_and_preserve_existing_values() {
        use vigilshell_domain::{PreferencesPatch, UpdatePreferencesRequest};
        let mut repo =
            SqlitePreferences::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let result = repo
            .update(UpdatePreferencesRequest {
                patch: PreferencesPatch {
                    terminal_copy_on_select: Some(true),
                    terminal_theme_mode: Some("dark".into()),
                    terminal_font_family: Some("menlo".into()),
                    sidebar_hover_preview: Some(false),
                    files_auto_open: Some(false),
                    remote_directory_mode: Some("last".into()),
                    ..Default::default()
                },
                base: PreferencesPatch {
                    terminal_copy_on_select: Some(false),
                    terminal_theme_mode: Some("inherit".into()),
                    terminal_font_family: Some("jetbrains-mono".into()),
                    sidebar_hover_preview: Some(true),
                    files_auto_open: Some(true),
                    remote_directory_mode: Some("initial".into()),
                    ..Default::default()
                },
                expected_revision: 0,
            })
            .unwrap();
        let saved = repo.get().unwrap();
        assert_eq!(saved, result.preferences);
        assert!(saved.terminal_copy_on_select);
        assert_eq!(saved.terminal_theme_mode, "dark");
        assert_eq!(saved.terminal_font_family, "menlo");
        assert!(!saved.sidebar_hover_preview);
        assert!(!saved.files_auto_open);
        assert_eq!(saved.remote_directory_mode, "last");
        let reopened = SqlitePreferences::initialize(repo.connection).unwrap();
        assert_eq!(reopened.get().unwrap(), saved);
        assert_eq!(saved.terminal_font_size, 12);
        assert_eq!(saved.revision, 1);
    }
}
