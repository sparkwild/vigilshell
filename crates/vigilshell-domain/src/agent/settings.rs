use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AgentClient {
    Codex,
    ClaudeCode,
    Cursor,
    Dsh,
    Generic,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AgentDelivery {
    Configuration,
    Prompt,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "group",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// 只比较并更新所选分组，其他分组的并发修改不会被旧快照覆盖。
pub enum AgentSettingsPatch {
    Service {
        enabled: bool,
        port: u16,
        base_enabled: bool,
        base_port: u16,
    },
    Activity {
        retention_days: u32,
        storage_mib: u32,
        base_retention_days: u32,
        base_storage_mib: u32,
    },
    Tray {
        show_tray: bool,
        base_show_tray: bool,
    },
}
impl AgentSettingsPatch {
    pub fn apply(&self, settings: &AgentSettings) -> AgentResult<AgentSettings> {
        let mut next = settings.clone();
        match *self {
            Self::Service {
                enabled,
                port,
                base_enabled,
                base_port,
            } => {
                if settings.enabled != base_enabled || settings.port != base_port {
                    return Err("configuration_conflict");
                }
                next.enabled = enabled;
                next.port = port;
            }
            Self::Activity {
                retention_days,
                storage_mib,
                base_retention_days,
                base_storage_mib,
            } => {
                if settings.retention_days != base_retention_days
                    || settings.storage_mib != base_storage_mib
                {
                    return Err("configuration_conflict");
                }
                if !(1..=365).contains(&retention_days) || !(16..=1024).contains(&storage_mib) {
                    return Err("invalid_settings");
                }
                next.retention_days = retention_days;
                next.storage_mib = storage_mib;
            }
            Self::Tray {
                show_tray,
                base_show_tray,
            } => {
                if settings.show_tray != base_show_tray {
                    return Err("configuration_conflict");
                }
                next.show_tray = show_tray;
            }
        }
        Ok(next)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// 记录成功接入历史；地址版本使端口改回旧值时仍需重新验证。
pub struct AccessObservation {
    #[serde(default)]
    pub endpoint_revision: u64,
    pub entry_id: String,
    pub generation: u64,
    pub endpoint: String,
    pub first_at: u64,
    pub last_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentClientDetection {
    pub client: AgentClient,
    pub installed: bool,
    pub configuration_found: bool,
    pub unavailable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_patch_preserves_unrelated_changes_and_rejects_same_group_conflicts() {
        let initial = AgentSettings::default();
        let activity = AgentSettingsPatch::Activity {
            retention_days: 90,
            storage_mib: 128,
            base_retention_days: 30,
            base_storage_mib: 1024,
        };
        let mut current = initial.clone();
        current.port = 52000;
        current.show_tray = false;
        let result = activity.apply(&current).unwrap();
        assert_eq!(result.port, 52000);
        assert!(!result.show_tray);
        assert_eq!(result.retention_days, 90);
        current.retention_days = 7;
        assert!(activity.apply(&current).is_err());
    }
}
