//! 仅为 Application 注入已有存储、连接和事件端口，不在 Backend 开放 MCP 监听。
use std::sync::Arc;
use tokio::sync::mpsc;
use vigilshell_application::{agent::AgentService, ports::*, workspace::WorkspaceService};
use vigilshell_domain::agent::AgentEvent;
struct Connector(Arc<WorkspaceService>);
impl AgentConnector for Connector {
    fn open_workspace(
        &self,
        identity: WorkspaceAgentIdentity,
        target: Option<String>,
    ) -> AgentFuture<'_, vigilshell_domain::agent::WorkspaceTerminalTab> {
        Box::pin(self.0.open_agent_terminal(identity, target))
    }
    fn workspace_tabs(
        &self,
        id: String,
        revision: u64,
    ) -> AgentFuture<'_, Vec<vigilshell_domain::agent::WorkspaceTerminalTab>> {
        Box::pin(async move { Ok(self.0.agent_tabs(&id, revision)) })
    }

    fn workspace_control_live(&self, context_id: String) -> AgentFuture<'_, bool> {
        Box::pin(async move { Ok(self.0.agent_control_live(&context_id)) })
    }

    fn workspace_terminal(
        &self,
        identity: WorkspaceAgentIdentity,
        command: vigilshell_domain::agent::WorkspaceTerminalAction,
    ) -> AgentFuture<'_, vigilshell_domain::agent::WorkspaceTerminalReply> {
        Box::pin(self.0.agent_terminal(identity, command))
    }
    fn release_workspace(&self, context_id: String, finish: bool) -> AgentFuture<'_, ()> {
        Box::pin(async move { self.0.release_agent_terminal(&context_id, finish).await })
    }

    fn connect(&self, id: String, revision: u64) -> FutureResult<'_, Arc<dyn AgentConnection>> {
        Box::pin(async move { self.0.connect_agent(&id, revision).await })
    }
}
struct Sink {
    output: mpsc::Sender<serde_json::Value>,
    runtime_id: String,
}
impl AgentSink for Sink {
    fn publish(&self, event: AgentEvent) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.output.send(serde_json::json!({"jsonrpc":"2.0","method":"agent.changed","params":{"runtimeId":self.runtime_id,"event":event}})).await.map_err(|_|vigilshell_domain::workspace::WorkspaceError::StreamUnavailable)
        })
    }
}
pub(crate) fn create(
    database: Arc<dyn WorkspaceDatabase>,
    workspace: Arc<WorkspaceService>,
    output: mpsc::Sender<serde_json::Value>,
    runtime_id: String,
    data_root: std::path::PathBuf,
) -> Arc<AgentService> {
    AgentService::new(
        database,
        Arc::new(Connector(workspace.clone())),
        Arc::new(Sink { output, runtime_id }),
        Arc::new(vigilshell_local_files::LocalFiles::for_application(
            data_root,
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vigilshell_domain::agent::{
        ActivityActor, ActivityBody, ActivityOutputFormat, ActivityRecord,
    };

    #[tokio::test]
    async fn activity_output_is_published_unchanged_on_the_agent_channel() {
        let (output, mut receiver) = mpsc::channel(2);
        let sink = Sink {
            output,
            runtime_id: "runtime".into(),
        };
        for format in [ActivityOutputFormat::Text, ActivityOutputFormat::Structured] {
            let event = AgentEvent::Activity {
                record: ActivityRecord {
                    session_id: "session".into(),
                    task_id: Some("task".into()),
                    parent_task_id: None,
                    sequence: 1,
                    timestamp_ms: 0,
                    actor: ActivityActor::Agent,
                    policy_revision: 1,
                    body: ActivityBody::Output {
                        format,
                        stream: "stdout".into(),
                        text: "{\"entries\":[\"file\"]}\n".into(),
                        data: None,
                    },
                },
            };
            let expected = serde_json::to_value(&event).unwrap();
            sink.publish(event).await.unwrap();
            let message = receiver.recv().await.unwrap();
            assert_eq!(message["method"], "agent.changed");
            assert_eq!(message["params"]["event"], expected);
        }
        drop(receiver);
        assert!(
            sink.publish(AgentEvent::Activity {
                record: ActivityRecord {
                    session_id: "session".into(),
                    task_id: None,
                    parent_task_id: None,
                    sequence: 2,
                    timestamp_ms: 0,
                    actor: ActivityActor::Agent,
                    policy_revision: 1,
                    body: ActivityBody::Control {
                        action: "test".into()
                    },
                }
            })
            .await
            .is_err()
        );
    }
}
