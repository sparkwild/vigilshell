use super::*;
use std::sync::atomic::Ordering;
impl LocalExecutor {
    pub(super) async fn prepare_files(
        &self,
        task: &ExecutionTask,
    ) -> AgentResult<FileTransferHandles> {
        let mut io = FileTransferHandles::default();
        match &task.request.operation {
            operation @ RuntimeOperation::TransfersUpload { .. } => {
                io.upload = Some(
                    self.local_files
                        .upload(local_transfer_grant(&task.policy.execution, operation)?)
                        .await
                        .map_err(|_| "local_transfer_denied")?,
                )
            }
            operation @ RuntimeOperation::TransfersDownload { overwrite, .. } => {
                io.download = Some(
                    self.local_files
                        .download(
                            local_transfer_grant(&task.policy.execution, operation)?,
                            *overwrite,
                        )
                        .await
                        .map_err(|_| "local_transfer_denied")?,
                )
            }
            _ => {}
        }
        Ok(io)
    }
    pub(super) fn pending_file_report(
        &self,
        operation: &CheckedFileOperation,
    ) -> AgentResult<FileExecutionReport> {
        let target = match operation.operation() {
            RuntimeOperation::FilesWrite {
                target,
                mode: FileWriteMode::Create | FileWriteMode::Replace,
                ..
            }
            | RuntimeOperation::FilesEdit { target, .. }
            | RuntimeOperation::FilesCopy { target, .. } => Some(target),
            RuntimeOperation::TransfersUpload { remote, .. } => Some(remote),
            _ => None,
        };
        let recovery_paths = if let Some(target) = target {
            vec![operation.staging_path(&normalize_path(
                "/",
                if target.path.is_empty() {
                    "."
                } else {
                    &target.path
                },
            )?)]
        } else {
            vec![]
        };
        Ok(FileExecutionReport {
            commit: FileCommitState::Unchanged,
            error: None,
            json: None,
            recovery_paths,
            bytes: 0,
            hash: None,
        })
    }
    pub(super) async fn run_files(
        &self,
        mut task: ExecutionTask,
        access: ExecutionAccess,
        operation: CheckedFileOperation,
        io: FileTransferHandles,
        cancel: CancellationToken,
        remaining: Option<Duration>,
    ) -> AgentResult<()> {
        task.state.phase = ExecutionPhase::Running;
        self.save(&mut task).await?;
        let control = FileExecutionControl::default();
        let work = access
            .connection
            .execute_files(operation, io, control.clone());
        tokio::pin!(work);
        let timer = async {
            if let Some(d) = remaining {
                tokio::time::sleep(d).await
            } else {
                std::future::pending::<()>().await
            }
        };
        tokio::pin!(timer);
        let grace = tokio::time::sleep(Duration::from_secs(40));
        tokio::pin!(grace);
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let mut reason = None;
        let mut last = 0;
        let mut activity = tokio::time::Instant::now();
        let mut recorded = activity;
        let report = loop {
            tokio::select! {biased;
                result=&mut work=>break result.ok(),
                _=cancel.cancelled(),if reason.is_none()=>{reason=Some(ExecutionStopReason::Cancelled);},
                _=self.shutdown.cancelled(),if reason.is_none()=>{reason=Some(ExecutionStopReason::Cancelled);},
                _=&mut timer,if reason.is_none()=>{reason=Some(ExecutionStopReason::TimedOut);},
                _=&mut grace,if reason.is_some()=>break None,
                _=tick.tick()=>{
                    let bytes=control.bytes.load(Ordering::Acquire);
                    if bytes!=last {last=bytes;activity=tokio::time::Instant::now();}
                    if task.request.operation.is_transfer() && recorded.elapsed()>=Duration::from_secs(1){
                        recorded=tokio::time::Instant::now();
                        if self.transfer_progress(&task,bytes,control.total.load(Ordering::Acquire),None).await.is_err(){task.output_incomplete=true;reason=Some(ExecutionStopReason::Disconnected);}
                    }
                    if task.request.operation.is_transfer() && task.policy.execution.limits.transfer_stall_seconds.is_some_and(|s|activity.elapsed()>=Duration::from_secs(s.into())) && reason.is_none(){reason=Some(ExecutionStopReason::TimedOut);}
                }
            }
            if reason.is_some() && !control.cancel.swap(true, Ordering::AcqRel) {
                task.state.phase = ExecutionPhase::Stopping;
                self.save(&mut task).await?;
                grace
                    .as_mut()
                    .reset(tokio::time::Instant::now() + Duration::from_secs(35));
            }
        };
        let outcome = if let Some(report) = report {
            task.error = report.error.clone();
            let status = match report.commit {
                FileCommitState::Unknown => ExecutionOutcome::Unknown {
                    reason: reason.unwrap_or(ExecutionStopReason::Disconnected),
                },
                FileCommitState::Partial => ExecutionOutcome::Partial {
                    reason: reason.unwrap_or(ExecutionStopReason::Disconnected),
                    termination_confirmed: true,
                },
                FileCommitState::Unchanged if report.error.as_deref() == Some("cancelled") => {
                    ExecutionOutcome::Stopped {
                        reason: reason.unwrap_or(ExecutionStopReason::Cancelled),
                    }
                }
                FileCommitState::Unchanged | FileCommitState::Committed => {
                    ExecutionOutcome::Completed {
                        exit_code: u32::from(report.error.is_some()),
                    }
                }
            };
            if task.request.operation.is_transfer()
                && self
                    .transfer_progress(
                        &task,
                        report.bytes,
                        control.total.load(Ordering::Acquire),
                        report.hash.clone(),
                    )
                    .await
                    .is_err()
            {
                task.output_incomplete = true;
            }
            task.file_report = Some(report);
            status
        } else {
            if let Some(report) = task.file_report.as_mut() {
                report.commit = FileCommitState::Unknown;
                report.error = Some("file_result_unknown".into());
            }
            ExecutionOutcome::Unknown {
                reason: reason.unwrap_or(ExecutionStopReason::Disconnected),
            }
        };
        self.finish(&mut task, outcome).await
    }
    async fn transfer_progress(
        &self,
        task: &ExecutionTask,
        bytes: u64,
        total: u64,
        hash: Option<String>,
    ) -> AgentResult<()> {
        let record = ActivityRecord {
            session_id: task.context_id.clone(),
            task_id: Some(task.id.clone()),
            parent_task_id: None,
            sequence: 0,
            timestamp_ms: now(),
            actor: ActivityActor::Agent,
            policy_revision: task.policy.revision,
            body: ActivityBody::Transfer {
                bytes,
                total: (total != u64::MAX).then_some(total),
                hash,
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
            Ok(DbReply::Agent(AgentDbReply::Recorded(_))) => Ok(()),
            _ => Err("activity_recording_unavailable"),
        }
    }
}
