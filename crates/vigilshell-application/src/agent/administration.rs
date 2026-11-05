use super::*;
impl AgentService {
    fn check_access_deletion(&self, state: &State, id: &str, generation: u64) -> AgentResult<bool> {
        if id.is_empty() || id.len() > 128 || generation == 0 {
            return Err("invalid_access_entry");
        }
        let Some(entry) = state.config.entries.iter().find(|e| e.entry.id == id) else {
            return Ok(false);
        };
        if entry.entry.enabled {
            return Err("access_not_revoked");
        }
        if entry.entry.generation != generation {
            return Err("configuration_conflict");
        }
        if self.executor.active_for_entry(id) > 0
            || state
                .tasks
                .values()
                .any(|t| t.entry_id == id && t.phase.active())
            || state.contexts.values().any(|context| {
                let view = context.view.lock().unwrap();
                view.entry_id == id
                    && !matches!(
                        view.phase,
                        ContextPhase::Closed | ContextPhase::Failed | ContextPhase::Disconnected
                    )
            })
        {
            return Err("access_busy");
        }
        Ok(true)
    }
    pub(super) async fn prepare(&self, change: AgentMutation) -> AgentResult<AgentReply> {
        self.refresh_configuration().await?;
        let script = if let AgentMutation::ApproveScript { task_id } = &change {
            let view = self.get_task(task_id).await?;
            let task = self.executor.get(&view.entry_id, task_id).await?;
            if task.state.phase != ExecutionPhase::WaitingReview {
                return Err("review_not_waiting");
            }
            if let ScriptReviewState::WaitingReview { binding } = &task.state.review {
                Some(binding.clone())
            } else {
                return Err("review_not_waiting");
            }
        } else {
            None
        };
        self.prepare_policy_change(&change).await?;
        let mut s = self.state.lock().unwrap();
        match &change {
            AgentMutation::ApproveScript { .. } => {}
            AgentMutation::Bootstrap { settings, secret } => {
                if !s.config.entries.is_empty()
                    || settings.revision != s.config.settings.revision
                    || !settings.enabled
                    || settings.port == 0
                    || !(1..=365).contains(&settings.retention_days)
                    || !(16..=1024).contains(&settings.storage_mib)
                    || secret.entry.generation != 1
                    || !secret.entry.enabled
                    || secret.entry.id.is_empty()
                    || secret.entry.name.trim().is_empty()
                    || secret.digest.len() != 64
                    || !secret.digest.bytes().all(|b| b.is_ascii_hexdigit())
                    || secret.sealed.is_empty()
                    || secret.sealed.len() > 4096
                {
                    return Err("invalid_settings");
                }
            }
            AgentMutation::SettingsPatch { patch } => {
                patch.apply(&s.config.settings)?;
            }
            AgentMutation::Export {
                id,
                generation,
                port,
                client,
                delivery: _,
            } => {
                let entry = s
                    .config
                    .entries
                    .iter()
                    .find(|e| {
                        e.entry.id == *id && e.entry.enabled && e.entry.generation == *generation
                    })
                    .ok_or("access_revoked")?;
                if s.config.settings.port != *port
                    || *port == 0
                    || entry.entry.client.as_ref().is_some_and(|c| c != client)
                {
                    return Err("invalid_delivery");
                }
            }
            AgentMutation::Settings { settings } => {
                if settings.revision != s.config.settings.revision
                    || settings.retention_days == 0
                    || settings.retention_days > 365
                    || !(16..=1024).contains(&settings.storage_mib)
                {
                    return Err("invalid_settings");
                }
            }
            AgentMutation::UpdatePolicySettings { .. } | AgentMutation::SetPolicyAccess { .. } => {}
            AgentMutation::Access { secret } | AgentMutation::Provision { secret, .. } => {
                if secret.entry.id.is_empty()
                    || secret.entry.id.len() > 128
                    || secret.entry.name.trim().is_empty()
                    || secret.entry.name.len() > 128
                    || secret.digest.len() != 64
                    || !secret.digest.bytes().all(|b| b.is_ascii_hexdigit())
                    || secret.sealed.len() > 4096
                    || secret.sealed.is_empty()
                {
                    return Err("invalid_access_entry");
                }
                let old = s
                    .config
                    .entries
                    .iter()
                    .find(|e| e.entry.id == secret.entry.id);
                if old.is_none() && s.config.entries.len() >= 32
                    || old.is_none() && secret.entry.generation != 1
                    || old.is_some_and(|e| {
                        secret.entry.generation < e.entry.generation
                            || secret.entry.generation > e.entry.generation + 1
                            || secret.digest != e.digest
                                && secret.entry.generation == e.entry.generation
                    })
                {
                    return Err("invalid_access_entry");
                }
            }
            AgentMutation::Revoke { id } => {
                if !s.config.entries.iter().any(|e| e.entry.id == *id) {
                    return Err("not_found");
                }
            }
            AgentMutation::DeleteAccess { id, generation } => {
                self.check_access_deletion(&s, id, *generation)?;
            }
            AgentMutation::ApproveConnection { context_id } => {
                let c = s.contexts.get(context_id).ok_or("not_found")?;
                let v = c.view.lock().unwrap();
                if v.phase != ContextPhase::WaitingUser
                    || now().saturating_sub(v.created_at) > 300_000
                {
                    return Err("authorization_expired");
                }
            }
        }
        if let AgentMutation::Provision { port, secret, .. } = &change
            && (*port == 0 || secret.entry.client.is_none())
        {
            return Err("invalid_delivery");
        }
        s.challenges.retain(|_, c| c.expires > Instant::now());
        if s.challenges.len() >= 16 {
            return Err("busy");
        }
        let id = new_id();
        let revision = s.config.settings.revision;
        s.challenges.insert(
            id.clone(),
            Challenge {
                change,
                revision,
                expires: Instant::now() + Duration::from_secs(300),
                script,
            },
        );
        Ok(AgentReply::Challenge { id, revision })
    }
    pub(super) async fn commit(self: &Arc<Self>, id: &str) -> AgentResult<AgentReply> {
        let _guard = self.administration.lock().await;
        self.refresh_configuration().await?;
        let (challenge, mut config) = {
            let mut s = self.state.lock().unwrap();
            (
                s.challenges.remove(id).ok_or("authorization_expired")?,
                s.config.clone(),
            )
        };
        if challenge.expires < Instant::now() || self.stop.is_cancelled() {
            return Err("authorization_expired");
        }
        if let AgentMutation::DeleteAccess { id, generation } = &challenge.change
            && !self.check_access_deletion(&self.state.lock().unwrap(), id, *generation)?
        {
            return Ok(AgentReply::Done {});
        }
        if challenge.revision != config.settings.revision {
            return Err("authorization_expired");
        }
        self.prepare_policy_change(&challenge.change).await?;
        if let AgentMutation::ApproveScript { task_id } = &challenge.change {
            let binding = challenge.script.ok_or("authorization_expired")?;
            let result = self
                .executor
                .approve_script(&binding.identity.entry_id.clone(), task_id, binding)
                .await;
            if let Err(code) = &result
                && matches!(
                    *code,
                    "activity_recording_unavailable" | "agent_storage_unavailable"
                )
            {
                self.recording_failed().await;
            }
            return result.map(|task| AgentReply::Task {
                task: self.project_task(&task),
            });
        }
        let previous_port = config.settings.port;
        let previous_endpoint = config.settings.endpoint_revision;
        let mut revoked = None;
        match challenge.change {
            AgentMutation::ApproveScript { .. } => return Err("authorization_expired"),
            AgentMutation::Bootstrap { settings, secret } => {
                config.settings = settings;
                config.entries.push(secret);
            }
            AgentMutation::Settings { settings } => config.settings = settings,
            AgentMutation::SettingsPatch { patch } => {
                config.settings = patch.apply(&config.settings)?
            }
            AgentMutation::Export { .. } => return Ok(AgentReply::Done {}),
            AgentMutation::Provision { secret, port, .. } => {
                if config.entries.iter().any(|e| e.entry.id == secret.entry.id) {
                    revoked = Some(secret.entry.id.clone());
                }
                config.entries.retain(|e| e.entry.id != secret.entry.id);
                config.entries.push(secret);
                config.settings.enabled = true;
                config.settings.port = port;
            }
            change @ (AgentMutation::UpdatePolicySettings { .. }
            | AgentMutation::SetPolicyAccess { .. }) => {
                let mut next = self
                    .prepare_policy_change(&change)
                    .await?
                    .ok_or("invalid_policy")?;
                let old = config
                    .policies
                    .iter()
                    .find(|p| p.connection_id == next.connection_id);
                if old == Some(&next) {
                    return Ok(AgentReply::Done {});
                }
                next.policy.revision += 1;
                config
                    .policies
                    .retain(|p| p.connection_id != next.connection_id);
                config.policies.push(next);
            }
            AgentMutation::Access { secret } => {
                if config.entries.iter().any(|e| {
                    e.entry.id == secret.entry.id
                        && (e.entry.generation != secret.entry.generation || !secret.entry.enabled)
                }) {
                    revoked = Some(secret.entry.id.clone());
                }
                config.entries.retain(|e| e.entry.id != secret.entry.id);
                config.entries.push(secret);
            }
            AgentMutation::Revoke { id } => {
                config.entries.retain(|entry| entry.entry.id != id);
                revoked = Some(id);
            }
            AgentMutation::DeleteAccess { id, .. } => {
                config.entries.retain(|entry| entry.entry.id != id);
            }
            AgentMutation::ApproveConnection { context_id } => {
                if !config.settings.enabled {
                    return Err("service_disabled");
                }
                let c = self
                    .state
                    .lock()
                    .unwrap()
                    .contexts
                    .get(&context_id)
                    .cloned()
                    .ok_or("not_found")?;
                let view = c.view.lock().unwrap().clone();
                if view.phase != ContextPhase::WaitingUser
                    || now().saturating_sub(view.created_at) > 300_000
                    || !config
                        .entries
                        .iter()
                        .any(|e| e.entry.id == view.entry_id && e.entry.enabled)
                    || !self.policy(&view.connection_id).enabled()
                {
                    return Err("authorization_expired");
                }
                self.begin_connect(c);
                return Ok(AgentReply::Done {});
            }
        }
        config.settings.endpoint_revision =
            previous_endpoint + u64::from(config.settings.port != previous_port);
        config.settings.revision = challenge.revision + 1;
        self.db(AgentDbCommand::SaveConfiguration {
            configuration: config.clone(),
            expected: challenge.revision,
        })
        .await?;
        // Policy changes gate future execution; running work retains its original snapshot.
        self.state.lock().unwrap().config = config.clone();
        self.executor
            .set_recording_limit(u64::from(config.settings.storage_mib) * 1024 * 1024)?;
        if !config.settings.enabled {
            self.stop_tasks(None).await;
        } else if let Some(entry) = revoked {
            self.stop_tasks(Some(&entry)).await;
        }
        self.event(AgentEvent::Configuration {}).await;
        Ok(AgentReply::Configuration {
            protections: system_protections(),
            recording_error: self
                .state
                .lock()
                .unwrap()
                .recording_blocked
                .then(|| "activity_recording_unavailable".into()),
            configuration: config,
        })
    }
}
