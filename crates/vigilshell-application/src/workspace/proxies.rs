use super::*;
impl WorkspaceService {
    pub(super) async fn proxy(&self, id: &str) -> Result<ProxyRecord, WorkspaceError> {
        match self
            .database
            .execute(DbCommand::GetProxy(id.into()))
            .await?
        {
            DbReply::Proxy(p) => Ok(p),
            _ => Err(WorkspaceError::NotFound),
        }
    }
    pub(super) async fn read_credential(
        &self,
        reference: String,
        context: CredentialContext,
    ) -> Result<SecretMaterial, WorkspaceError> {
        let ns = self.namespace().await?;
        let store = self.secrets.clone();
        store.read(&ns, &reference, context).await
    }
    pub(super) async fn route(
        &self,
        proxy: Option<&ProxyRecord>,
    ) -> Result<Option<ProxyRoute>, WorkspaceError> {
        let Some(proxy) = proxy else { return Ok(None) };
        let secret = if let Some(reference) = &proxy.credential {
            Some(
                self.read_credential(reference.clone(), CredentialContext::proxy(&proxy.id))
                    .await?,
            )
        } else {
            None
        };
        Ok(Some(ProxyRoute {
            proxy: proxy.clone(),
            secret,
        }))
    }
    pub(super) async fn begin_proxy_draft(
        &self,
        owner: &str,
        id: Option<String>,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let proxy = if let Some(id) = id {
            Some(self.proxy(&id).await?)
        } else {
            None
        };
        let mut state = self.state.lock().unwrap();
        if state
            .drafts
            .values()
            .any(|d| d.owner == owner && d.proxy.is_some())
        {
            return Err(WorkspaceError::Busy);
        }
        let id = new_id();
        let mut draft = Draft {
            intent: CredentialIntent::Keep,
            owner: owner.into(),
            connection: None,
            proxy,
            secret: None,
            trust: None,
            busy: false,
        };
        // A new proxy draft still needs a target marker distinct from a connection draft.
        if draft.proxy.is_none() {
            draft.proxy = Some(ProxyRecord {
                id: String::new(),
                name: String::new(),
                kind: "socks5".into(),
                host: String::new(),
                port: 0,
                username: String::new(),
                version: 0,
                credential: None,
            });
        }
        let mut view = Self::draft_view(&id, &draft);
        if view.proxy_id.as_deref() == Some("") {
            view.proxy_id = None;
        }
        state.drafts.insert(id, draft);
        Ok(WorkspaceResult::Draft(view))
    }
    pub(super) async fn start_proxy_save(
        self: &Arc<Self>,
        owner: &str,
        mut request: ProxySave,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let needs_secret = request.draft_id.as_ref().is_some_and(|id| {
            self.state
                .lock()
                .unwrap()
                .drafts
                .get(id)
                .is_some_and(|draft| draft.secret.is_some())
        });
        if needs_secret {
            self.database.execute(DbCommand::ConfigurationReady).await?;
        }
        let _gate = self.group_gate.lock().await;
        request.proxy.validate()?;
        if request.clear_password
            && self
                .state
                .lock()
                .unwrap()
                .groups
                .values()
                .any(|g| g.proxy.as_ref().is_some_and(|p| p.id == request.proxy.id))
        {
            return Err(WorkspaceError::Busy);
        }
        let current = if request.expected_version.is_some() {
            Some(self.proxy(&request.proxy.id).await?)
        } else {
            None
        };
        if current.as_ref().map(|p| p.version) != request.expected_version {
            return Err(WorkspaceError::Conflict);
        }
        let secret = {
            let mut state = self.state.lock().unwrap();
            if state.changing.contains(&request.proxy.id) {
                return Err(WorkspaceError::Busy);
            }
            let secret = if let Some(id) = &request.draft_id {
                let d = state
                    .drafts
                    .get_mut(id)
                    .filter(|d| d.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if d.busy
                    || d.proxy.as_ref().is_none_or(|p| {
                        p.id != current.as_ref().map(|p| p.id.as_str()).unwrap_or("")
                    })
                {
                    return Err(WorkspaceError::InvalidInput);
                }
                if d.secret
                    .as_ref()
                    .is_some_and(|s| s.kind != AuthKind::Password)
                {
                    return Err(WorkspaceError::InvalidInput);
                }
                d.busy = true;
                d.secret.as_ref().map(SecretMaterial::duplicate)
            } else {
                None
            };
            state.changing.insert(request.proxy.id.clone());
            secret
        };
        let operation = match self.begin(owner, "saveProxy", false) {
            Ok(o) => o,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.changing.remove(&request.proxy.id);
                if let Some(d) = request
                    .draft_id
                    .as_ref()
                    .and_then(|id| state.drafts.get_mut(id))
                {
                    d.busy = false;
                }
                return Err(e);
            }
        };
        let service = self.clone();
        let owner = owner.to_string();
        let op = operation.id.clone();
        self.tasks.spawn(async move {
            let result = service.save_proxy(&mut request, current, secret).await;
            {
                let mut state = service.state.lock().unwrap();
                state.changing.remove(&request.proxy.id);
                if let Some(id) = &request.draft_id {
                    if result.is_ok() {
                        state.drafts.remove(id);
                    } else if let Some(d) = state.drafts.get_mut(id) {
                        d.busy = false;
                    }
                }
            }
            if let Ok(proxy) = &result {
                service.update(&op, |o| o.proxy = Some(proxy.clone())).await;
                service.changed(&owner).await;
            }
            service.finish(&op, result.map(|_| ())).await;
        });
        Ok(WorkspaceResult::Operation(Box::new(operation)))
    }
    async fn save_proxy(
        &self,
        request: &mut ProxySave,
        current: Option<ProxyRecord>,
        secret: Option<SecretMaterial>,
    ) -> Result<ProxyRecord, WorkspaceError> {
        if let Some(secret) = &secret {
            let limit = if request.proxy.kind == "socks5" {
                255
            } else {
                4096
            };
            if secret.bytes.is_empty() || secret.bytes.len() > limit {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let namespace = self.namespace().await?;
        let old = current.as_ref().and_then(|p| p.credential.clone());
        if secret.is_none()
            && !request.clear_password
            && request.proxy.kind == "socks5"
            && current.as_ref().is_some_and(|p| p.kind != "socks5")
            && let Some(reference) = &old
        {
            let saved = self
                .read_credential(
                    reference.clone(),
                    CredentialContext::proxy(&request.proxy.id),
                )
                .await?;
            if saved.bytes.is_empty() || saved.bytes.len() > 255 {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let reference = secret.as_ref().map(|_| new_id());
        request.proxy.credential = if request.clear_password {
            None
        } else {
            reference.clone().or(old.clone())
        };
        if request.clear_password && secret.is_some() {
            return Err(WorkspaceError::InvalidInput);
        }
        let changed = request.proxy.credential != old;
        let job = new_id();
        if changed {
            self.database
                .execute(DbCommand::PrepareCredential(CredentialJob {
                    id: job.clone(),
                    connection_id: request.proxy.id.clone(),
                    new_ref: request.proxy.credential.clone(),
                    old_ref: old.clone(),
                    action: "save".into(),
                }))
                .await?;
        }
        if let Some(secret) = secret {
            let store = self.secrets.clone();
            let ns = namespace.clone();
            let r = reference.clone().unwrap();
            store
                .write(
                    &ns,
                    &r,
                    &secret,
                    CredentialContext::proxy(&request.proxy.id),
                )
                .await?;
        }
        let result = self
            .database
            .execute(DbCommand::SaveProxy {
                proxy: request.proxy.clone(),
                expected_version: request.expected_version,
            })
            .await;
        let cleanup = if result.is_ok() { old } else { reference };
        if changed {
            let cleaned = if let Some(r) = cleanup {
                if self.credential_leased(&r) {
                    false
                } else {
                    let store = self.secrets.clone();
                    store.remove(&namespace, &r).await.is_ok()
                }
            } else {
                true
            };
            if cleaned {
                let _ = self
                    .database
                    .execute(DbCommand::FinishCredential(job))
                    .await;
            }
        }
        match result? {
            DbReply::Proxy(p) => Ok(p),
            _ => Err(WorkspaceError::StorageUnavailable),
        }
    }
    pub(super) async fn delete_proxy(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        version: u64,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let _gate = self.group_gate.lock().await;
        if self
            .state
            .lock()
            .unwrap()
            .groups
            .values()
            .any(|g| g.proxy.as_ref().is_some_and(|p| p.id == id))
        {
            return Err(WorkspaceError::ProxyInUse);
        }
        let proxy = self.proxy(id).await?;
        if proxy.version != version {
            return Err(WorkspaceError::Conflict);
        }
        let job = new_id();
        self.database
            .execute(DbCommand::PrepareCredential(CredentialJob {
                id: job.clone(),
                connection_id: id.into(),
                old_ref: proxy.credential.clone(),
                new_ref: None,
                action: "delete".into(),
            }))
            .await?;
        if let Err(e) = self
            .database
            .execute(DbCommand::DeleteProxy {
                id: id.into(),
                version,
            })
            .await
        {
            let _ = self
                .database
                .execute(DbCommand::FinishCredential(job))
                .await;
            return Err(e);
        }
        self.changed(owner).await;
        if let Some(r) = proxy.credential {
            let ns = self.namespace().await?;
            let store = self.secrets.clone();
            store.remove(&ns, &r).await?;
        }
        self.database
            .execute(DbCommand::FinishCredential(job))
            .await?;
        Ok(WorkspaceResult::Done)
    }
}
