use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuCommand {
    New,
    NewGroup,
    Connect,
    ConnectNew,
    EditConnection,
    Find,
    Close,
    Workbench,
    AgentPolicy,
    AgentActivity,
    ToggleSidebar,
    ToggleTools,
    ShowFiles,
    ShowMonitor,
    ShowTransfers,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeMenuCloseTarget {
    Dialog,
    Page,
    Tab,
    Window,
}

/// Presentation capabilities only; connection identities and business state stay in Renderer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeMenuState {
    #[schemars(length(max = 15))]
    pub enabled: Vec<NativeMenuCommand>,
    #[schemars(length(max = 15))]
    pub checked: Vec<NativeMenuCommand>,
    pub close_target: NativeMenuCloseTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_state_rejects_unknown_commands_and_fields() {
        let state: NativeMenuState = serde_json::from_str(
            r#"{"enabled":["newGroup","close"],"checked":[],"closeTarget":"page"}"#,
        )
        .unwrap();
        assert_eq!(
            state.enabled,
            vec![NativeMenuCommand::NewGroup, NativeMenuCommand::Close]
        );
        assert!(
            serde_json::from_str::<NativeMenuState>(
                r#"{"enabled":["shell"],"checked":[],"closeTarget":"page"}"#,
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<NativeMenuState>(
                r#"{"enabled":[],"checked":[],"closeTarget":"page","connectionId":"hidden"}"#,
            )
            .is_err()
        );
    }
}
