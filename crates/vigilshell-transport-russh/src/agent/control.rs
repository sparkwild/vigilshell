use super::*;
impl Connection {
    pub(super) async fn control_bounded(
        &self,
        command: String,
        limit: usize,
    ) -> Result<String, WorkspaceError> {
        tokio::time::timeout(Duration::from_secs(45), async {
            let mut process = self
                .open_exec_encoded(command, false, false, encoding_rs::UTF_8)
                .await?;
            let mut output = Vec::new();
            let mut code = None;
            loop {
                match process.read().await? {
                    AgentProcessEvent::Stdout(bytes) => {
                        if output.len() + bytes.len() > limit {
                            return Err(WorkspaceError::StreamUnavailable);
                        }
                        output.extend(bytes);
                    }
                    AgentProcessEvent::Exit(value) => code = Some(value),
                    AgentProcessEvent::Closed => break,
                    AgentProcessEvent::Stderr(_) | AgentProcessEvent::Eof => {}
                    AgentProcessEvent::ExitSignal { .. } => {
                        return Err(WorkspaceError::StreamUnavailable);
                    }
                }
            }
            if code != Some(0) {
                return Err(WorkspaceError::ToolsUnavailable);
            }
            String::from_utf8(output).map_err(|_| WorkspaceError::UnmappableInput)
        })
        .await
        .map_err(|_| WorkspaceError::Timeout)?
    }
}
