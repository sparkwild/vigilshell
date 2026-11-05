//! 审阅、分派与结果事实独立记录；重启不能从缺少响应推断未执行。
use super::execution::{
    deserialize_identity, deserialize_revision, validate_identity, validate_revision,
};
use super::execution_policy::validate_absolute_path;
use super::*;
use serde::de::Error;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(extend("x-vigil-rule" = "scriptReviewBinding")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptReviewBinding {
    pub identity: ExecutionIdentity,
    pub snapshot: ScriptSnapshot,
}

impl<'de> Deserialize<'de> for ScriptReviewBinding {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Fields {
            identity: ExecutionIdentity,
            snapshot: ScriptSnapshot,
        }
        let fields = Fields::deserialize(decoder)?;
        let binding = Self {
            identity: fields.identity,
            snapshot: fields.snapshot,
        };
        binding.validate().map_err(D::Error::custom)?;
        Ok(binding)
    }
}

impl ScriptReviewBinding {
    pub fn validate(&self) -> AgentResult<()> {
        self.identity.validate()?;
        validate_absolute_path(&self.snapshot.path)?;
        validate_absolute_path(&self.snapshot.directory)?;
        if self.snapshot.text.trim().is_empty()
            || self.snapshot.text.len() > SCRIPT_LIMIT
            || self.snapshot.text.contains('\0')
            || self.snapshot.arguments.len() > 128
            || self
                .snapshot
                .arguments
                .iter()
                .any(|value| value.contains('\0'))
            || self
                .snapshot
                .arguments
                .iter()
                .map(String::len)
                .sum::<usize>()
                > 64 * 1024
            || self
                .snapshot
                .source_version
                .as_ref()
                .is_some_and(|v| !valid_content_hash(v))
            || self.snapshot.hash != format!("{:x}", Sha256::digest(self.snapshot.text.as_bytes()))
        {
            return Err("invalid_script_review_binding");
        }
        Ok(())
    }

