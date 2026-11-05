use super::*;
use crate::ports::{
    AgentProcess, FileExecutionControl, FileTransferHandles, PreparedNativeOperation,
    PreparedScript, ScriptPreparation,
};

/// Agent 没有主机信任确认权限，也不沿用人工终端的 PTY 或自动隧道。
impl WorkspaceService {
    pub async fn connect_agent(
        self: &Arc<Self>,
        id: &str,
        revision: u64,
    ) -> Result<Arc<dyn AgentConnection>, WorkspaceError> {
        let profile = self.connection(id).await?;
        if profile.revision != revision || profile.deleting || profile.credential_pending {
            return Err(WorkspaceError::Busy);
        }
        let trust = self.trust(id).await?.ok_or(WorkspaceError::TrustRequired)?;
        if trust.host != profile.input.host || trust.port != profile.input.port {
            return Err(WorkspaceError::TrustRequired);
        }
        let owner = new_id();
        let group = self.runtime_group(&owner, profile).await?;
        let lease = Arc::new(Lease {
            inner: std::sync::OnceLock::new(),
            owner: owner.clone(),
            workspace: self.clone(),
            group: group.clone(),
            closed: AtomicBool::new(false),
        });
        let result = async {
            let secret = self.group_secret(&group).await?;
            let route = self.group_route(&group).await?;
            self.transport
                .agent(group.profile.input.clone(), trust.key, secret, route)
                .await
        }
        .await;
        match result {
            Ok(inner) => {
                let _ = lease.inner.set(inner);
                Ok(lease)
            }
            Err(error) => {
                let _ = lease.close().await;
                Err(error)
            }
        }
    }
}
struct Lease {
    owner: String,
    inner: std::sync::OnceLock<Arc<dyn AgentConnection>>,
    workspace: Arc<WorkspaceService>,
    group: Arc<groups::RuntimeGroup>,
    closed: AtomicBool,
}
impl AgentConnection for Lease {
    fn inspect_resources(
        &self,
        query: vigilshell_domain::agent::PolicyResourceQuery,
    ) -> FutureResult<'_, vigilshell_domain::agent::PolicyResourcePage> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .inspect_resources(query)
                .await
        })
    }
    fn probe_environment(
        &self,
    ) -> FutureResult<'_, vigilshell_domain::agent::ExecutionEnvironment> {
        Box::pin(async {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .probe_environment()
                .await
        })
    }
    fn probe_script(
        &self,
        path: String,
    ) -> FutureResult<'_, Option<vigilshell_domain::agent::ScriptInterpreter>> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .probe_script(path)
                .await
        })
    }
    fn execute_command(
        &self,
        command: vigilshell_domain::agent::CheckedCommand,
    ) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .execute_command(command)
                .await
        })
    }
    fn execute_terminal(
        &self,
        operation: vigilshell_domain::agent::CheckedTerminal,
    ) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .execute_terminal(operation)
                .await
        })
    }
    fn prepare_script(
        &self,
        operation: vigilshell_domain::agent::CheckedScriptOperation,
    ) -> FutureResult<'_, ScriptPreparation> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .prepare_script(operation)
                .await
        })
    }
    fn prepare_script_launch(
        self: Arc<Self>,
        operation: vigilshell_domain::agent::CheckedScriptLaunch,
    ) -> FutureResult<'static, Box<dyn PreparedScript>> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .clone()
                .prepare_script_launch(operation)
                .await
        })
    }
    fn execute_files(
        &self,
        operation: vigilshell_domain::agent::CheckedFileOperation,
        io: FileTransferHandles,
        control: FileExecutionControl,
    ) -> FutureResult<'_, vigilshell_domain::agent::FileExecutionReport> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .execute_files(operation, io, control)
                .await
        })
    }
    fn prepare_native(
        self: Arc<Self>,
        operation: vigilshell_domain::agent::CheckedNativeOperation,
    ) -> FutureResult<'static, Box<dyn PreparedNativeOperation>> {
        Box::pin(async move {
            self.inner
                .get()
                .ok_or(WorkspaceError::Disconnected)?
                .clone()
                .prepare_native(operation)
                .await
        })
    }
    fn encoding(&self) -> &str {
        self.inner.get().map_or("utf-8", |c| c.encoding())
    }
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.inner.get().is_none_or(|c| c.is_closed())
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if !self.closed.swap(true, Ordering::AcqRel) {
                let result = if let Some(inner) = self.inner.get() {
                    inner.close().await
                } else {
                    Ok(())
                };
                self.workspace.release_group(&self.owner, &self.group).await;
                result
            } else {
                Ok(())
            }
        })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let inner = self.inner.get().cloned();
            let owner = self.owner.clone();
            let workspace = self.workspace.clone();
            let group = self.group.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    if let Some(inner) = inner {
                        let _ = inner.close().await;
                    }
                    workspace.release_group(&owner, &group).await;
                });
            }
        }
    }
}
