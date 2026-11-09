use super::*;
impl WorkspaceService {
    pub(super) async fn start_save(
        self: &Arc<Self>,
        owner: &str,
        mut request: SaveRequest,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let _group_guard = self.group_gate.lock().await;
        request.input = request.input.normalized()?;
        if request.connection_id.is_some() != request.expected_version.is_some() {
            return Err(WorkspaceError::InvalidInput);
        }
        let current = if let Some(id) = &request.connection_id {
            let p = self.connection(id).await?;
            if p.deleting {
                return Err(WorkspaceError::Busy);
            }
            if Some(p.version) != request.expected_version {
                return Err(WorkspaceError::Conflict);
            }
            Some(p)
        } else {
            None
        };
        let connection_id = request.connection_id.clone().unwrap_or_else(new_id);
        let (secret, trust, intent) = {
            let mut state = self.state.lock().unwrap();
            if state.changing.contains(&connection_id) {
                return Err(WorkspaceError::Busy);
            }
            let result = if let Some(id) = &request.draft_id {
                let d = state
                    .drafts
                    .get_mut(id)
                    .filter(|d| d.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if d.proxy.is_some() {
                    return Err(WorkspaceError::InvalidInput);
                }
                if d.busy || d.connection.as_ref().map(|p| &p.id) != request.connection_id.as_ref()
                {
                    return Err(WorkspaceError::Busy);
                }
                if d.secret
                    .as_ref()
                    .is_some_and(|s| s.kind != request.input.auth)
                {
                    return Err(WorkspaceError::CredentialMissing);
                }
                if current
                    .as_ref()
                    .is_some_and(|p| p.input.auth != request.input.auth)
                    && d.secret.is_none()
                    && d.intent != CredentialIntent::Remove
                {
                    return Err(WorkspaceError::CredentialMissing);
                }
                d.busy = true;
                (
                    d.secret.as_ref().map(SecretMaterial::duplicate),
                    d.trust
                        .as_ref()
                        .filter(|(input, _)| input.same_endpoint(&request.input))
                        .map(|(_, key)| key.clone()),
                    d.intent,
                )
            } else {
                (None, None, CredentialIntent::Keep)
            };
            if current
                .as_ref()
                .is_some_and(|p| p.input.auth != request.input.auth)
                && result.0.is_none()
                && result.2 != CredentialIntent::Remove
            {
                return Err(WorkspaceError::CredentialMissing);
            }
            state.changing.insert(connection_id.clone());
            result
        };
        let operation = match self.begin(owner, "save", false) {
            Ok(o) => o,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.changing.remove(&connection_id);
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
        let owner = owner.to_owned();
        let id = operation.id.clone();
        self.tasks.spawn(async move {
            let result = service
                .save(
                    &connection_id,
                    current,
                    request.clone(),
                    secret,
                    trust,
                    intent,
                )
                .await;
            {
                let mut state = service.state.lock().unwrap();
                state.changing.remove(&connection_id);
                if let Some(did) = &request.draft_id {
                    if result.is_ok() {
                        state.drafts.remove(did);
                    } else if let Some(d) = state.drafts.get_mut(did) {
                        d.busy = false;
                    }
                }
            }
            if let Ok(p) = &result {
                service
                    .update(&id, |o| o.connection = Some(p.clone()))
                    .await;
                service.changed(&owner).await;
            }
            service.finish(&id, result.map(|_| ())).await;
        });
        Ok(WorkspaceResult::Operation(Box::new(operation)))
    }
    async fn save(
        &self,
        id: &str,
        current: Option<ConnectionRecord>,
        request: SaveRequest,
        secret: Option<SecretMaterial>,
        trust: Option<HostKey>,
        intent: CredentialIntent,
    ) -> Result<ConnectionRecord, WorkspaceError> {
        let namespace = self.namespace().await?;
        let reference = secret.as_ref().map(|_| new_id());
        let job_id = new_id();
        let changed = reference.is_some()
            || (intent == CredentialIntent::Remove
                && current.as_ref().is_some_and(|p| p.credential.is_some()));
        if changed {
            self.database
                .execute(DbCommand::PrepareCredential(CredentialJob {
                    id: job_id.clone(),
                    connection_id: id.into(),
                    new_ref: reference.clone(),
                    old_ref: current.as_ref().and_then(|p| p.credential.clone()),
                    action: "save".into(),
                }))
                .await?;
        }
        if let Some(reference) = &reference {
            let store = self.secrets.clone();
            let namespace = namespace.clone();
            let reference = reference.clone();
            let secret = secret.as_ref().unwrap().duplicate();
            store
                .write(
                    &namespace,
                    &reference,
                    &secret,
                    CredentialContext::login(id, secret.kind),
                )
                .await?;
        }
        let diagnostic = secret.as_ref().and_then(|s| s.diagnostic.clone());
        let expected_input = diagnostic.as_ref().map(|_| request.input.clone());
        let expected_reference = if intent == CredentialIntent::Remove {
            None
        } else {
            reference
                .clone()
                .or_else(|| current.as_ref().and_then(|p| p.credential.clone()))
        };
        let saved = self
            .database
            .execute(DbCommand::Save {
                id: id.into(),
                expected_version: request.expected_version,
                group_id: request.group_id,
                input: request.input,
                credential: if intent == CredentialIntent::Remove {
                    CredentialChange::Remove
                } else if let Some(reference) = &reference {
                    CredentialChange::Replace {
                        reference: reference.clone(),
                        label: secret.as_ref().unwrap().label.clone(),
                    }
                } else {
                    CredentialChange::Keep
                },
                trust,
            })
            .await;
        let cleanup = if saved.is_ok() {
            current.and_then(|p| p.credential)
        } else {
            reference.clone()
        };
        if changed {
            let cleaned = if let Some(reference) = cleanup {
                let store = self.secrets.clone();
                let namespace = namespace.clone();
                if self.credential_leased(&reference) {
                    Err(WorkspaceError::Busy)
                } else {
                    store.remove(&namespace, &reference).await
                }
            } else {
                Ok(())
            };
            if cleaned.is_ok() {
                let _ = self
                    .database
                    .execute(DbCommand::FinishCredential(job_id))
                    .await;
            }
        }
        match saved? {
            DbReply::Connection(mut p) => {
                p.credential_pending = changed;
                let readback = self.connection(id).await;
                if let Some(trace) = diagnostic {
                    match &readback {
                        Ok(stored) => {
                            trace.record(
                                "connection_config_readback",
                                Some(expected_input.as_ref() == Some(&stored.input)),
                            );
                            trace.record(
                                "connection_credential_reference",
                                Some(stored.credential == expected_reference),
                            );
                        }
                        Err(_) => trace.record("connection_config_readback", None),
                    }
                }
                Ok(readback.unwrap_or(p))
            }
            _ => Err(WorkspaceError::StorageUnavailable),
        }
    }
    pub(super) async fn start_delete(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        version: u64,
        disconnect: bool,
        approved: Vec<String>,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let _group_guard = self.group_gate.lock().await;
        let p = self.connection(id).await?;
        if p.version != version {
            return Err(WorkspaceError::Conflict);
        }
        let sessions = {
            let mut state = self.state.lock().unwrap();
            if state.changing.contains(id) {
                return Err(WorkspaceError::Busy);
            }
            let ids = state
                .sessions
                .values()
                .filter(|s| {
                    s.view.connection.id == id
                        && !matches!(s.view.phase.as_str(), "closed" | "failed" | "cancelled")
                })
                .map(|s| (s.owner.clone(), s.view.id.clone()))
                .collect::<Vec<_>>();
            if !disconnect && !ids.is_empty() {
                return Err(WorkspaceError::Busy);
            }
            if ids.iter().any(|(_, id)| !approved.contains(id)) {
                return Err(WorkspaceError::Conflict);
            }
            state.changing.insert(id.into());
            ids
        };
        let operation = match self.begin(owner, "delete", false) {
            Ok(o) => o,
            Err(e) => {
                self.state.lock().unwrap().changing.remove(id);
                return Err(e);
            }
        };
        let service = self.clone();
        let owner = owner.to_owned();
        let id = id.to_owned();
        let operation_id = operation.id.clone();
        self.tasks.spawn(async move {
            let result = service.delete(&p, sessions).await;
            service.state.lock().unwrap().changing.remove(&id);
            service.changed(&owner).await;
            service.finish(&operation_id, result).await;
        });
        Ok(WorkspaceResult::Operation(Box::new(operation)))
    }
    async fn delete(
        &self,
        p: &ConnectionRecord,
        sessions: Vec<(String, String)>,
    ) -> Result<(), WorkspaceError> {
        for (owner, id) in sessions {
            self.close_session(&owner, &id).await?;
        }
        let namespace = self.namespace().await?;
        let job = new_id();
        self.database
            .execute(DbCommand::BeginDeletion {
                id: p.id.clone(),
                version: p.version,
                job: job.clone(),
            })
            .await?;
        if let Some(reference) = p.credential.clone() {
            if self.credential_leased(&reference) {
                return Err(WorkspaceError::Busy);
            }
            let store = self.secrets.clone();
            store.remove(&namespace, &reference).await?;
        }
        self.database
            .execute(DbCommand::Delete(p.id.clone()))
            .await?;
        self.database
            .execute(DbCommand::FinishCredential(job))
            .await?;
        Ok(())
    }
    pub fn recover_credentials_background(self: &Arc<Self>) {
        if self.state.lock().unwrap().stopping {
            return;
        }
        let service = self.clone();
        self.tasks.spawn(async move {
            let _ = service.recover_credentials().await;
        });
    }
    pub async fn recover_credentials(&self) -> Result<(), WorkspaceError> {
        self.recover_credentials_for(None).await
    }
    /// 结算当前 save/delete 及历史 clear 待办；保留仍绑定或被运行组租用的凭据。
    pub(super) async fn recover_credentials_for(
        &self,
        only: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        let namespace = self.namespace().await?;
        let DbReply::Jobs(jobs) = self.database.execute(DbCommand::CredentialJobs).await? else {
            return Err(WorkspaceError::StorageUnavailable);
        };
        let mut pending_error = None;
        for job in jobs {
            if only.is_some_and(|id| id != job.connection_id) {
                continue;
            }
            {
                let mut state = self.state.lock().unwrap();
                if !state.changing.insert(job.connection_id.clone()) {
                    if only.is_some() {
                        return Err(WorkspaceError::Busy);
                    }
                    continue;
                }
            }
            let _reservation = ChangeReservation {
                state: &self.state,
                id: job.connection_id.clone(),
            };
            let current = match self.connection(&job.connection_id).await {
                Ok(p) => Some(p),
                Err(WorkspaceError::NotFound) => None,
                Err(e) => return Err(e),
            };
            let proxy_current = if current.is_none() {
                self.proxy(&job.connection_id).await.ok()
            } else {
                None
            };
            let current_reference = current
                .as_ref()
                .and_then(|p| p.credential.as_ref())
                .or_else(|| proxy_current.as_ref().and_then(|p| p.credential.as_ref()));
            let targets = match job.action.as_str() {
                "save" => job
                    .old_ref
                    .iter()
                    .chain(job.new_ref.iter())
                    .filter(|reference| Some(*reference) != current_reference)
                    .cloned()
                    .collect::<Vec<_>>(),
                "clear" => {
                    if current
                        .as_ref()
                        .is_some_and(|p| p.credential == job.old_ref && job.old_ref.is_some())
                    {
                        Vec::new()
                    } else {
                        job.old_ref.iter().cloned().collect()
                    }
                }
                "delete" => {
                    if current.as_ref().is_some_and(|p| !p.deleting) {
                        Vec::new()
                    } else {
                        job.old_ref.iter().cloned().collect()
                    }
                }
                _ => continue,
            };
            let mut remaining = false;
            for reference in targets {
                if self.credential_leased(&reference) {
                    remaining = true;
                    continue;
                }
                let store = self.secrets.clone();
                let namespace = namespace.clone();
                if let Err(e) = store.remove(&namespace, &reference).await {
                    pending_error = Some(e);
                    remaining = true;
                }
            }
            if remaining {
                continue;
            }
            if job.action == "delete" && current.is_some_and(|p| p.deleting) {
                self.database
                    .execute(DbCommand::Delete(job.connection_id))
                    .await?;
            }
            self.database
                .execute(DbCommand::FinishCredential(job.id))
                .await?;
        }
        if only.is_some()
            && let Some(e) = pending_error
        {
            return Err(e);
        }
        Ok(())
    }
}
