use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
impl LocalExecutor {
    pub(super) async fn run(
        &self,
        mut task: ExecutionTask,
        cancel: CancellationToken,
    ) -> AgentResult<()> {
        let started_at = tokio::time::Instant::now();
        let class = task.request.operation.execution_class();
        let limit = task
            .policy
            .execution
            .limits
            .timeout(class, task.request.timeout_seconds)?;
        let deadline = async {
            if let Some(seconds) = limit {
                tokio::time::sleep(Duration::from_secs(seconds.into())).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::pin!(deadline);
        // 同一连接的具名变更复用维护锁，避免本应用在版本核对与发布之间相互覆盖。
        let _mutation = if (task.request.operation.is_file_operation()
            || task.request.operation.is_native_operation())
            && task.request.operation.mutating()
        {
            let lock = self.scheduler.maintenance(&task.connection_id);
            let permit = tokio::select! {biased;
                _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=&mut deadline=>Err(NotExecutedReason::TimedOut),
                guard=lock.lock_owned()=>Ok(guard),
            };
            match permit {
                Ok(guard) => Some(guard),
                Err(reason) => {
                    return self
                        .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                        .await;
                }
            }
        } else {
            None
        };
        self.scheduler.configure(
            &task.connection_id,
            task.policy.revision,
            &task.policy.execution.limits,
        );
        let permit = tokio::select! {biased;
            _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=&mut deadline=>Err(NotExecutedReason::TimedOut),
            result=self.scheduler.acquire(&task.connection_id,class)=>result.map_err(|_|NotExecutedReason::Denied)
        };
        let _permit = match permit {
            Ok(p) => p,
            Err(reason) => {
                return self
                    .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                    .await;
            }
        };
        let current = tokio::select! {biased;
            _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=&mut deadline=>Err(NotExecutedReason::TimedOut),
            result=self.authority.current(task.request.identity.clone())=>result.map_err(|_|NotExecutedReason::Denied),
        };
        let access = match current {
            Ok(access) => access,
            Err(reason) => {
                task.error = Some("access_revoked".into());
                return self
                    .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                    .await;
            }
        };
        let checked = match checked(&task.request, &access.policy) {
            Ok(command) => command,
            Err(error) => {
                task.error = Some(error.into());
                return self
                    .finish(
                        &mut task,
                        ExecutionOutcome::NotExecuted {
                            reason: NotExecutedReason::Denied,
                        },
                    )
                    .await;
            }
        };
        if access.policy != task.policy || access.connection.is_closed() {
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Denied,
                    },
                )
                .await;
        }
        // 记录入口必须存在且可写；记录失败不发送 SSH exec。
        let recorded = tokio::select! {biased;
            _=cancel.cancelled()=>Err("cancelled_before_start"),
            _=self.shutdown.cancelled()=>Err("cancelled_before_start"),
            _=&mut deadline=>Err("timed_out_before_start"),
            result=self.output(&task, "stdout", String::new(),None)=>result,
        };
        if let Err(code) = recorded {
            task.error = Some(code.into());
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: match code {
                            "cancelled_before_start" => NotExecutedReason::Cancelled,
                            "timed_out_before_start" => NotExecutedReason::TimedOut,
                            _ => NotExecutedReason::Denied,
                        },
                    },
                )
                .await;
        }
        if cancel.is_cancelled() || self.shutdown.is_cancelled() {
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Cancelled,
                    },
                )
                .await;
        }
        // 记录预检可能等待磁盘；其后再读一次当前会话授权，避免使用预检前的版本。
        let rechecked = tokio::select! {biased;
            _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
            _=&mut deadline=>Err(NotExecutedReason::TimedOut),
            result=self.authority.current(task.request.identity.clone())=>result.map_err(|_|NotExecutedReason::Denied),
        };
        let access = match rechecked {
            Ok(current) if current.policy == task.policy && !current.connection.is_closed() => {
                current
            }
            Ok(_) => {
                return self
                    .finish(
                        &mut task,
                        ExecutionOutcome::NotExecuted {
                            reason: NotExecutedReason::Denied,
                        },
                    )
                    .await;
            }
            Err(reason) => {
                return self
                    .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                    .await;
            }
        };
        if task.request.operation.is_native_operation() {
            let operation = check_native_operation(&task.request, &access.policy)?;
            let prepared = tokio::select! {biased;
                _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=&mut deadline=>Err(NotExecutedReason::TimedOut),
                result=access.connection.clone().prepare_native(operation)=>result.map_err(|_|NotExecutedReason::Denied),
            };
            let prepared = match prepared {
                Ok(p) => p,
                Err(reason) => {
                    task.error = Some("native_preparation_failed".into());
                    return self
                        .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                        .await;
                }
            };
            let latest = tokio::select! {biased;
                _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=&mut deadline=>Err(NotExecutedReason::TimedOut),
                result=self.authority.current(task.request.identity.clone())=>result.map_err(|_|NotExecutedReason::Denied),
            };
            match latest {
                Ok(a)
                    if a.policy == task.policy
                        && Arc::ptr_eq(&a.connection, &access.connection)
                        && !a.connection.is_closed() => {}
                other => {
                    return self
                        .finish(
                            &mut task,
                            ExecutionOutcome::NotExecuted {
                                reason: other.err().unwrap_or(NotExecutedReason::Denied),
                            },
                        )
                        .await;
                }
            }
            task.native_report = Some(NativeExecutionReport {
                outcome: None,
                effects: FileCommitState::Unchanged,
                error: None,
                json: None,
                recovery_paths: prepared.recovery_paths(),
            });
            task.state.begin_dispatch(task.id.clone(), now())?;
            self.save(&mut task).await?;
            return self
                .run_native(
                    task,
                    prepared,
                    cancel,
                    limit.map(|s| {
                        Duration::from_secs(s.into()).saturating_sub(started_at.elapsed())
                    }),
                )
                .await;
        }
        if task.request.operation.is_file_operation() {
            let prepared = tokio::select! {biased;
                _=cancel.cancelled()=>Err(("cancelled_before_start",NotExecutedReason::Cancelled)),
                _=self.shutdown.cancelled()=>Err(("cancelled_before_start",NotExecutedReason::Cancelled)),
                _=&mut deadline=>Err(("timed_out_before_start",NotExecutedReason::TimedOut)),
                result=self.prepare_files(&task)=>result.map_err(|e|(e,NotExecutedReason::Denied)),
            };
            let io = match prepared {
                Ok(io) => io,
                Err((error, reason)) => {
                    task.error = Some(error.into());
                    return self
                        .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                        .await;
                }
            };
            let latest = tokio::select! {biased;
                _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=&mut deadline=>Err(NotExecutedReason::TimedOut),
                result=self.authority.current(task.request.identity.clone())=>result.map_err(|_|NotExecutedReason::Denied),
            };
            let access = match latest {
                Ok(a) if a.policy == task.policy && !a.connection.is_closed() => a,
                Ok(_) => {
                    return self
                        .finish(
                            &mut task,
                            ExecutionOutcome::NotExecuted {
                                reason: NotExecutedReason::Denied,
                            },
                        )
                        .await;
                }
                Err(reason) => {
                    return self
                        .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                        .await;
                }
            };
            let operation = check_file_operation(&task.request, &access.policy)?;
            task.file_report = Some(self.pending_file_report(&operation)?);
            task.state.begin_dispatch(task.id.clone(), now())?;
            self.save(&mut task).await?;
            return self
                .run_files(
                    task,
                    access,
                    operation,
                    io,
                    cancel,
                    limit.map(|s| {
                        Duration::from_secs(s.into()).saturating_sub(started_at.elapsed())
                    }),
                )
                .await;
        }
        let script = if matches!(task.request.operation, RuntimeOperation::Script { .. }) {
            let snapshot = task.script_snapshot.clone();
            let staging = task.script_staging_path.clone();
            Some(check_script_launch(
                &task.request,
                &access.policy,
                &task.state,
                snapshot,
                staging,
            )?)
        } else {
            None
        };
        let script = if let Some(checked_script) = script {
            let result = tokio::select! {biased;
                _=cancel.cancelled()=>Err(("cancelled_before_start",NotExecutedReason::Cancelled)),
                _=self.shutdown.cancelled()=>Err(("cancelled_before_start",NotExecutedReason::Cancelled)),
                _=&mut deadline=>Err(("timed_out_before_start",NotExecutedReason::TimedOut)),
                result=access.connection.clone().prepare_script_launch(checked_script)=>result.map_err(|e|(if matches!(e,WorkspaceError::Conflict|WorkspaceError::NotFound){"script_source_changed"}else{"script_preparation_failed"},NotExecutedReason::Denied)),
            };
            match result {
                Ok(prepared) => Some(prepared),
                Err((code, reason)) => {
                    task.error = Some(code.into());
                    return self
                        .finish(&mut task, ExecutionOutcome::NotExecuted { reason })
                        .await;
                }
            }
        } else {
            None
        };
        if script.is_some() {
            let current = tokio::select! {biased;
                _=cancel.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=self.shutdown.cancelled()=>Err(NotExecutedReason::Cancelled),
                _=&mut deadline=>Err(NotExecutedReason::TimedOut),
                result=self.authority.current(task.request.identity.clone())=>result.map_err(|_|NotExecutedReason::Denied),
            };
            match current {
                Ok(current) if current.policy == task.policy && !current.connection.is_closed() => {
                }
                other => {
                    task.error = Some("access_revoked".into());
                    return self
                        .finish(
                            &mut task,
                            ExecutionOutcome::NotExecuted {
                                reason: other.err().unwrap_or(NotExecutedReason::Denied),
                            },
                        )
                        .await;
                }
            }
        }
        let terminal = if task.request.operation.is_terminal() {
            Some(check_terminal(&task.request, &access.policy)?)
        } else {
            None
        };
        if checked.is_none() && script.is_none() && terminal.is_none() {
            return Err("command_required");
        }
        let encoding = encoding_rs::Encoding::for_label(access.connection.encoding().as_bytes())
            .filter(|e| e.is_ascii_compatible());
        let Some(encoding) = encoding else {
            task.error = Some("invalid_encoding".into());
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Denied,
                    },
                )
                .await;
        };
        if checked.as_ref().is_some_and(|checked| {
            encoding.encode(checked.shell_source()).2 || encoding.encode(checked.directory()).2
        }) {
            task.error = Some("unmappable_input".into());
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Denied,
                    },
                )
                .await;
        }
        if terminal
            .as_ref()
            .is_some_and(|p| encoding.encode(p.directory()).2)
        {
            return self
                .finish(
                    &mut task,
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Denied,
                    },
                )
                .await;
        }
        task.state.begin_dispatch(task.id.clone(), now())?;
        self.save(&mut task).await?;
        let launched = tokio::select! {biased;
            _=cancel.cancelled()=>Err(ExecutionStopReason::Cancelled),
            _=self.shutdown.cancelled()=>Err(ExecutionStopReason::Cancelled),
            _=&mut deadline=>Err(ExecutionStopReason::TimedOut),
            result=async {if let Some(script)=script {script.launch().await}else if let Some(terminal)=terminal{access.connection.execute_terminal(terminal).await}else{access.connection.execute_command(checked.unwrap()).await}}=>result.map_err(|_|ExecutionStopReason::Disconnected),
        };
        let mut process = match launched {
            Ok(p) => p,
            Err(reason) => {
                return self
                    .finish(&mut task, ExecutionOutcome::Unknown { reason })
                    .await;
            }
        };
        let (terminal_tx, mut terminal_rx) =
            tokio::sync::mpsc::channel::<terminal::TerminalRequest>(32);
        if task.state.terminal {
            self.terminals
                .lock()
                .unwrap()
                .insert(task.id.clone(), terminal_tx);
        }
        let idle_seconds = task
            .policy
            .execution
            .limits
            .terminal_idle_seconds
            .filter(|_| task.state.terminal);
        let idle = tokio::time::sleep(Duration::from_secs(u64::from(
            idle_seconds.unwrap_or(u32::MAX),
        )));
        tokio::pin!(idle);
        task.state.phase = ExecutionPhase::Running;
        if let Err(error) = self.save(&mut task).await {
            let _ = process.stop().await;
            return Err(error);
        }
        let mut out = encoding.new_decoder();
        let mut err = encoding.new_decoder();
        let mut stopping = None;
        let mut exit = None;
        let mut signal = false;
        let grace = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(grace);
        let result = loop {
            let event = tokio::select! {biased;
                _=cancel.cancelled(),if stopping.is_none()=> {stopping=Some(ExecutionStopReason::Cancelled);None},
                _=self.shutdown.cancelled(),if stopping.is_none()=> {stopping=Some(ExecutionStopReason::Cancelled);None},
                _=&mut deadline,if stopping.is_none()=> {stopping=Some(ExecutionStopReason::TimedOut);None},
                _=&mut grace,if stopping.is_some()=>break None,
                _=&mut idle,if idle_seconds.is_some() && stopping.is_none()=>{task.error=Some("terminal_idle_timeout".into());stopping=Some(ExecutionStopReason::TimedOut);None},
                Some(input)=terminal_rx.recv(),if task.state.terminal && stopping.is_none()=>{
                    let allowed=self.authority.current(input.identity).await.is_ok_and(|a|a.policy.execution.authorize(&task.request.operation).is_ok());
                    let delivery=if !allowed{TerminalDelivery::NotSent("access_revoked")}else{
                        let sent=match input.control {
                            TerminalControl::Write{text}=>{let (bytes,_,bad)=encoding.encode(&text);if bad||bytes.len()>65536 {let _=input.reply.send(TerminalDelivery::NotSent("unmappable_input"));continue;}process.input(&bytes).await},
                            TerminalControl::Resize{columns,rows}=>process.resize(u32::from(columns),u32::from(rows)).await,
                        };
                        if sent.is_ok(){TerminalDelivery::Sent}else{TerminalDelivery::Unknown}
                    };
                    let _=input.reply.send(delivery);if let Some(seconds)=idle_seconds{idle.as_mut().reset(tokio::time::Instant::now()+Duration::from_secs(seconds.into()));}continue;
                },
                result=process.read()=>Some(result),
            };
            let Some(event) = event else {
                task.state.phase = ExecutionPhase::Stopping;
                if self.save(&mut task).await.is_err() {
                    break None;
                }
                let _ = tokio::time::timeout(Duration::from_secs(1), process.signal_stop()).await;
                grace
                    .as_mut()
                    .reset(tokio::time::Instant::now() + Duration::from_secs(3));
                continue;
            };
            match event {
                Ok(AgentProcessEvent::Stdout(bytes)) | Ok(AgentProcessEvent::Stderr(bytes))
                    if bytes.len() > 65536 =>
                {
                    break None;
                }
                Ok(AgentProcessEvent::Stdout(bytes)) => {
                    if let Some(seconds) = idle_seconds {
                        idle.as_mut().reset(
                            tokio::time::Instant::now() + Duration::from_secs(seconds.into()),
                        );
                    }
                    if self
                        .decode_output(&task, "stdout", &mut out, &bytes, false)
                        .await
                        .is_err()
                    {
                        task.output_incomplete = true;
                        break None;
                    }
                }
                Ok(AgentProcessEvent::Stderr(bytes)) => {
                    if let Some(seconds) = idle_seconds {
                        idle.as_mut().reset(
                            tokio::time::Instant::now() + Duration::from_secs(seconds.into()),
                        );
                    }
                    if self
                        .decode_output(&task, "stderr", &mut err, &bytes, false)
                        .await
                        .is_err()
                    {
                        task.output_incomplete = true;
                        break None;
                    }
                }
                Ok(AgentProcessEvent::Exit(code)) => {
                    if exit.is_some() || signal {
                        break None;
                    }
                    exit = Some(code);
                }
                Ok(AgentProcessEvent::ExitSignal {
                    signal: name,
                    core_dumped,
                }) => {
                    task.error = Some(format!(
                        "remote_exit_signal:{name}:core_dumped={core_dumped}"
                    ));
                    if exit.is_some() || signal {
                        break None;
                    }
                    signal = true;
                }
                Ok(AgentProcessEvent::Eof) => {}
                Ok(AgentProcessEvent::Closed) => break Some(()),
                Err(_) => break None,
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), process.stop()).await;
        for (stream, decoder) in [("stdout", &mut out), ("stderr", &mut err)] {
            if self
                .decode_output(&task, stream, decoder, &[], true)
                .await
                .is_err()
            {
                task.output_incomplete = true;
            }
        }
        let reason = stopping.unwrap_or(ExecutionStopReason::Disconnected);
        let outcome = if result.is_some() && (exit.is_some() || signal) {
            if stopping.is_some() || signal {
                ExecutionOutcome::Partial {
                    reason,
                    termination_confirmed: true,
                }
            } else {
                ExecutionOutcome::Completed {
                    exit_code: exit.unwrap(),
                }
            }
        } else {
            ExecutionOutcome::Unknown { reason }
        };
        self.finish(&mut task, outcome).await
    }
    async fn decode_output(
        &self,
        task: &ExecutionTask,
        stream: &str,
        decoder: &mut encoding_rs::Decoder,
        bytes: &[u8],
        last: bool,
    ) -> AgentResult<()> {
        let mut text = String::with_capacity(bytes.len() * 4 + 16);
        let (_, consumed, _) = decoder.decode_to_string(bytes, &mut text, last);
        if consumed != bytes.len() {
            return Err("output_limit");
        }
        if !text.is_empty() || !bytes.is_empty() {
            self.output(
                task,
                stream,
                text,
                if bytes.is_empty() {
                    None
                } else {
                    Some(STANDARD.encode(bytes))
                },
            )
            .await?;
        }
        Ok(())
    }
    pub(super) async fn output(
        &self,
        task: &ExecutionTask,
        stream: &str,
        text: String,
        data: Option<String>,
    ) -> AgentResult<()> {
        self.output_formatted(task, stream, text, data, ActivityOutputFormat::Text)
            .await
    }
    pub(super) async fn output_formatted(
        &self,
        task: &ExecutionTask,
        stream: &str,
        text: String,
        data: Option<String>,
        format: ActivityOutputFormat,
    ) -> AgentResult<()> {
        let record = ActivityRecord {
            session_id: task.context_id.clone(),
            task_id: Some(task.id.clone()),
            parent_task_id: None,
            sequence: 0,
            timestamp_ms: now(),
            actor: ActivityActor::Agent,
            policy_revision: task.policy.revision,
            body: ActivityBody::Output {
                format,
                stream: stream.into(),
                text,
                data,
            },
        };
        match self
            .database
            .execute(DbCommand::Agent(AgentDbCommand::recording(
                RecordingCommand::Append {
                    record,
                    limit_bytes: self
                        .recording_limit_bytes
                        .load(std::sync::atomic::Ordering::Relaxed),
                },
            )))
            .await
        {
            Ok(DbReply::Agent(AgentDbReply::Recorded(record))) => {
                let _ = self
                    .events
                    .send(LocalExecutionEvent::Activity(Box::new(record)));
                Ok(())
            }
            _ => Err("activity_recording_unavailable"),
        }
    }
}
