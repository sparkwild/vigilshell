use super::*;
impl LocalExecutor {
    pub async fn approve_script(
        self: &Arc<Self>,
        entry: &str,
        id: &str,
        verified: ScriptReviewBinding,
    ) -> AgentResult<ExecutionTask> {
        let _admission = self.admissions.lock().await;
        let mut task = self.get(entry, id).await?;
        let current = self
            .authority
            .current(task.request.identity.clone())
            .await
            .map_err(|_| "access_revoked")?;
        if current.policy != task.policy || current.connection.is_closed() {
            return Err("access_revoked");
        }
        verified.matches_request(&task.request)?;
        task.state.approve_verified(&verified)?;
        self.script_decision(&task, true).await?;
        self.save(&mut task).await?;
        let cancel = self
            .active
            .lock()
            .unwrap()
            .get(id)
            .map(|v| v.cancel.clone())
            .ok_or("execution_not_active")?;
        self.reviews_changed.send_modify(|v| *v = v.wrapping_add(1));
        self.spawn_run(task.clone(), cancel);
        Ok(task)
    }
    pub async fn reject_script(&self, entry: &str, id: &str) -> AgentResult<ExecutionTask> {
        let _admission = self.admissions.lock().await;
        let mut task = self.get(entry, id).await?;
        task.state.reject_review()?;
        let recorded = self.script_decision(&task, false).await;
        self.save(&mut task).await?;
        recorded?;
        self.reviews_changed.send_modify(|v| *v = v.wrapping_add(1));
        Ok(task)
    }
    async fn script_decision(&self, task: &ExecutionTask, allowed: bool) -> AgentResult<()> {
        let record = ActivityRecord {
            session_id: task.context_id.clone(),
            task_id: Some(task.id.clone()),
            parent_task_id: None,
            sequence: 0,
            timestamp_ms: now(),
            actor: ActivityActor::User,
            policy_revision: task.policy.revision,
            body: ActivityBody::Decision {
                allowed,
                reason: Some(
                    if allowed {
                        "script_approved"
                    } else {
                        "script_rejected"
                    }
                    .into(),
                ),
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
    pub(super) fn watch_review(self: &Arc<Self>, task: ExecutionTask, cancel: CancellationToken) {
        let service = self.clone();
        let mut changed = self.reviews_changed.subscribe();
        tokio::spawn(async move {
            loop {
                let waiting = service
                    .get(&task.entry_id, &task.id)
                    .await
                    .is_ok_and(|t| t.state.phase == ExecutionPhase::WaitingReview);
                if !waiting {
                    break;
                }
                tokio::select! {_=cancel.cancelled()=>{},_=service.shutdown.cancelled()=>{},_=changed.changed()=>continue}
                let _guard = service.admissions.lock().await;
                if let Ok(mut current) = service.get(&task.entry_id, &task.id).await
                    && current.state.phase == ExecutionPhase::WaitingReview
                    && let Err(error) = service
                        .finish(
                            &mut current,
                            ExecutionOutcome::NotExecuted {
                                reason: NotExecutedReason::Cancelled,
                            },
                        )
                        .await
                {
                    service
                        .failed
                        .lock()
                        .unwrap()
                        .insert(task.id.clone(), error);
                }
                break;
            }
            // Approval hands the same cancellation token to the running task.
            if service
                .get(&task.entry_id, &task.id)
                .await
                .is_ok_and(|t| t.state.phase == ExecutionPhase::Settled)
            {
                service.active.lock().unwrap().remove(&task.id);
            }
        });
    }
}
