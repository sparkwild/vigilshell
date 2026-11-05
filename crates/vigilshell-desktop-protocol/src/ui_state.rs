use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vigilshell_domain::ui_state as domain;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[schemars(extend("x-vigil-rule" = "uiState"))]
pub enum UiStateData {
    Directory {
        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        connection_revision: u64,
        #[schemars(extend("maxLength" = 4096))]
        path: String,
        #[schemars(extend("maximum" = 4294967295_u64, "minimum" = 0))]
        scroll: u32,
        #[schemars(extend("maxItems" = 64))]
        expanded: Vec<String>,
    },
    Pickers {
        #[schemars(extend("maxLength" = 4096))]
        upload: String,
        #[schemars(extend("maxLength" = 4096))]
        download: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "uiScope"))]
pub struct UiStateRecord {
    pub scope: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u64,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<UiStateData>")]
    pub data: Option<UiStateData>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "uiScope"))]
pub struct UiStateUpdate {
    pub scope: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub expected_revision: u64,
    pub data: UiStateData,
}
impl From<UiStateData> for domain::UiStateData {
    fn from(value: UiStateData) -> Self {
        match value {
            UiStateData::Directory {
                connection_revision,
                path,
                scroll,
                expanded,
            } => Self::Directory {
                connection_revision,
                path,
                scroll,
                expanded,
            },
            UiStateData::Pickers { upload, download } => Self::Pickers { upload, download },
        }
    }
}
impl From<domain::UiStateData> for UiStateData {
    fn from(value: domain::UiStateData) -> Self {
        match value {
            domain::UiStateData::Directory {
                connection_revision,
                path,
                scroll,
                expanded,
            } => Self::Directory {
                connection_revision,
                path,
                scroll,
                expanded,
            },
            domain::UiStateData::Pickers { upload, download } => Self::Pickers { upload, download },
        }
    }
}
impl From<UiStateUpdate> for domain::UiStateUpdate {
    fn from(v: UiStateUpdate) -> Self {
        Self {
            scope: v.scope,
            expected_revision: v.expected_revision,
            data: v.data.into(),
        }
    }
}
impl From<domain::UiStateRecord> for UiStateRecord {
    fn from(v: domain::UiStateRecord) -> Self {
        Self {
            scope: v.scope,
            revision: v.revision,
            data: v.data.map(Into::into),
        }
    }
}
