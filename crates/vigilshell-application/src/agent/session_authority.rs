use super::*;
use std::sync::Weak;
pub(super) struct SessionAuthority(pub Weak<AgentService>);
impl ExecutionAuthority for SessionAuthority {
    fn current(&self, identity: ExecutionIdentity) -> FutureResult<'_, ExecutionAccess> {
        Box::pin(async move {
            let service = self.0.upgrade().ok_or(WorkspaceError::Cancelled)?;
            service
                .refresh_configuration()
                .await
                .map_err(|_| WorkspaceError::AgentConfigurationUnavailable)?;
            if !service.valid_entry(&identity.entry_id, identity.entry_generation) {
                return Err(WorkspaceError::Cancelled);
            }
            let profile = service
                .database
                .execute(DbCommand::Get(identity.connection_id.clone()))
                .await?;
            if !matches!(profile,DbReply::Connection(ref c) if !c.deleting&&c.revision==identity.connection_revision)
            {
                return Err(WorkspaceError::Cancelled);
            }
            let context = service
                .context(&identity.entry_id, &identity.session_id)
                .map_err(|_| WorkspaceError::NotFound)?;
            let view = context.view.lock().unwrap().clone();
            let policy = service.policy(&identity.connection_id);
            if context.cancel.is_cancelled()
                || view.phase != ContextPhase::Ready
                || view.connection_id != identity.connection_id
                || view.connection_revision != identity.connection_revision
                || policy.connection_revision != identity.connection_revision
                || policy.revision != identity.policy_revision
                || !policy.enabled()
            {
                return Err(WorkspaceError::Cancelled);
            }
            let attached = context.workspace_tab.lock().unwrap().is_some();
            if attached
                && !service
                    .connector
                    .workspace_control_live(view.id.clone())
                    .await
                    .map_err(|_| WorkspaceError::Cancelled)?
            {
                return Err(WorkspaceError::Cancelled);
            }
            let connection = service
                .connection(&view.connection_id, view.connection_revision)
                .map_err(|_| WorkspaceError::Disconnected)?;
            Ok(ExecutionAccess { policy, connection })
        })
    }
}
