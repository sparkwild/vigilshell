use super::*;
use crate::workspace::WorkspaceError;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionTask {
    pub id: String,
    pub entry_id: String,
    pub connection_id: String,
    pub context_id: String,
    pub created_at: u64,
    #[serde(default)]
    pub finished_at: Option<u64>,
    pub revision: u64,
    pub request: ExecutionRequest,
    #[serde(default)]
    pub source_request: Option<ExecutionRequest>,
    pub policy: ConnectionPolicy,
    pub state: ExecutionState,
    #[serde(default)]
    pub phase: Option<TaskPhase>,
    pub error: Option<String>,
    pub output_incomplete: bool,
    #[serde(default)]
    pub file_report: Option<FileExecutionReport>,
    #[serde(default)]
    pub native_report: Option<NativeExecutionReport>,
    #[serde(default)]
    pub script_snapshot: Option<ScriptSnapshot>,
    #[serde(default)]
    pub script_staging_path: Option<String>,
}
impl ExecutionTask {
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if let Some(source) = &self.source_request {
            source
                .validate()
                .map_err(|_| WorkspaceError::InvalidInput)?;
            if source.identity != self.request.identity
                || source.request_key != self.request.request_key
                || source.timeout_seconds != self.request.timeout_seconds
                || !matches!(source.operation, RuntimeOperation::Command { .. })
                || !matches!(self.request.operation, RuntimeOperation::Script { .. })
            {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        if let Some(snapshot) = &self.script_snapshot {
            if let ScriptReviewState::WaitingReview { binding }
            | ScriptReviewState::Approved { binding }
            | ScriptReviewState::Rejected { binding } = &self.state.review
                && binding.snapshot != *snapshot
            {
                return Err(WorkspaceError::InvalidInput);
            }
            ScriptReviewBinding {
                identity: self.request.identity.clone(),
                snapshot: snapshot.clone(),
            }
            .matches_request(&self.request)
            .map_err(|_| WorkspaceError::InvalidInput)?;
        }
        match (
            &self.request.operation,
            &self.script_snapshot,
            &self.script_staging_path,
        ) {
            (RuntimeOperation::Script { source, .. }, Some(snapshot), staging) => {
                check_script(&self.request, &self.policy)
                    .and_then(|c| c.bind(snapshot.clone()))
                    .map_err(|_| WorkspaceError::InvalidInput)?;
                let valid = match (source, staging) {
                    (ScriptSource::RemoteFile { .. }, None) => true,
                    (ScriptSource::Inline { .. }, Some(path)) => {
                        normalize_path("/", path).ok().as_ref() == Some(path)
                            && *path == snapshot.path
                            && path.ends_with(&format!(
                                "/.local/share/vigilshell/script-staging/{}/entry",
                                self.id
                            ))
                    }
                    _ => false,
                };
                if !valid {
                    return Err(WorkspaceError::InvalidInput);
                }
            }
            (RuntimeOperation::Script { .. }, None, None) if self.state.initial_denial() => {}
            (RuntimeOperation::Script { .. }, None, None)
                if !self.policy.execution.script_review
                    && matches!(self.state.review, ScriptReviewState::NotRequired {}) => {}
            (RuntimeOperation::Script { .. }, _, _) => return Err(WorkspaceError::InvalidInput),
            (_, None, None) => {}
            _ => return Err(WorkspaceError::InvalidInput),
        }
        self.state
            .validate_for(&self.request, &self.policy)
            .map_err(|_| WorkspaceError::InvalidInput)?;
        if self.id != self.request.identity.task_id
            || self.entry_id != self.request.identity.entry_id
            || self.connection_id != self.request.identity.connection_id
            || self.context_id != self.request.identity.session_id
            || self.created_at > crate::MAX_REVISION
            || self.revision > crate::MAX_REVISION
            || !(matches!(
                self.request.operation,
                RuntimeOperation::Command { .. }
                    | RuntimeOperation::Script { .. }
                    | RuntimeOperation::Terminal { .. }
            ) || self.request.operation.is_file_operation()
                || self.request.operation.is_native_operation())
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(())
    }
}

impl ExecutionTask {
    pub fn view_phase(&self) -> TaskPhase {
        match self.state.phase {
            ExecutionPhase::WaitingReview => TaskPhase::WaitingReview,
            ExecutionPhase::Queued | ExecutionPhase::Dispatching => TaskPhase::Queued,
            ExecutionPhase::Running => TaskPhase::Running,
            ExecutionPhase::Stopping => TaskPhase::Stopping,
            ExecutionPhase::Settled => match &self.state.outcome {
                Some(ExecutionOutcome::Completed { exit_code: 0 }) => TaskPhase::Succeeded,
                Some(ExecutionOutcome::Completed { .. }) => TaskPhase::Failed,
                Some(ExecutionOutcome::RequestAccepted {}) => TaskPhase::RequestAccepted,
                Some(ExecutionOutcome::NotExecuted {
                    reason: NotExecutedReason::Denied,
                }) => TaskPhase::Denied,
                Some(ExecutionOutcome::NotExecuted {
                    reason: NotExecutedReason::Cancelled,
                }) => TaskPhase::Cancelled,
                Some(ExecutionOutcome::NotExecuted {
                    reason: NotExecutedReason::TimedOut,
                }) => TaskPhase::TimedOut,
                Some(ExecutionOutcome::NotExecuted { .. }) => TaskPhase::Failed,
                Some(
                    ExecutionOutcome::Partial {
                        reason: ExecutionStopReason::Cancelled,
                        ..
                    }
                    | ExecutionOutcome::Stopped {
                        reason: ExecutionStopReason::Cancelled,
                    },
                ) => TaskPhase::Cancelled,
                Some(
                    ExecutionOutcome::Partial {
                        reason: ExecutionStopReason::TimedOut,
                        ..
                    }
                    | ExecutionOutcome::Stopped {
                        reason: ExecutionStopReason::TimedOut,
                    },
                ) => TaskPhase::TimedOut,
                _ => TaskPhase::InterruptedUnknown,
            },
        }
    }
}