    pub fn matches_request(&self, request: &ExecutionRequest) -> AgentResult<()> {
        self.validate()?;
        request.validate()?;
        let RuntimeOperation::Script {
            source,
            interpreter,
            arguments,
            directory,
        } = &request.operation
        else {
            return Err("script_review_request_mismatch");
        };
        let directory = normalize_path(
            "/",
            if directory.path.is_empty() {
                "."
            } else {
                &directory.path
            },
        )?;
        if self.identity != request.identity
            || self.snapshot.interpreter != *interpreter
            || self.snapshot.arguments != *arguments
            || self.snapshot.directory != directory
        {
            return Err("script_review_request_mismatch");
        }
        match source {
            ScriptSource::Inline { text }
                if *text != self.snapshot.text || self.snapshot.source_version.is_some() =>
            {
                Err("script_review_request_mismatch")
            }
            ScriptSource::RemoteFile {
                target,
                expected_hash,
            } => {
                let path = normalize_path(
                    "/",
                    if target.path.is_empty() {
                        "."
                    } else {
                        &target.path
                    },
                )?;
                if self.snapshot.path != path
                    || expected_hash
                        .as_ref()
                        .is_some_and(|hash| *hash != self.snapshot.hash)
                {
                    Err("script_review_request_mismatch")
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    fn same_snapshot(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.snapshot.hash == other.snapshot.hash
            && self.snapshot.source_version == other.snapshot.source_version
            && self.snapshot.path == other.snapshot.path
            && self.snapshot.directory == other.snapshot.directory
            && self.snapshot.interpreter == other.snapshot.interpreter
            && self.snapshot.arguments == other.snapshot.arguments
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScriptReviewState {
    NotRequired {},
    WaitingReview { binding: ScriptReviewBinding },
    Approved { binding: ScriptReviewBinding },
    Rejected { binding: ScriptReviewBinding },
}

impl ScriptReviewState {
    fn permits_dispatch(&self) -> bool {
        matches!(self, Self::NotRequired {} | Self::Approved { .. })
    }
    fn validate(&self) -> AgentResult<()> {
        match self {
            Self::NotRequired {} => Ok(()),
            Self::WaitingReview { binding }
            | Self::Approved { binding }
            | Self::Rejected { binding } => binding.validate(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DispatchState {
    NotDispatched {},
    Attempted {
        #[serde(deserialize_with = "deserialize_identity")]
        #[cfg_attr(
            feature = "schema",
            schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_-]+$"))
        )]
        attempt_id: String,
        #[serde(deserialize_with = "deserialize_revision")]
        #[cfg_attr(
            feature = "schema",
            schemars(range(min = 0, max = 9_007_199_254_740_991_u64))
        )]
        attempted_at: u64,
    },
}

impl DispatchState {
    fn validate(&self) -> AgentResult<()> {
        if let Self::Attempted {
            attempt_id,
            attempted_at,
        } = self
        {
            validate_identity(attempt_id)?;
            validate_revision(*attempted_at)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPhase {
    WaitingReview,
    Queued,
    Dispatching,
    Running,
    Stopping,
    Settled,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(extend("x-vigil-rule" = "executionState")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionState {
    pub phase: ExecutionPhase,
    pub review: ScriptReviewState,
    pub dispatch: DispatchState,
    pub outcome: Option<ExecutionOutcome>,
    pub terminal: bool,
}

impl<'de> Deserialize<'de> for ExecutionState {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Fields {
            phase: ExecutionPhase,
            review: ScriptReviewState,
            dispatch: DispatchState,
            outcome: Option<ExecutionOutcome>,
            terminal: bool,
        }
        let fields = Fields::deserialize(decoder)?;
        let state = Self {
            phase: fields.phase,
            review: fields.review,
            dispatch: fields.dispatch,
            outcome: fields.outcome,
            terminal: fields.terminal,
        };
        state.validate().map_err(D::Error::custom)?;
        Ok(state)
    }
}

impl ExecutionState {
    pub fn initial_denial(&self) -> bool {
        self.phase == ExecutionPhase::Settled
            && matches!(self.dispatch, DispatchState::NotDispatched {})
            && matches!(
                self.outcome,
                Some(ExecutionOutcome::NotExecuted {
                    reason: NotExecutedReason::Denied
                })
            )
    }

    /// 校验状态与当前请求、权限版本及脚本快照的一致性，不替代命令检查或远端能力检查。
    pub fn validate_for(
        &self,
        request: &ExecutionRequest,
        policy: &ConnectionPolicy,
    ) -> AgentResult<()> {
        self.validate()?;
        request.validate()?;
        policy.validate()?;
        if request.identity.connection_id != policy.connection_id
            || request.identity.connection_revision != policy.connection_revision
            || request.identity.policy_revision != policy.revision
        {
            return Err("execution_policy_mismatch");
        }
        if self.terminal != request.operation.is_terminal() {
            return Err("execution_state_request_mismatch");
        }
        if self.initial_denial() {
            return Ok(());
        }
        let authorization = request.authorize(policy)?;
        if self.terminal != request.operation.is_terminal()
            || authorization.script_review_required
                && matches!(self.review, ScriptReviewState::NotRequired {})
        {
            return Err("execution_state_request_mismatch");
        }
        match &self.review {
            ScriptReviewState::NotRequired {} => Ok(()),
            ScriptReviewState::WaitingReview { binding }
            | ScriptReviewState::Approved { binding }
            | ScriptReviewState::Rejected { binding } => binding.matches_request(request),
        }
    }

    pub fn validate(&self) -> AgentResult<()> {
        self.review.validate()?;
        self.dispatch.validate()?;
        let attempted = matches!(self.dispatch, DispatchState::Attempted { .. });
        if self.terminal && !matches!(self.review, ScriptReviewState::NotRequired {}) {
            return Err("invalid_execution_state");
        }
        let valid = match self.phase {
            ExecutionPhase::WaitingReview => {
                !attempted
                    && self.outcome.is_none()
                    && matches!(self.review, ScriptReviewState::WaitingReview { .. })
            }
            ExecutionPhase::Queued => {
                !attempted && self.outcome.is_none() && self.review.permits_dispatch()
            }
            ExecutionPhase::Dispatching | ExecutionPhase::Running | ExecutionPhase::Stopping => {
                attempted && self.outcome.is_none() && self.review.permits_dispatch()
            }
            ExecutionPhase::Settled => match &self.outcome {
                Some(ExecutionOutcome::NotExecuted { reason }) => {
                    !attempted
                        && (!matches!(self.review, ScriptReviewState::Rejected { .. })
                            || *reason == NotExecutedReason::Denied)
                }
                Some(_) => attempted && self.review.permits_dispatch(),
                None => false,
            },
        };
        if valid {
            Ok(())
        } else {
            Err("invalid_execution_state")
        }
    }

    pub fn active(&self) -> bool {
        self.validate().is_ok() && self.phase != ExecutionPhase::Settled
    }

    /// 只由可信桌面在身份验证成功后调用；MCP 请求不能提交批准或此状态转换。
    pub fn approve_verified(&mut self, verified: &ScriptReviewBinding) -> AgentResult<()> {
        self.validate()?;
        verified.validate()?;
        let ScriptReviewState::WaitingReview { binding } = &self.review else {
            return Err("review_not_waiting");
        };
        if self.phase != ExecutionPhase::WaitingReview || !binding.same_snapshot(verified) {
            return Err("script_review_binding_changed");
        }
        self.review = ScriptReviewState::Approved {
            binding: binding.clone(),
        };
        self.phase = ExecutionPhase::Queued;
        Ok(())
    }

    pub fn reject_review(&mut self) -> AgentResult<()> {
        self.validate()?;
        let ScriptReviewState::WaitingReview { binding } = &self.review else {
            return Err("review_not_waiting");
        };
        if self.phase != ExecutionPhase::WaitingReview {
            return Err("review_not_waiting");
        }
        self.review = ScriptReviewState::Rejected {
            binding: binding.clone(),
        };
        self.phase = ExecutionPhase::Settled;
        self.outcome = Some(ExecutionOutcome::NotExecuted {
            reason: NotExecutedReason::Denied,
        });
        Ok(())
    }

    /// 调用者须先完成当前请求的权限、命令和目标检查，再持久保存此状态，才可发送 SSH 请求。
    /// 此方法只记录尝试边界，不把状态本身作为命令检查或启动凭据。
    pub fn begin_dispatch(&mut self, attempt_id: String, attempted_at: u64) -> AgentResult<()> {
        self.validate()?;
        validate_identity(&attempt_id)?;
        validate_revision(attempted_at)?;
        if self.phase != ExecutionPhase::Queued {
            return Err("execution_not_queued");
        }
        self.dispatch = DispatchState::Attempted {
            attempt_id,
            attempted_at,
        };
        self.phase = ExecutionPhase::Dispatching;
        Ok(())
    }

    pub fn settle(&mut self, outcome: ExecutionOutcome) -> AgentResult<()> {
        self.validate()?;
        if self.phase == ExecutionPhase::Settled {
            return Err("execution_already_settled");
        }
        let mut next = self.clone();
        next.phase = ExecutionPhase::Settled;
        next.outcome = Some(outcome);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn recover_after_restart(&self) -> AgentResult<Self> {
        self.validate()?;
        let mut recovered = self.clone();
        if recovered.phase != ExecutionPhase::Settled {
            recovered.phase = ExecutionPhase::Settled;
            recovered.outcome = Some(
                if matches!(self.dispatch, DispatchState::NotDispatched {}) {
                    ExecutionOutcome::NotExecuted {
                        reason: NotExecutedReason::Restarted,
                    }
                } else {
                    ExecutionOutcome::Unknown {
                        reason: ExecutionStopReason::Restarted,
                    }
                },
            );
        }
        // 已分派的批准留作历史证据；未分派批准失效，任何恢复结果都不会重新排队。
        if matches!(recovered.dispatch, DispatchState::NotDispatched {})
            && let ScriptReviewState::Approved { binding } = &recovered.review
        {
            recovered.review = ScriptReviewState::WaitingReview {
                binding: binding.clone(),
            };
        }
        recovered.validate()?;
        Ok(recovered)
    }
}

#[cfg(test)]
#[path = "execution_state_tests.rs"]
mod tests;
