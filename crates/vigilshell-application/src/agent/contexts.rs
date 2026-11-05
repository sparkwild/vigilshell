use super::*;
use std::collections::BTreeMap;
impl AgentService {
    pub(super) fn context(&self, entry: &str, id: &str) -> AgentResult<Arc<Context>> {
        self.state
            .lock()
            .unwrap()
            .contexts
            .get(id)
            .filter(|c| c.view.lock().unwrap().entry_id == entry)
            .cloned()
            .ok_or("not_found")
    }
    pub(super) async fn connections(&self) -> AgentResult<AgentReply> {
        let policies = self.state.lock().unwrap().config.policies.clone();
        let mut connections = Vec::new();
        for policy in policies.into_iter().filter(|p| p.enabled()) {
            if let Ok(DbReply::Connection(c)) = self
                .database
                .execute(DbCommand::Get(policy.connection_id.clone()))
                .await
                && !c.deleting
                && c.revision == policy.connection_revision
            {
                connections.push(AgentConnectionView {
                    tabs: self
                        .connector
                        .workspace_tabs(c.id.clone(), c.revision)
                        .await?,
                    id: c.id,
                    name: c.input.name,
                    environment: c.input.environment.as_str().into(),
                    policy: policy.policy,
                });
            }
        }
        Ok(AgentReply::Connections { connections })
    }
    pub(super) fn receipt(&self, key: &str, fingerprint: &str) -> AgentResult<Option<AgentReply>> {
        let s = self.state.lock().unwrap();
        if let Some((original, reply)) = s.receipts.get(key) {
            if original != fingerprint {
                return Err("request_conflict");
            }
            return Ok(Some(reply.clone()));
        }
        if s.receipts.len() >= 4096 {
            return Err("receipt_capacity");
        }
        Ok(None)
    }
    pub(super) async fn open_context(
        self: &Arc<Self>,
        entry: AccessEntry,
        connection: &str,
        key: &str,
        tab_id: Option<String>,
    ) -> AgentResult<AgentReply> {
        validate_key(key)?;
        if let Some(id) = &tab_id {
            validate_key(id)?;
        }
        let _admit = self.admissions.lock().await;
        let receipt = format!("{}:open:{key}", entry.id);
        let fingerprint =
            serde_json::to_string(&(connection, &tab_id)).map_err(|_| "invalid_input")?;
        if let Some(reply) = self.receipt(&receipt, &fingerprint)? {
            return Ok(reply);
        }
        if self.state.lock().unwrap().recording_blocked {
            return Err("activity_recording_unavailable");
        }
        let policy = self.policy(connection);
        if !policy.enabled() {
            return Err("connection_denied");
        }
        let record = match self
            .database
            .execute(DbCommand::Get(connection.into()))
            .await
            .map_err(ws)?
        {
            DbReply::Connection(r) if !r.deleting => r,
            _ => return Err("connection_denied"),
        };
        if record.revision != policy.connection_revision {
            return Err("connection_changed");
        }
        let count = self
            .state
            .lock()
            .unwrap()
            .contexts
            .values()
            .filter(|context| !context.cancel.is_cancelled())
            .count();
        if count >= 128 {
            return Err("context_capacity");
        }
        let id = new_id();
        let view = AgentContext {
            terminal: None,
            support: None,
            id: id.clone(),
            entry_id: entry.id.clone(),
            entry_name: entry.name.clone(),
            connection_id: connection.into(),
            connection_name: record.input.name,
            connection_revision: record.revision,
            // 认证绑定本次逻辑会话；共享已连接的 SSH 传输不能继承此前会话的认证。
            phase: ContextPhase::WaitingUser,
            cwd: "/".into(),
            environment: BTreeMap::new(),
            revision: 0,
            created_at: now(),
            error: None,
        };
        let context = Arc::new(Context {
            created_terminal: tab_id.is_none(),
            target_tab: tab_id,
            workspace_tab: Mutex::new(None),
            closing: tokio::sync::Mutex::new(()),
            recorded_policies: Mutex::new(Default::default()),
            unconfirmed: std::sync::atomic::AtomicBool::new(false),
            entry_generation: entry.generation,
            view: Mutex::new(view.clone()),
            cancel: CancellationToken::new(),
        });
        if let Err(error) = self.create_recording(&view, &entry).await {
            self.stop_tasks(None).await;
            return Err(error);
        }
        let reply;
        {
            let mut s = self.state.lock().unwrap();
            if !s.config.settings.enabled
                || !s.config.entries.iter().any(|e| {
                    e.entry.id == entry.id
                        && e.entry.enabled
                        && e.entry.generation == entry.generation
                })
                || !s.config.policies.iter().any(|p| {
                    p.connection_id == connection
                        && p.enabled()
                        && p.revision == policy.revision
                        && p.connection_revision == record.revision
                })
            {
                return Err("access_revoked");
            }
            *context.view.lock().unwrap() = view.clone();
            reply = AgentReply::Context {
                context: view.clone(),
            };
            s.contexts.insert(id.clone(), context.clone());
            s.receipts.insert(receipt, (fingerprint, reply.clone()));
        }
        self.publish_context(view.clone()).await?;
        {
            if !policy.verify_connect {
                self.begin_connect(context.clone());
            } else {
                self.event(AgentEvent::Attention {
                    id: id.clone(),
                    connection_id: connection.into(),
                    code: "connection_approval_required".into(),
                })
                .await;
            }
            let service = self.clone();
            self.jobs.spawn(async move {
                tokio::select! {
                    _ = service.stop.cancelled() => return,
                    _ = context.cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(300)) => {}
                }
                let _lifecycle = context.closing.lock().await;
                let expired = {
                    let mut view = context.view.lock().unwrap();
                    if view.phase == ContextPhase::WaitingUser && !context.cancel.is_cancelled() {
                        view.phase = ContextPhase::Failed;
                        view.revision += 1;
                        view.error = Some("authorization_expired".into());
                        Some(view.clone())
                    } else {
                        None
                    }
                };
                if let Some(context) = expired {
                    let _ = service.publish_context(context).await;
                }
            });
        }
        Ok(reply)
    }
    pub(super) fn begin_connect(self: &Arc<Self>, context: Arc<Context>) {
        {
            let mut v = context.view.lock().unwrap();
            v.phase = ContextPhase::Connecting;
            v.revision += 1;
            v.error = None;
        }
        let service = self.clone();
        self.jobs.spawn(async move {
            let cv = {
                let _lifecycle = context.closing.lock().await;
                let view = context.view.lock().unwrap().clone();
                if view.phase != ContextPhase::Connecting || context.cancel.is_cancelled() {return;}
                if service.publish_context(view.clone()).await.is_err() {return;}
                view
            };
            let pool=pool_key(&cv.connection_id,cv.connection_revision);
            let gate={service.state.lock().unwrap().connection_gates.entry(pool.clone()).or_insert_with(||Arc::new(tokio::sync::Mutex::new(()))).clone()};
            let _guard=gate.lock().await;
            if context.cancel.is_cancelled() || service.stop.is_cancelled(){return;}
            let existing=service.state.lock().unwrap().connections.get(&pool).filter(|c|!c.is_closed()).cloned();
            let fresh=existing.is_none();
            let result=if let Some(c)=existing{Ok(c)}else{tokio::select!{biased;_=context.cancel.cancelled()=>Err(WorkspaceError::Cancelled),_=service.stop.cancelled()=>Err(WorkspaceError::Cancelled),r=tokio::time::timeout(Duration::from_secs(60),service.connector.connect(cv.connection_id.clone(),cv.connection_revision))=>r.unwrap_or(Err(WorkspaceError::Timeout))}};
            match result {
                Ok(c)=>{
                    let prepared=tokio::time::timeout(Duration::from_secs(30),c.probe_environment()).await.unwrap_or(Err(WorkspaceError::Timeout));
                    let _admit = service.admissions.lock().await;
                    let _lifecycle = context.closing.lock().await;
                    if context.cancel.is_cancelled() || service.stop.is_cancelled() {
                        if fresh {let _=c.close().await;}
                        return;
                    }
                    let support = match prepared {
                        Ok(support) => support,
                        _ => {
                            if fresh {let _=c.close().await;}
                            let context={
                                let mut view = context.view.lock().unwrap();
                                if view.phase==ContextPhase::Connecting && !context.cancel.is_cancelled() {
                                    view.phase=ContextPhase::Failed; view.revision+=1;
                                    view.error=Some("environment_probe_unavailable".into());
                                }
                                view.clone()
                            };
                            let _=service.publish_context(context).await;
                            return;
                        }
                    };
                    let accepted={let mut state=service.state.lock().unwrap();let mut view=context.view.lock().unwrap();
                        if context.cancel.is_cancelled() || service.stop.is_cancelled() || !state.config.settings.enabled || !state.config.entries.iter().any(|e|e.entry.id==cv.entry_id&&e.entry.enabled&&e.entry.generation==context.entry_generation) {false}
                        else {state.connections.insert(pool,c.clone());view.support=Some(support); view.revision+=1;true}
                    };
                    if !accepted && fresh {let _=c.close().await;}
                    if accepted {
                        let result = service.open_context_terminal(&context).await;
                        if let Err(error) = result {
                            let mut view = context.view.lock().unwrap();
                            view.phase = ContextPhase::Failed;
                            view.error = Some(error.into());
                            view.revision += 1;
                        }
                        let view=context.view.lock().unwrap().clone();
                        let _=service.publish_context(view).await;
                    }
                },
                Err(e)=>{
                    let _lifecycle = context.closing.lock().await;
                    let view = {
                        let mut v=context.view.lock().unwrap();
                        if v.phase != ContextPhase::Connecting || context.cancel.is_cancelled() { return; }
                        v.phase=ContextPhase::Failed; v.revision+=1;v.error=Some(e.code().into());
                        v.clone()
                    };
                    let _=service.publish_context(view).await;
                },
            }
        });
    }
    pub(super) async fn close_context(&self, id: &str) {
        let _ = self.end_context(id, false).await;
    }
    /// Closing an ended session is an idempotent observation, not a new recorded operation.
    pub(super) async fn end_context(&self, id: &str, requested: bool) -> AgentResult<()> {
        let Some(context) = self.state.lock().unwrap().contexts.get(id).cloned() else {
            return Ok(());
        };
        let _closing = context.closing.lock().await;
        let mut view = context.view.lock().unwrap().clone();
        if matches!(
            view.phase,
            ContextPhase::Closed | ContextPhase::Disconnected | ContextPhase::Failed
        ) {
            self.connector.release_workspace(id.into(), false).await?;
            if self.executor.active_for_context(id) == 0 {
                self.connector.release_workspace(id.into(), true).await?;
            }
            return Ok(());
        }
        self.connector.release_workspace(id.into(), false).await?;
        let request_record = if requested {
            self.record(
                id,
                None,
                ActivityActor::Agent,
                self.policy(&view.connection_id).revision,
                ActivityBody::Control {
                    action: "end_session".into(),
                },
            )
            .await
            .map(|_| ())
        } else {
            Ok(())
        };
        view.phase = ContextPhase::Closing;
        view.revision += 1;
        *context.view.lock().unwrap() = view.clone();
        let _ = self.publish_context(view.clone()).await;
        context.cancel.cancel();
        self.executor.cancel_scope(None, None, Some(id)).await;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let pending = self.executor.active_for_context(id) > 0;
            if !pending || Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let connection = {
            let mut state = self.state.lock().unwrap();
            let other = state.contexts.values().any(|other| {
                let value = other.view.lock().unwrap();
                value.id != id
                    && value.connection_id == view.connection_id
                    && value.connection_revision == view.connection_revision
                    && !other.cancel.is_cancelled()
            });
            if other {
                None
            } else {
                state
                    .connections
                    .remove(&pool_key(&view.connection_id, view.connection_revision))
            }
        };
        if let Some(connection) = connection {
            let _ = tokio::time::timeout(Duration::from_secs(1), connection.close()).await;
        }
        // 会话已取消后不再准入新任务；在途文件任务结算前继续锁住人工写入。
        if self.executor.active_for_context(id) == 0 {
            self.connector.release_workspace(id.into(), true).await?;
        } else if context.workspace_tab.lock().unwrap().is_some() {
            return Err("terminal_control_busy");
        }
        let unknown = context
            .unconfirmed
            .load(std::sync::atomic::Ordering::Acquire)
            || self
                .state
                .lock()
                .unwrap()
                .tasks
                .values()
                .any(|task| task.context_id == id && task.phase.active());
        view.revision += 1;
        view.phase = if unknown {
            ContextPhase::Disconnected
        } else {
            ContextPhase::Closed
        };
        view.error = unknown.then(|| "stop_unconfirmed".into());
        *context.view.lock().unwrap() = view.clone();
        let recorded = self.publish_context(view).await.is_ok();
        let _ = self
            .db(AgentDbCommand::recording(RecordingCommand::End {
                session_id: id.into(),
                ended_at: now(),
                result: if unknown {
                    "interruptedUnknown"
                } else {
                    "closed"
                }
                .into(),
                incomplete: unknown || !recorded,
            }))
            .await;
        request_record
    }
    pub(super) fn connection(
        &self,
        id: &str,
        revision: u64,
    ) -> AgentResult<Arc<dyn AgentConnection>> {
        self.state
            .lock()
            .unwrap()
            .connections
            .get(&pool_key(id, revision))
            .filter(|c| !c.is_closed())
            .cloned()
            .ok_or("connection_unavailable")
    }
}
pub(super) fn validate_key(key: &str) -> AgentResult<()> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
        Err("invalid_request_key")
    } else {
        Ok(())
    }
}

pub(super) fn pool_key(id: &str, revision: u64) -> String {
    format!("{id}:{revision}")
}
