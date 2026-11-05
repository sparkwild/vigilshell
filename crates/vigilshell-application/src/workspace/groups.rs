use super::*;
pub(super) struct RuntimeGroup {
    pub tools: tools::GroupTools,
    pub namespace: String,
    pub login_access: Arc<credential_access::CredentialAccess>,
    pub proxy_access: Arc<credential_access::CredentialAccess>,
    pub transfers: Arc<Semaphore>,
    pub profile: ConnectionRecord,
    pub proxy: Option<ProxyRecord>,
    pub refs: std::sync::atomic::AtomicUsize,
    pub trusted: tokio::sync::OnceCell<HostKey>,
    pub cancel: CancellationToken,
    forwarding: Mutex<Option<Arc<dyn SshForwarding>>>,
    start: tokio::sync::Mutex<()>,
    statuses: Mutex<Vec<TunnelStatus>>,
    monitoring: AtomicBool,
}
impl WorkspaceService {
    pub(super) async fn runtime_group(
        &self,
        owner: &str,
        profile: ConnectionRecord,
    ) -> Result<Arc<RuntimeGroup>, WorkspaceError> {
        let key = (owner.to_owned(), profile.id.clone());
        if let Some(group) = self.state.lock().unwrap().groups.get(&key).cloned() {
            group.refs.fetch_add(1, Ordering::AcqRel);
            return Ok(group);
        }
        let proxy = if let Some(id) = &profile.input.proxy_id {
            if self.state.lock().unwrap().changing.contains(id) {
                return Err(WorkspaceError::Busy);
            }
            Some(self.proxy(id).await?)
        } else {
            None
        };
        let group = Arc::new(RuntimeGroup {
            tools: tools::GroupTools::default(),
            namespace: self.namespace().await?,
            login_access: Arc::default(),
            proxy_access: Arc::default(),
            transfers: Arc::new(Semaphore::new(2)),
            statuses: Mutex::new(
                profile
                    .input
                    .tunnels
                    .iter()
                    .map(|r| TunnelStatus {
                        rule_id: r.id.clone(),
                        phase: if r.enabled { "stopped" } else { "disabled" }.into(),
                        error: None,
                    })
                    .collect(),
            ),
            profile,
            proxy,
            refs: std::sync::atomic::AtomicUsize::new(1),
            trusted: tokio::sync::OnceCell::new(),
            cancel: CancellationToken::new(),
            forwarding: Mutex::new(None),
            start: tokio::sync::Mutex::new(()),
            monitoring: AtomicBool::new(false),
        });
        self.state.lock().unwrap().groups.insert(key, group.clone());
        Ok(group)
    }
    pub(super) fn credential_leased(&self, reference: &str) -> bool {
        let state = self.state.lock().unwrap();
        state.groups.values().any(|g| {
            g.profile.credential.as_deref() == Some(reference)
                || g.proxy.as_ref().and_then(|p| p.credential.as_deref()) == Some(reference)
        }) || state.file_edits.values().any(|e| {
            e.group.profile.credential.as_deref() == Some(reference)
                || e.group.proxy.as_ref().and_then(|p| p.credential.as_deref()) == Some(reference)
        }) || state.file_jobs.values().any(|j| {
            j.phase == "working"
                && (j.group.profile.credential.as_deref() == Some(reference)
                    || j.group.proxy.as_ref().and_then(|p| p.credential.as_deref())
                        == Some(reference))
        }) || state.transfers.values().any(|t| {
            tools::transfer_active(&t.view.phase)
                && (t.group.profile.credential.as_deref() == Some(reference)
                    || t.group.proxy.as_ref().and_then(|p| p.credential.as_deref())
                        == Some(reference))
        })
    }
    pub(super) fn tunnel_statuses(&self, owner: &str, id: &str) -> Vec<TunnelStatus> {
        self.state
            .lock()
            .unwrap()
            .groups
            .get(&(owner.into(), id.into()))
            .map(|g| g.statuses.lock().unwrap().clone())
            .unwrap_or_default()
    }
    async fn publish_tunnels(&self, owner: &str, g: &RuntimeGroup) {
        let states = g.statuses.lock().unwrap().clone();
        let _ = self
            .sink
            .event(
                owner.into(),
                WorkspaceNotice::Tunnels {
                    connection_id: g.profile.id.clone(),
                    states,
                },
            )
            .await;
    }
    pub(super) fn begin_tunnels(self: &Arc<Self>, owner: String, g: Arc<RuntimeGroup>) {
        if !g.profile.input.tunnels.iter().any(|r| r.enabled)
            || g.monitoring.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let service = self.clone();
        self.tasks.spawn(async move{
            service.start_tunnels(&owner,&g,None).await;
            loop{tokio::select!{biased;()=g.cancel.cancelled()=>break,()=tokio::time::sleep(Duration::from_millis(500))=>{}}
                let forward=g.forwarding.lock().unwrap().clone();
                if let Some(forward)=forward{let next=forward.statuses();let changed={let mut states=g.statuses.lock().unwrap();let before=states.clone();for n in next{if let Some(s)=states.iter_mut().find(|s|s.rule_id==n.rule_id){*s=n;}}*states!=before};if changed{service.publish_tunnels(&owner,&g).await;}}
            }
        });
    }
    async fn start_tunnels(&self, owner: &str, g: &Arc<RuntimeGroup>, only: Option<&str>) {
        let _start = g.start.lock().await;
        if g.cancel.is_cancelled() {
            return;
        }
        let work = async {
            let mut existing = g.forwarding.lock().unwrap().clone();
            if existing.as_ref().is_some_and(|f| f.is_closed()) {
                if let Some(f) = existing.take() {
                    let _ = f.close().await;
                }
                *g.forwarding.lock().unwrap() = None;
            }
            let forward = if let Some(f) = existing {
                f
            } else {
                let _permit = self
                    .network
                    .acquire()
                    .await
                    .map_err(|_| WorkspaceError::Busy)?;
                let secret = self.group_secret(g).await?;
                let route = self.group_route(g).await?;
                let key = g
                    .trusted
                    .get()
                    .cloned()
                    .ok_or(WorkspaceError::TrustRequired)?;
                let f = self
                    .transport
                    .forwarding(g.profile.input.clone(), key, secret, route)
                    .await?;
                *g.forwarding.lock().unwrap() = Some(f.clone());
                f
            };
            for rule in g
                .profile
                .input
                .tunnels
                .iter()
                .filter(|r| r.enabled && only.is_none_or(|id| r.id == id))
            {
                let state = forward
                    .start(rule.clone())
                    .await
                    .unwrap_or_else(|e| TunnelStatus {
                        rule_id: rule.id.clone(),
                        phase: "failed".into(),
                        error: Some(e.code().into()),
                    });
                if let Some(s) = g
                    .statuses
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .find(|s| s.rule_id == rule.id)
                {
                    *s = state;
                }
            }
            Ok::<_, WorkspaceError>(())
        };
        let result = tokio::select! {biased;()=g.cancel.cancelled()=>return,r=work=>r};
        if let Err(e) = result {
            for s in g
                .statuses
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|s| s.phase != "disabled" && only.is_none_or(|id| s.rule_id == id))
            {
                s.phase = "failed".into();
                s.error = Some(e.code().into());
            }
        }
        self.publish_tunnels(owner, g).await;
    }
    pub(super) async fn retry_tunnel(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        rule: &str,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let g = self
            .state
            .lock()
            .unwrap()
            .groups
            .get(&(owner.into(), id.into()))
            .cloned()
            .ok_or(WorkspaceError::NotFound)?;
        if !g
            .profile
            .input
            .tunnels
            .iter()
            .any(|r| r.id == rule && r.enabled)
        {
            return Err(WorkspaceError::NotFound);
        }
        g.retry_credentials();
        self.start_tunnels(owner, &g, Some(rule)).await;
        Ok(WorkspaceResult::Tunnels(self.tunnel_statuses(owner, id)))
    }
    pub(super) async fn release_group(&self, owner: &str, g: &Arc<RuntimeGroup>) {
        let _gate = self.group_gate.lock().await;
        if g.refs.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        {
            let mut state = self.state.lock().unwrap();
            let key = (owner.into(), g.profile.id.clone());
            if state.groups.get(&key).is_some_and(|v| Arc::ptr_eq(v, g)) {
                state.groups.remove(&key);
            }
        }
        g.cancel.cancel();
        self.stop_group_tools(g).await;
        let f = g.forwarding.lock().unwrap().take();
        if let Some(f) = f {
            let _ = tokio::time::timeout(Duration::from_secs(1), f.close()).await;
        }
        for s in g
            .statuses
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|s| s.phase != "disabled")
        {
            s.phase = "stopped".into();
            s.error = None;
        }
        self.publish_tunnels(owner, g).await;
        let _ = self.recover_credentials().await;
    }
}
