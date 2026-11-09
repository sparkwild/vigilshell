use super::*;
impl AgentService {
    /// Preparation and commit both check access, policy and connection versions.
    pub(super) async fn prepare_policy_change(
        &self,
        change: &AgentMutation,
    ) -> AgentResult<Option<ConnectionPolicySettings>> {
        let (id, expected, revision) = match change {
            AgentMutation::UpdatePolicySettings {
                connection_id,
                expected_revision,
                connection_revision,
                ..
            }
            | AgentMutation::SetPolicyAccess {
                connection_id,
                expected_revision,
                connection_revision,
                ..
            } => (connection_id, *expected_revision, *connection_revision),
            _ => return Ok(None),
        };
        let record = self
            .database
            .execute(DbCommand::Get(id.clone()))
            .await
            .map_err(ws)?;
        if !matches!(record, DbReply::Connection(ref c) if c.revision == revision && !c.deleting) {
            return Err("connection_changed");
        }
        let mut next = self
            .state
            .lock()
            .unwrap()
            .config
            .policies
            .iter()
            .find(|p| p.connection_id == *id)
            .cloned()
            .unwrap_or_else(|| ConnectionPolicySettings::new(id.clone(), revision));
        if next.revision != expected {
            return Err("configuration_conflict");
        }
        match change {
            AgentMutation::UpdatePolicySettings { patch, .. } => {
                if next.access_enabled {
                    return Err("policy_access_enabled");
                }
                next.policy = patch.apply(&next.policy)?;
                next.policy.connection_revision = revision;
            }
            AgentMutation::SetPolicyAccess { enabled, .. } => {
                next.access_enabled = *enabled;
                if *enabled {
                    next.policy.connection_revision = revision;
                }
            }
            _ => unreachable!(),
        }
        next.validate()?;
        Ok(Some(next))
    }
}
