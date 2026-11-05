use super::*;
#[derive(Clone, Debug)]
pub struct CheckedTerminal {
    directory: String,
    columns: u16,
    rows: u16,
}
impl CheckedTerminal {
    pub fn directory(&self) -> &str {
        &self.directory
    }
    pub fn columns(&self) -> u16 {
        self.columns
    }
    pub fn rows(&self) -> u16 {
        self.rows
    }
}
pub fn check_terminal(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<CheckedTerminal> {
    request.authorize(policy)?;
    let RuntimeOperation::Terminal {
        directory,
        columns,
        rows,
    } = &request.operation
    else {
        return Err("terminal_required");
    };
    Ok(CheckedTerminal {
        directory: normalize_path(
            "/",
            if directory.path.is_empty() {
                "."
            } else {
                &directory.path
            },
        )?,
        columns: *columns,
        rows: *rows,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum TerminalControl {
    Write { text: String },
    Resize { columns: u16, rows: u16 },
}
