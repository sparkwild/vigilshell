//! 有界、按作用域保存的展示状态；不保存会话或执行授权。
use crate::{MAX_REVISION, SettingsError};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum UiStateData {
    Directory {
        connection_revision: u64,
        path: String,
        scroll: u32,
        expanded: Vec<String>,
    },
    Pickers {
        upload: String,
        download: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiStateRecord {
    pub scope: String,
    pub revision: u64,
    pub data: Option<UiStateData>,
}
#[derive(Debug, Clone)]
pub struct UiStateUpdate {
    pub scope: String,
    pub expected_revision: u64,
    pub data: UiStateData,
}

pub fn valid_scope(scope: &str) -> bool {
    scope == "pickers"
        || scope
            .strip_prefix("connection:")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
}
impl UiStateUpdate {
    pub fn validate(&self) -> Result<(), SettingsError> {
        let path_ok = |p: &str| p.len() <= 4096 && !p.contains('\0');
        let encoded_len = |p: &str| {
            p.chars()
                .map(|c| {
                    if c.is_control() {
                        6
                    } else if c == '"' || c == '\\' {
                        2
                    } else {
                        c.len_utf8()
                    }
                })
                .sum::<usize>()
                + 2
        };
        let valid = self.expected_revision <= MAX_REVISION
            && valid_scope(&self.scope)
            && match &self.data {
                UiStateData::Directory {
                    connection_revision,
                    path,
                    expanded,
                    ..
                } => {
                    self.scope.starts_with("connection:")
                        && *connection_revision <= MAX_REVISION
                        && path_ok(path)
                        && expanded.len() <= 64
                        && expanded.iter().all(|p| path_ok(p))
                        && expanded.iter().map(String::len).sum::<usize>() <= 24000
                        && encoded_len(path)
                            + expanded.iter().map(|p| encoded_len(p) + 1).sum::<usize>()
                            <= 31000
                }
                UiStateData::Pickers { upload, download } => {
                    self.scope == "pickers"
                        && path_ok(upload)
                        && path_ok(download)
                        && [upload, download]
                            .iter()
                            .all(|p| p.is_empty() || std::path::Path::new(p).is_absolute())
                }
            };
        if valid {
            Ok(())
        } else {
            Err(SettingsError::InvalidRequest)
        }
    }
}
