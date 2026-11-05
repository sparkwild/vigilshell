use super::*;
impl WorkspaceService {
    pub(super) async fn start_test(
        self: &Arc<Self>,
        owner: &str,
        request: TestRequest,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let profile = if let Some(id) = &request.connection_id {
            Some(self.connection(id).await?)
        } else {
            None
        };
        if profile.as_ref().is_some_and(|p| p.deleting) {
            return Err(WorkspaceError::Busy);
        }
        let input = request
            .input
            .or_else(|| profile.as_ref().map(|p| p.input.clone()))
            .ok_or(WorkspaceError::InvalidInput)?
            .normalized()?;
        let secret = self.draft_secret(
            owner,
            request.draft_id.as_deref(),
            profile.as_ref(),
            input.auth,
        )?;
        if secret.is_none()
            && profile
                .as_ref()
                .and_then(|p| p.credential.as_ref())
                .is_none()
        {
            return Err(WorkspaceError::CredentialMissing);
        }
        let permit = self
            .network
            .clone()
            .try_acquire_owned()
            .map_err(|_| WorkspaceError::Busy)?;
        let operation = self.begin(owner, "test", true)?;
        {
            let mut state = self.state.lock().unwrap();
            if let Some(d) = request
                .draft_id
                .as_ref()
                .and_then(|id| state.drafts.get_mut(id))
            {
                d.busy = true;
            }
        }
        let service = self.clone();
        let owner = owner.to_owned();
        let id = operation.id.clone();
        let (cancel, _) = {
            let state = self.state.lock().unwrap();
            let o = &state.operations[&id];
            (o.cancel.clone(), o.flag.clone())
        };
        self.tasks.spawn(async move {
            let _permit=permit;
            let work=async {
                let proxy=if let Some(id)=&input.proxy_id{Some(service.proxy(id).await?)}else{None};
                let route=service.route(proxy.as_ref()).await?;
                let key=service.verify(&owner,&id,&input,profile.as_ref(),request.draft_id.as_deref(),route.as_ref().map(ProxyRoute::duplicate)).await?;
                let secret=service.load_secret(secret,profile.as_ref(),input.auth).await?;
                service.stage(&id,"authenticating").await;
                let mut session=service.transport.authenticate_via(input,key,secret,route).await?;
                session.close().await
            };
            let result=tokio::select!{biased;()=cancel.cancelled()=>Err(WorkspaceError::Cancelled),r=work=>r};
            {let mut state=service.state.lock().unwrap();if let Some(d)=request.draft_id.as_ref().and_then(|id|state.drafts.get_mut(id)){d.busy=false;}state.challenges.retain(|_,c|c.operation!=id);}
            service.finish(&id,result).await;
        });
        Ok(WorkspaceResult::Operation(Box::new(operation)))
    }
    fn draft_secret(
        &self,
        owner: &str,
        id: Option<&str>,
        profile: Option<&ConnectionRecord>,
        kind: AuthKind,
    ) -> Result<Option<SecretMaterial>, WorkspaceError> {
        if let Some(id) = id {
            let state = self.state.lock().unwrap();
            let draft = state
                .drafts
                .get(id)
                .filter(|d| d.owner == owner)
                .ok_or(WorkspaceError::NotFound)?;
            if draft.proxy.is_some() {
                return Err(WorkspaceError::InvalidInput);
            }
            if draft.busy || draft.connection.as_ref().map(|p| &p.id) != profile.map(|p| &p.id) {
                return Err(WorkspaceError::Busy);
            }
            if draft
                .connection
                .as_ref()
                .zip(profile)
                .is_some_and(|(old, current)| old.version != current.version)
            {
                return Err(WorkspaceError::Conflict);
            }
            if draft.intent == CredentialIntent::Remove {
                return Err(WorkspaceError::CredentialMissing);
            }
            if let Some(secret) = &draft.secret {
                if secret.kind != kind {
                    return Err(WorkspaceError::CredentialMissing);
                }
                return Ok(Some(secret.duplicate()));
            }
        }
        if profile.is_some_and(|p| p.input.auth != kind) {
            return Err(WorkspaceError::CredentialMissing);
        }
        Ok(None)
    }
    async fn load_secret(
        &self,
        prepared: Option<SecretMaterial>,
        profile: Option<&ConnectionRecord>,
        kind: AuthKind,
    ) -> Result<SecretMaterial, WorkspaceError> {
        if let Some(secret) = prepared {
            if let Some(trace) = &secret.diagnostic {
                trace.check("test_draft_input", &secret.bytes);
            }
            return Ok(secret);
        }
        let p = profile.ok_or(WorkspaceError::CredentialMissing)?;
        let current = self.connection(&p.id).await?;
        if current.revision != p.revision || current.deleting {
            return Err(WorkspaceError::Conflict);
        }
        let reference = p
            .credential
            .clone()
            .ok_or(WorkspaceError::CredentialMissing)?;
        let namespace = self.namespace().await?;
        let store = self.secrets.clone();
        store
            .read(
                &namespace,
                &reference,
                CredentialContext::login(&p.id, kind),
            )
            .await
    }
    async fn verify(
        &self,
        owner: &str,
        operation: &str,
        input: &ConnectionInput,
        profile: Option<&ConnectionRecord>,
        draft: Option<&str>,
        route: Option<ProxyRoute>,
    ) -> Result<HostKey, WorkspaceError> {
        self.stage(operation, "connecting").await;
        let observed = self.transport.probe_via(input.clone(), route).await?;
        let prepared = draft.and_then(|id| {
            self.state
                .lock()
                .unwrap()
                .drafts
                .get(id)
                .filter(|d| d.owner == owner)
                .and_then(|d| {
                    d.trust
                        .as_ref()
                        .filter(|(saved, _)| saved.same_endpoint(input))
                        .map(|(_, key)| key.clone())
                })
        });
        let saved = if let Some(p) = profile {
            if p.input.same_endpoint(input) {
                self.trust(&p.id).await?.map(|t| t.key)
            } else {
                None
            }
        } else {
            None
        };
        let expected = prepared.or(saved);
        if expected.as_ref().is_some_and(|key| {
            key.public_key == observed.public_key && key.fingerprint == observed.fingerprint
        }) {
            return Ok(observed);
        }
        let id = new_id();
        let (sender, receiver) = oneshot::channel();
        self.state.lock().unwrap().challenges.insert(
            id.clone(),
            Challenge {
                owner: owner.into(),
                operation: operation.into(),
                sender,
            },
        );
        let view = ChallengeView {
            id: id.clone(),
            input: input.clone(),
            key: observed.clone(),
            previous: expected.map(|k| k.fingerprint),
            temporary: draft.is_some(),
        };
        self.update(operation, |o| {
            o.phase = "trust".into();
            o.stage = "trust".into();
            o.challenge = Some(view);
        })
        .await;
        self.session_stage(operation, "trust", None).await;
        let accepted = tokio::time::timeout(Duration::from_secs(60), receiver)
            .await
            .map_err(|_| WorkspaceError::TrustExpired)
            .and_then(|v| v.map_err(|_| WorkspaceError::Cancelled));
        self.state.lock().unwrap().challenges.remove(&id);
        if !accepted? {
            return Err(WorkspaceError::Cancelled);
        }
        if let Some(id) = draft {
            let mut state = self.state.lock().unwrap();
            let d = state
                .drafts
                .get_mut(id)
                .filter(|d| d.owner == owner)
                .ok_or(WorkspaceError::Cancelled)?;
            d.trust = Some((input.clone(), observed.clone()));
        } else if let Some(p) = profile {
            let current = self.connection(&p.id).await?;
            if current.revision != p.revision || current.deleting {
                return Err(WorkspaceError::Conflict);
            }
            self.database
                .execute(DbCommand::SetTrust {
                    id: p.id.clone(),
                    revision: p.revision,
                    key: observed.clone(),
                })
                .await?;
            self.changed(owner).await;
        }
        self.update(operation, |o| {
            o.phase = "working".into();
            o.challenge = None;
        })
        .await;
        Ok(observed)
    }
    async fn stage(&self, id: &str, stage: &str) {
        self.update(id, |o| o.stage = stage.into()).await;
        self.session_stage(id, stage, None).await;
    }
    async fn session_stage(&self, operation: &str, phase: &str, error: Option<WorkspaceError>) {
        let value = {
            let mut state = self.state.lock().unwrap();
            state
                .sessions
                .values_mut()
                .find(|s| s.view.operation_id == operation)
                .map(|s| {
                    s.view.phase = phase.into();
                    s.view.error = error;
                    if s.view.end_reason.is_none() && matches!(phase, "failed" | "cancelled") {
                        s.view.end_reason = Some(SessionEndReason {
                            kind: if phase == "cancelled" {
                                SessionEndKind::Cancelled
                            } else {
                                SessionEndKind::TransportError
                            },
                            message: None,
                        });
                    }
                    (s.owner.clone(), s.view.clone())
                })
        };
        if let Some((owner, view)) = value {
            let _ = self
                .sink
                .event(owner, WorkspaceNotice::Session(Box::new(view)))
                .await;
        }
    }
    pub(super) async fn start_open(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        columns: u16,
        rows: u16,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        self.start_open_owned(owner, id, columns, rows, None).await
    }
    pub(super) async fn start_open_owned(
        self: &Arc<Self>,
        owner: &str,
        id: &str,
        columns: u16,
        rows: u16,
        agent: Option<WorkspaceAgentIdentity>,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        if columns == 0 || rows == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        let _group_guard = self.group_gate.lock().await;
        let profile = self.connection(id).await?;
        if profile.deleting {
            return Err(WorkspaceError::Busy);
        }
        if profile.credential.is_none() {
            return Err(WorkspaceError::CredentialMissing);
        }
        let permit = self
            .network
            .clone()
            .try_acquire_owned()
            .map_err(|_| WorkspaceError::Busy)?;
        {
            let state = self.state.lock().unwrap();
            let active = state
                .sessions
                .values()
                .filter(|s| !matches!(s.view.phase.as_str(), "closed" | "failed" | "cancelled"))
                .collect::<Vec<_>>();
            if state.changing.contains(id)
                || active.len() >= 32
                || active.iter().filter(|s| s.view.connection.id == id).count() >= 4
            {
                return Err(WorkspaceError::Busy);
            }
        }
        // Agent 新建标签只使用已保存的主机信任，不代用户确认新主机。
        let agent_trust = if let Some(identity) = &agent {
            if identity.connection_revision != profile.revision || profile.credential_pending {
                return Err(WorkspaceError::Busy);
            }
            let trust = self.trust(id).await?.ok_or(WorkspaceError::TrustRequired)?;
            if trust.host != profile.input.host || trust.port != profile.input.port {
                return Err(WorkspaceError::TrustRequired);
            }
            Some(trust.key)
        } else {
            None
        };
        let mut operation = self.begin(owner, "connect", true)?;
        let group = match self.runtime_group(owner, profile).await {
            Ok(g) => g,
            Err(e) => {
                self.finish(&operation.id, Err(e)).await;
                return Err(e);
            }
        };
        if agent
            .as_ref()
            .is_some_and(|identity| group.profile.revision != identity.connection_revision)
        {
            self.release_group(owner, &group).await;
            self.finish(&operation.id, Err(WorkspaceError::Busy)).await;
            return Err(WorkspaceError::Busy);
        }
        group.retry_credentials();
        let profile = group.profile.clone();
        let session_id = new_id();
        operation.session_id = Some(session_id.clone());
        let cancel = CancellationToken::new();
        let mut control = vigilshell_domain::workspace_control::WorkspaceControl::default();
        let agent_control = agent.as_ref().map(|identity| {
            let lease = control
                .acquire(
                    &identity.entry_id,
                    &identity.context_id,
                    &identity.context_id,
                )
                .expect("new control with validated identity");
            vigilshell_domain::agent::WorkspaceAgentControl {
                entry_id: identity.entry_id.clone(),
                entry_name: identity.entry_name.clone(),
                context_id: identity.context_id.clone(),
                lease_id: lease.lease_id,
                releasing: false,
            }
        });
        {
            let mut state = self.state.lock().unwrap();
            state
                .operations
                .get_mut(&operation.id)
                .unwrap()
                .view
                .session_id = Some(session_id.clone());
            state.sessions.insert(
                session_id.clone(),
                Session {
                    control_deadline: agent
                        .as_ref()
                        .and_then(|a| a.terminal_seconds)
                        .map(|s| std::time::Instant::now() + Duration::from_secs(u64::from(s))),
                    control_idle_deadline: None,
                    control_idle_seconds: agent.as_ref().and_then(|a| a.idle_seconds),
                    control,
                    write_gate: Default::default(),
                    output_history: Default::default(),
                    output_bytes: 0,
                    output_cursor: 0,
                    agent_inputs: Default::default(),
                    owner: owner.into(),
                    view: SessionView {
                        agent_control,
                        group_id: group.tools.id.clone(),
                        id: session_id.clone(),
                        operation_id: operation.id.clone(),
                        connection: profile.clone(),
                        phase: "connecting".into(),
                        error: None,
                        exit_code: None,
                        end_reason: None,
                    },
                    cancel: cancel.clone(),
                    controls: None,
                },
            );
        }
        let service = self.clone();
        let owner = owner.to_owned();
        let id = operation.id.clone();
        let op_cancel = self.state.lock().unwrap().operations[&id].cancel.clone();
        self.tasks.spawn(async move {
            let establish=async {
                let route=service.group_route(&group).await?;
                let key=group.trusted.get_or_try_init(||async { if let Some(key)=agent_trust { Ok(key) } else { service.verify(&owner,&id,&profile.input,Some(&profile),None,route.as_ref().map(ProxyRoute::duplicate)).await } }).await?.clone();
                let secret=service.group_secret(&group).await?;
                service.stage(&id,"authenticating").await;
                let mut session=service.transport.authenticate_via(profile.input.clone(),key,secret,route).await?;
                service.stage(&id,"opening").await;session.open_terminal(columns,rows).await?;Ok::<_,WorkspaceError>(session)
            };
            let established=tokio::select!{biased;()=cancel.cancelled()=>Err(WorkspaceError::Cancelled),()=op_cancel.cancelled()=>Err(WorkspaceError::Cancelled),r=establish=>r};
            drop(permit);
            match established {
                Err(e)=>{service.session_stage(&id,if e==WorkspaceError::Cancelled{"cancelled"}else{"failed"},Some(e)).await;service.finish(&id,Err(e)).await;}
                Ok(session)=>{
                    let(tx,rx)=mpsc::channel(128);{let mut state=service.state.lock().unwrap();if let Some(s)=state.sessions.get_mut(&session_id){s.controls=Some(tx);}}
                    service.session_stage(&id,"running",None).await;service.finish(&id,Ok(())).await;
                    if agent.is_none() { service.begin_tunnels(owner.clone(),group.clone()); }
                    let result=service.run_session(&owner,&session_id,session,rx,cancel).await;
                    service.session_stage(&id,if result.is_ok(){"closed"}else{"failed"},result.err()).await;
                    if let Some(s)=service.state.lock().unwrap().sessions.get_mut(&session_id){s.controls=None;}
                }
            }
            service.release_group(&owner,&group).await;
            {let mut state=service.state.lock().unwrap();state.challenges.retain(|_,c|c.operation!=id);state.closed.push_back(session_id);while state.closed.len()>16{if let Some(id)=state.closed.pop_front(){state.sessions.remove(&id);}}}
        });
        Ok(WorkspaceResult::Operation(Box::new(operation)))
    }
    pub(super) async fn run_session(
        &self,
        owner: &str,
        id: &str,
        mut session: Box<dyn SshSession>,
        mut controls: mpsc::Receiver<SessionCommand>,
        cancel: CancellationToken,
    ) -> Result<(), WorkspaceError> {
        let settings = self
            .state
            .lock()
            .unwrap()
            .sessions
            .get(id)
            .map(|s| s.view.connection.input.terminal.clone())
            .unwrap_or_default();
        let write_gate = self
            .state
            .lock()
            .unwrap()
            .sessions
            .get(id)
            .map(|session| session.write_gate.clone())
            .unwrap_or_default();
        let mut codec = codec::Codec::new(&settings.encoding)?;
        let mut credit = 256usize * 1024;
        let mut sequence = 0u64;
        let mut input_sequence = 0u64;
        let mut in_flight = VecDeque::new();
        let mut pending: VecDeque<Vec<u8>> = VecDeque::new();
        let mut exit = None;
        let mut eof = false;
        let mut remote_message = None;
        let mut remote_exit = false;
        let work = async {
            loop {
                if eof && pending.is_empty() {
                    return Ok(());
                }
                if let Some(data) = pending.front_mut()
                    && credit > 0
                    && in_flight.len() < 32
                {
                    let n = data.len().min(credit).min(32 * 1024);
                    let bytes = data.drain(..n).collect::<Vec<_>>();
                    if data.is_empty() {
                        pending.pop_front();
                    }
                    sequence += 1;
                    credit -= n;
                    in_flight.push_back((sequence, n));
                    {
                        let mut state = self.state.lock().unwrap();
                        if let Some(session) = state.sessions.get_mut(id) {
                            session.control_activity();
                            session.output_cursor = sequence;
                            session.output_bytes += bytes.len();
                            session.output_history.push_back((sequence, bytes.clone()));
                            while session.output_bytes > 256 * 1024 {
                                if let Some((_, bytes)) = session.output_history.pop_front() {
                                    session.output_bytes -= bytes.len();
                                }
                            }
                        }
                    }
                    self.sink
                        .output(owner.into(), id.into(), sequence, bytes)
                        .await?;
                    continue;
                }
                tokio::select! {biased;
                    command=controls.recv()=>match command {
                        Some(SessionCommand::Input{sequence,bytes,generation})=>{
                            if sequence!=input_sequence+1{return Err(WorkspaceError::StreamUnavailable);}
                            let _gate = write_gate.lock().await;
                            let allowed = self.state.lock().unwrap().sessions.get(id).is_some_and(|s| generation.is_some() && s.control.manual_generation() == generation);
                            if !allowed {
                                input_sequence = sequence;
                                self.sink.input_ack(owner.into(), id.into(), sequence, bytes.len() as u32).await?;
                                continue;
                            }
                            match codec.input(&bytes){Ok(encoded)=>{if !encoded.is_empty(){session.write(&encoded).await?;}},Err(WorkspaceError::UnmappableInput)=>{let _=self.sink.event(owner.into(),WorkspaceNotice::InputRejected{session_id:id.into(),code:"unmappable_input".into()}).await;},Err(e)=>return Err(e)}input_sequence=sequence;self.sink.input_ack(owner.into(),id.into(),sequence,bytes.len()as u32).await?;
                        }
                        Some(SessionCommand::AgentInput { request_key, control, bytes, reply }) => {
                            let _gate = write_gate.lock().await;
                            let allowed = self.state.lock().unwrap().sessions.get(id).is_some_and(|s| s.agent_input_allowed(&control));
                            let result = if allowed {
                                match codec.input(&bytes) { Ok(encoded) => tokio::time::timeout(Duration::from_secs(5), session.write(&encoded)).await.unwrap_or(Err(WorkspaceError::Timeout)), Err(error) => Err(error) }
                            } else { Err(WorkspaceError::Cancelled) };
                            if let Some(receipt) = self.state.lock().unwrap().sessions.get_mut(id).and_then(|s| s.agent_inputs.get_mut(&request_key)) { receipt.1 = Some(result.is_ok()); }
                            if result.is_ok() && let Some(s) = self.state.lock().unwrap().sessions.get_mut(id) { s.control_activity(); }
                            let _ = reply.send(result);
                        }
                        Some(SessionCommand::Credit{sequence,length})=>{
                            if in_flight.pop_front()!=Some((sequence,length as usize)){return Err(WorkspaceError::StreamUnavailable);}credit+=length as usize;if credit>256*1024{return Err(WorkspaceError::StreamUnavailable);}
                        }
                        Some(SessionCommand::Resize(c,r))=>session.resize(c,r).await?,
                        None=>return Err(WorkspaceError::StreamUnavailable),
                    },
                    event=session.read(),if pending.is_empty() && !eof=>match event?{
                        TransportEvent::Data(bytes)=>{if bytes.len()>1024*1024{return Err(WorkspaceError::StreamUnavailable);}let bytes=codec.output(&bytes,false)?;if !bytes.is_empty(){pending.push_back(bytes);}},
                        TransportEvent::Exit(code)=>{exit=Some(code);remote_exit=true;},
                        TransportEvent::ExitSignal(message)=>{remote_exit=true;remote_message=Some(message);},
                        TransportEvent::Disconnected{error,message}=>{remote_message=message;if let Some(error)=error{return Err(error);}eof=true;let bytes=codec.output(&[],true)?;if !bytes.is_empty(){pending.push_back(bytes);}},
                        TransportEvent::Eof=>{eof=true;let bytes=codec.output(&[],true)?;if !bytes.is_empty(){pending.push_back(bytes);}},
                    },
                }
            }
        };
        let (result, cancelled) =
            tokio::select! {biased;()=cancel.cancelled()=>(Ok(()), true),r=work=>(r, false)};
        let _ = session.close().await;
        let _ = self.sink.end(owner.into(), id.into(), sequence).await;
        if let Some(s) = self.state.lock().unwrap().sessions.get_mut(id) {
            s.view.exit_code = exit;
            s.view.end_reason = Some(SessionEndReason {
                kind: if result.is_err() {
                    SessionEndKind::TransportError
                } else if cancelled {
                    SessionEndKind::UserDisconnected
                } else if remote_exit {
                    SessionEndKind::RemoteExit
                } else {
                    SessionEndKind::RemoteClosed
                },
                message: remote_message,
            });
        }
        result
    }
}
