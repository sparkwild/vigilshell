use super::*;
impl AgentService {
    pub(super) async fn save_screen(
        &self,
        id: &str,
        checkpoint: ScreenCheckpoint,
    ) -> AgentResult<AgentReply> {
        if checkpoint.version != 1
            || checkpoint.sequence == 0
            || checkpoint.sequence > i64::MAX as u64
            || !(1..=500).contains(&checkpoint.columns)
            || !(1..=300).contains(&checkpoint.rows)
            || checkpoint.gzip.len() > 1024 * 1024
        {
            return Err("invalid_screen_checkpoint");
        }
        use base64::Engine;
        if base64::engine::general_purpose::STANDARD
            .decode(&checkpoint.gzip)
            .is_err()
        {
            return Err("invalid_screen_checkpoint");
        }
        let task = self.get_task(id).await?;
        if self
            .state
            .lock()
            .unwrap()
            .contexts
            .get(&task.context_id)
            .is_none_or(|context| context.cancel.is_cancelled())
        {
            return Ok(AgentReply::Done {});
        }
        if !task.phase.active() {
            return Ok(AgentReply::Done {});
        }
        let AgentReply::Activity { page } = self
            .activity(
                task.context_id.clone(),
                Some(task.id.clone()),
                checkpoint.sequence - 1,
                true,
            )
            .await?
        else {
            return Err("activity_recording_unavailable");
        };
        if page
            .records
            .first()
            .is_none_or(|record| record.sequence != checkpoint.sequence)
        {
            return Err("invalid_screen_checkpoint");
        }
        let record = ActivityRecord {
            session_id: task.context_id,
            task_id: Some(task.id),
            parent_task_id: task.parent_task_id,
            sequence: 0,
            timestamp_ms: now(),
            actor: ActivityActor::System,
            policy_revision: task.policy_revision,
            body: ActivityBody::Screen { checkpoint },
        };
        let limit_bytes =
            u64::from(self.state.lock().unwrap().config.settings.storage_mib) * 1024 * 1024;
        if self
            .db(AgentDbCommand::recording(RecordingCommand::AppendScreen {
                record,
                limit_bytes,
            }))
            .await
            .is_err()
        {
            return Err("screen_cache_unavailable");
        }
        Ok(AgentReply::Done {})
    }
    pub(super) async fn load_screen(&self, id: &str) -> AgentResult<AgentReply> {
        let task = self.get_task(id).await?;
        match self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Screen {
                session_id: task.context_id,
                task_id: task.id,
            }))
            .await
        {
            Ok(DbReply::AgentHistory(AgentHistoryReply::Screen(checkpoint))) => {
                Ok(AgentReply::Screen { checkpoint })
            }
            _ => Err("activity_recording_unavailable"),
        }
    }

    pub(super) async fn history_recordings(
        &self,
        filter: RecordingFilter,
    ) -> AgentResult<AgentReply> {
        match self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Recordings {
                filter,
            }))
            .await
        {
            Ok(DbReply::AgentHistory(AgentHistoryReply::Recordings(page))) => {
                Ok(AgentReply::Recordings {
                    has_more: page.next_offset.is_some(),
                    sessions: page
                        .sessions
                        .into_iter()
                        .map(|s| RecordedSession {
                            id: s.id,
                            connection_id: s.connection_id,
                            entry_id: s.entry_id,
                            connection_name: s.connection_name.unwrap_or_default(),
                            entry_name: s.entry_name.unwrap_or_default(),
                            created_at: s.created_at,
                            ended_at: s.ended_at,
                            result: s.result,
                            incomplete: s.incomplete,
                            last_sequence: s.last_sequence,
                            bytes: s.bytes,
                        })
                        .collect(),
                })
            }
            _ => Err("activity_recording_unavailable"),
        }
    }

    pub(super) async fn recording_failed(&self) {
        let first = {
            let mut state = self.state.lock().unwrap();
            let first = !state.recording_blocked;
            state.recording_blocked = true;
            for context in state.contexts.values() {
                context.cancel.cancel();
            }
            first
        };
        self.executor.cancel_scope(None, None, None).await;
        if first {
            self.event(AgentEvent::Configuration {}).await;
        }
    }
    pub(super) async fn record_policy(
        &self,
        context: &Arc<Context>,
        policy: &ConnectionPolicy,
    ) -> AgentResult<()> {
        if context
            .recorded_policies
            .lock()
            .unwrap()
            .contains(&policy.revision)
        {
            return Ok(());
        }
        let id = context.view.lock().unwrap().id.clone();
        self.record(
            &id,
            None,
            ActivityActor::System,
            policy.revision,
            ActivityBody::Policy {
                policy: Box::new(policy.execution.clone()),
            },
        )
        .await?;
        context
            .recorded_policies
            .lock()
            .unwrap()
            .insert(policy.revision);
        Ok(())
    }
    pub(super) async fn publish_context(&self, context: AgentContext) -> AgentResult<()> {
        if self
            .state
            .lock()
            .unwrap()
            .contexts
            .get(&context.id)
            .is_some_and(|current| current.view.lock().unwrap().revision > context.revision)
        {
            return Ok(());
        }
        let recorded = self
            .record(
                &context.id,
                None,
                ActivityActor::System,
                self.policy(&context.connection_id).revision,
                ActivityBody::Session {
                    phase: context.phase.clone(),
                    connection_id: context.connection_id.clone(),
                    entry_id: context.entry_id.clone(),
                    entry_name: context.entry_name.clone(),
                },
            )
            .await;
        self.event(AgentEvent::Context {
            context: context.clone(),
        })
        .await;
        if context.phase == ContextPhase::Failed {
            if let Some(active) = self.state.lock().unwrap().contexts.get(&context.id) {
                active.cancel.cancel();
            }
            let _ = self
                .db(AgentDbCommand::recording(RecordingCommand::End {
                    session_id: context.id,
                    ended_at: now(),
                    result: "failed".into(),
                    incomplete: recorded.is_err(),
                }))
                .await;
        }
        recorded.map(|_| ())
    }
    pub(super) async fn create_recording(
        &self,
        context: &AgentContext,
        entry: &AccessEntry,
    ) -> AgentResult<()> {
        self.db(AgentDbCommand::recording(RecordingCommand::Create {
            session: RecordedSession {
                id: context.id.clone(),
                connection_id: context.connection_id.clone(),
                connection_name: context.connection_name.clone(),
                entry_id: entry.id.clone(),
                entry_name: entry.name.clone(),
                created_at: context.created_at,
                ended_at: None,
                result: None,
                incomplete: false,
                last_sequence: 0,
                bytes: 0,
            },
        }))
        .await?;
        Ok(())
    }
    pub(super) async fn record(
        &self,
        session_id: &str,
        task_id: Option<String>,
        actor: ActivityActor,
        policy_revision: u64,
        body: ActivityBody,
    ) -> AgentResult<ActivityRecord> {
        let parent = task_id.as_ref().and_then(|id| {
            self.state
                .lock()
                .unwrap()
                .tasks
                .get(id)
                .and_then(|task| task.parent_task_id.clone())
        });
        self.record_linked(session_id, task_id, parent, actor, policy_revision, body)
            .await
    }
    async fn record_linked(
        &self,
        session_id: &str,
        task_id: Option<String>,
        parent_task_id: Option<String>,
        actor: ActivityActor,
        policy_revision: u64,
        body: ActivityBody,
    ) -> AgentResult<ActivityRecord> {
        let limit_bytes =
            u64::from(self.state.lock().unwrap().config.settings.storage_mib) * 1024 * 1024;
        let record = ActivityRecord {
            session_id: session_id.into(),
            task_id,
            parent_task_id,
            sequence: 0,
            timestamp_ms: now(),
            actor,
            policy_revision,
            body,
        };
        let result = self
            .db(AgentDbCommand::recording(RecordingCommand::Append {
                record,
                limit_bytes,
            }))
            .await;
        match result {
            Ok(AgentDbReply::Recorded(record)) => {
                self.event(AgentEvent::Activity {
                    record: record.clone(),
                })
                .await;
                Ok(record)
            }
            _ => {
                // 已有记录缺口时停止该会话的接纳和执行，不能继续产生看似完整的后续记录。
                self.recording_failed().await;
                Err("activity_recording_unavailable")
            }
        }
    }
    pub(super) async fn record_task(&self, task: &AgentTask) -> AgentResult<()> {
        let targets = task.targets.clone();
        self.record_linked(
            &task.context_id,
            Some(task.id.clone()),
            task.parent_task_id.clone(),
            ActivityActor::Agent,
            task.policy_revision,
            ActivityBody::Operation {
                tool: task.tool.clone(),
                phase: task.phase.clone(),
                command: matches!(task.phase, TaskPhase::Queued | TaskPhase::Denied)
                    .then(|| task.command.clone()),
                targets,
                error: task.error.clone(),
                exit_code: task.exit_code,
            },
        )
        .await?;
        Ok(())
    }
    pub(super) async fn activity(
        &self,
        session_id: String,
        task_id: Option<String>,
        cursor: u64,
        include_output: bool,
    ) -> AgentResult<AgentReply> {
        if cursor > vigilshell_domain::MAX_REVISION {
            return Err("invalid_cursor");
        }
        match self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Activity {
                session_id,
                task_id,
                cursor,
                include_output,
            }))
            .await
            .map_err(|_| "activity_recording_unavailable")?
        {
            DbReply::AgentHistory(AgentHistoryReply::Activity(history)) => {
                let records = history
                    .records
                    .into_iter()
                    .map(|record| {
                        serde_json::from_str::<ActivityRecord>(&record.raw_json).unwrap_or_else(
                            |_| ActivityRecord {
                                session_id: record.session_id,
                                task_id: record.task_id,
                                parent_task_id: record.parent_task_id,
                                sequence: record.sequence,
                                timestamp_ms: record.timestamp_ms,
                                actor: ActivityActor::System,
                                policy_revision: record.policy_revision.unwrap_or(0),
                                body: ActivityBody::Historical {
                                    original_kind: record.kind,
                                    raw: record.raw_json,
                                },
                            },
                        )
                    })
                    .collect();
                Ok(AgentReply::Activity {
                    page: ActivityPage {
                        records,
                        next_cursor: history.next_cursor,
                        incomplete: history.incomplete,
                    },
                })
            }
            _ => Err("activity_recording_unavailable"),
        }
    }
}
