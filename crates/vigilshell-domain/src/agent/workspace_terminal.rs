use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAgentControl {
    pub entry_id: String,
    pub entry_name: String,
    pub context_id: String,
    pub lease_id: String,
    pub releasing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceTerminalTab {
    pub id: String,
    pub name: String,
    pub connection_id: String,
    pub control: Option<WorkspaceAgentControl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceTerminalAction {
    ListTabs {},
    Takeover {
        tab_id: String,
        request_key: String,
    },
    ReadTab {
        tab_id: String,
        lease_id: String,
        after: u64,
    },
    WriteTab {
        tab_id: String,
        lease_id: String,
        request_key: String,
        text: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceTerminalReply {
    Tabs {
        tabs: Vec<WorkspaceTerminalTab>,
    },
    Control {
        tab: WorkspaceTerminalTab,
    },
    Output {
        text: String,
        cursor: u64,
        incomplete: bool,
    },
    Input {
        request_key: String,
    },
}
