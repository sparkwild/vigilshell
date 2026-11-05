use super::*;
impl WorkspaceService {
    pub(in crate::workspace) async fn monitor_action(
        self: &Arc<Self>,
        owner: &str,
        action: ToolAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let reply =
            match action {
                ToolAction::Subscribe {
                    group: group_id,
                    visible,
                } => {
                    let group = self
                        .state
                        .lock()
                        .unwrap()
                        .groups
                        .iter()
                        .find(|((candidate_owner, _), group)| {
                            candidate_owner == owner && group.tools.id == group_id
                        })
                        .map(|(_, group)| group.clone());
                    let Some(group) = group else {
                        return if visible {
                            Err(WorkspaceError::Disconnected)
                        } else {
                            Ok(WorkspaceResult::Tools(ToolResult::Done))
                        };
                    };
                    let changed = {
                        let _sample = group.tools.sample.lock().unwrap();
                        let changed = visible != group.tools.visible.load(Ordering::Acquire);
                        if changed {
                            group
                                .tools
                                .monitor_generation
                                .fetch_add(1, Ordering::AcqRel);
                            *group.tools.previous_cpu.lock().unwrap() = None;
                            *group.tools.previous_network.lock().unwrap() = Default::default();
                        }
                        group.tools.visible.store(visible, Ordering::Release);
                        changed
                    };
                    if changed && group.tools.monitoring.load(Ordering::Acquire) {
                        group.tools.wake.notify_one();
                    }
                    if visible {
                        self.begin_monitor(owner.into(), group.clone());
                    }
                    let sample = group.tools.sample.lock().unwrap().clone();
                    ToolResult::Snapshot(Box::new(ToolSnapshot {
                        connection_id: group.profile.id.clone(),
                        group_id: group.tools.id.clone(),
                        sample,
                    }))
                }
                ToolAction::Refresh { session } => {
                    let group = self.tool_group(owner, &session)?;
                    group.retry_credentials();
                    let failed = group.tools.sample.lock().unwrap().error.is_some();
                    let transferring =
                        self.state.lock().unwrap().transfers.values().any(|t| {
                            Arc::ptr_eq(&t.group, &group) && transfer_active(&t.view.phase)
                        });
                    if failed
                        && !transferring
                        && let Some(client) = group.tools.client.lock().await.take()
                    {
                        let _ = client.close().await;
                    }
                    let snapshot = self.sample_tools(owner, &group, true).await?;
                    if snapshot.sample.error.is_none() {
                        group.tools.suspended.store(false, Ordering::Release);
                        group.tools.wake.notify_one();
                    }
                    if group.tools.visible.load(Ordering::Acquire) {
                        self.begin_monitor(owner.into(), group);
                    }
                    ToolResult::Snapshot(Box::new(snapshot))
                }
                _ => return Err(WorkspaceError::InvalidInput),
            };
        Ok(WorkspaceResult::Tools(reply))
    }
    pub(in crate::workspace) async fn monitor_preferences(
        &self,
    ) -> Result<vigilshell_domain::Preferences, WorkspaceError> {
        match self.database.execute(DbCommand::Preferences).await? {
            DbReply::Preferences(p) => Ok(p),
            _ => Err(WorkspaceError::StorageUnavailable),
        }
    }
    pub(in crate::workspace) async fn sample_tools(
        &self,
        owner: &str,
        group: &Arc<RuntimeGroup>,
        force_full: bool,
    ) -> Result<ToolSnapshot, WorkspaceError> {
        let _sampling = group.tools.sampling.lock().await;
        if group.refs.load(Ordering::Acquire) == 0 {
            return Err(WorkspaceError::Disconnected);
        }
        let generation = group.tools.monitor_generation.load(Ordering::Acquire);
        let _slot = loop {
            if !force_full && !group.tools.visible.load(Ordering::Acquire) {
                return Err(WorkspaceError::Cancelled);
            }
            tokio::select! {
                _ = group.cancel.cancelled() => return Err(WorkspaceError::Cancelled),
                _ = group.tools.wake.notified(), if !force_full => continue,
                permit = self.monitor_slots.acquire() => break permit.map_err(|_| WorkspaceError::Cancelled)?,
            }
        };
        if !force_full
            && (!group.tools.visible.load(Ordering::Acquire)
                || group.tools.monitor_generation.load(Ordering::Acquire) != generation)
        {
            return Err(WorkspaceError::Cancelled);
        }
        let preferences = self.monitor_preferences().await?;
        let full = force_full
            || group.tools.last_full.lock().unwrap().is_none_or(|time| {
                time.elapsed() >= Duration::from_secs(u64::from(preferences.monitor_full_seconds))
            });
        let result = match self.tools_client(group).await {
            Ok(client) => client.sample(full).await,
            Err(e) => Err(e),
        };
        let snapshot = {
            let mut sample = group.tools.sample.lock().unwrap();
            if !force_full && group.tools.monitor_generation.load(Ordering::Acquire) != generation {
                return Err(WorkspaceError::Cancelled);
            }
            match result {
                Ok(raw) => {
                    let (receive, send) = group
                        .tools
                        .previous_network
                        .lock()
                        .unwrap()
                        .sample(raw.network_time, raw.network);
                    sample.network_receive = receive;
                    sample.network_send = send;
                    let mut previous = group.tools.previous_cpu.lock().unwrap();
                    sample.cpu_percent = match (previous.as_ref(), raw.cpu_total, raw.cpu_idle) {
                        (Some((old_total, old_idle)), Some(total), Some(idle))
                            if total > *old_total
                                && idle >= *old_idle
                                && idle - old_idle <= total - old_total =>
                        {
                            Some(
                                100.0
                                    * (1.0 - (idle - old_idle) as f64 / (total - old_total) as f64),
                            )
                        }
                        _ => None,
                    };
                    *previous = raw.cpu_total.zip(raw.cpu_idle);
                    sample.memory_total = raw.view.memory_total;
                    sample.memory_available = raw.view.memory_available;
                    sample.uptime = raw.view.uptime;
                    if full {
                        sample.hostname = raw.view.hostname;
                        sample.distribution = raw.view.distribution;
                        sample.kernel = raw.view.kernel;
                        sample.architecture = raw.view.architecture;
                        sample.disks = raw.view.disks;
                        *group.tools.last_full.lock().unwrap() = Some(Instant::now());
                    }
                    sample.collected_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    sample.phase = "ready".into();
                    sample.error = None;
                }
                Err(e) => {
                    *group.tools.previous_cpu.lock().unwrap() = None;
                    *group.tools.previous_network.lock().unwrap() = Default::default();
                    sample.network_receive = None;
                    sample.network_send = None;
                    sample.phase = if sample.collected_at == 0 {
                        "unavailable"
                    } else {
                        "stale"
                    }
                    .into();
                    sample.error = Some(e.code().into());
                }
            }
            ToolSnapshot {
                connection_id: group.profile.id.clone(),
                group_id: group.tools.id.clone(),
                sample: sample.clone(),
            }
        };
        let _ = self
            .sink
            .event(
                owner.into(),
                WorkspaceNotice::Tools(Box::new(snapshot.clone())),
            )
            .await;
        Ok(snapshot)
    }
    pub(in crate::workspace) fn begin_monitor(
        self: &Arc<Self>,
        owner: String,
        group: Arc<RuntimeGroup>,
    ) {
        if group.tools.monitoring.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        self.tasks.spawn(async move {
            let mut failures = 0;
            loop {
                if group.cancel.is_cancelled() { break; }
                if !group.tools.visible.load(Ordering::Acquire) || group.tools.suspended.load(Ordering::Acquire) {
                    tokio::select! { _=group.cancel.cancelled()=>break, _=group.tools.wake.notified()=>{} }
                    continue;
                }
                let round_started = tokio::time::Instant::now();
                let preferences=match service.monitor_preferences().await { Ok(p)=>p, Err(_)=>break };
                let needs_cpu_baseline = group.tools.previous_cpu.lock().unwrap().is_none();
                let result = tokio::select! { _ = group.cancel.cancelled() => break, result = service.sample_tools(&owner, &group, false) => result };
                if matches!(result, Err(WorkspaceError::Cancelled)) { continue; }
                // CPU 首次有效值需要两次采样，仅基线建立后提前补采一次。
                let warming_up = needs_cpu_baseline
                    && result.as_ref().is_ok_and(|s| s.sample.error.is_none() && s.sample.cpu_percent.is_none())
                    && group.tools.previous_cpu.lock().unwrap().is_some();
                let delay = if warming_up { Duration::from_secs(1) } else { Duration::from_secs(u64::from(preferences.monitor_sample_seconds)) };
                let delay = next_sample_delay(round_started.elapsed(), delay);
                if result.as_ref().is_ok_and(|s| s.sample.error.is_none()) { failures = 0; } else { failures += 1; }
                let credential_blocked = result.as_ref().is_ok_and(|s| matches!(s.sample.error.as_deref(), Some("credential_missing" | "credential_denied" | "credential_unavailable" | "vault_key_missing" | "vault_key_invalid" | "credential_corrupt" | "credential_format_unsupported")));
                if credential_blocked || failures >= 3 { group.tools.suspended.store(true, Ordering::Release); }
                tokio::select! { _ = group.cancel.cancelled() => break, _ = group.tools.wake.notified() => {}, _ = tokio::time::sleep(delay) => {} }
            }
            group.tools.monitoring.store(false, Ordering::Release);
        });
    }
}

fn next_sample_delay(elapsed: Duration, period: Duration) -> Duration {
    if elapsed <= period {
        return period - elapsed;
    }
    let remainder = Duration::from_nanos((elapsed.as_nanos() % period.as_nanos()) as u64);
    if remainder.is_zero() {
        Duration::ZERO
    } else {
        period - remainder
    }
}
#[cfg(test)]
mod cadence_tests {
    use super::*;
    #[test]
    fn subtracts_work_time_and_skips_missed_ticks_without_a_burst() {
        let period = Duration::from_secs(1);
        assert_eq!(
            next_sample_delay(Duration::from_millis(200), period),
            Duration::from_millis(800)
        );
        assert_eq!(
            next_sample_delay(Duration::from_millis(1200), period),
            Duration::from_millis(800)
        );
        assert_eq!(
            next_sample_delay(Duration::from_millis(5200), period),
            Duration::from_millis(800)
        );
        assert_eq!(next_sample_delay(period, period), Duration::ZERO);
    }
}
