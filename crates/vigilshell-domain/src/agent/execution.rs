//! 本地业务请求与结果；这些合同不构成远端启动凭据，也不接受调用者声明的批准。
use super::*;
use serde::de::Error;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionIdentity {
    #[serde(deserialize_with = "deserialize_identity")]
    #[cfg_attr(
        feature = "schema",
        schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_-]+$"))
    )]
    pub task_id: String,
    #[serde(deserialize_with = "deserialize_identity")]
    #[cfg_attr(
        feature = "schema",
        schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_-]+$"))
    )]
    pub entry_id: String,
    #[serde(deserialize_with = "deserialize_generation")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = 1, max = 9_007_199_254_740_991_u64))
    )]
    pub entry_generation: u64,
    #[serde(deserialize_with = "deserialize_identity")]
    #[cfg_attr(
        feature = "schema",
        schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_-]+$"))
    )]
    pub connection_id: String,
    #[serde(deserialize_with = "deserialize_revision")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = 0, max = 9_007_199_254_740_991_u64))
    )]
    pub connection_revision: u64,
    #[serde(deserialize_with = "deserialize_identity")]
    #[cfg_attr(
        feature = "schema",
        schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_-]+$"))
    )]
    pub session_id: String,
    #[serde(deserialize_with = "deserialize_revision")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = 0, max = 9_007_199_254_740_991_u64))
    )]
    pub policy_revision: u64,
}

impl ExecutionIdentity {
    pub fn validate(&self) -> AgentResult<()> {
        for id in [
            &self.task_id,
            &self.entry_id,
            &self.connection_id,
            &self.session_id,
        ] {
            validate_identity(id)?;
        }
        validate_generation(self.entry_generation)?;
        for revision in [self.connection_revision, self.policy_revision] {
            validate_revision(revision)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(extend("x-vigil-rule" = "executionRequest")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionRequest {
    pub identity: ExecutionIdentity,
    #[serde(deserialize_with = "deserialize_request_key")]
    #[cfg_attr(
        feature = "schema",
        schemars(
            length(min = 1, max = 128),
            regex(pattern = r"^[^\u0000-\u001f\u007f-\u009f]+$")
        )
    )]
    pub request_key: String,
    #[serde(default, deserialize_with = "deserialize_timeout")]
    #[cfg_attr(feature = "schema", schemars(range(min = 1)))]
    pub timeout_seconds: Option<u32>,
    #[serde(deserialize_with = "deserialize_operation")]
    pub operation: RuntimeOperation,
}

impl ExecutionRequest {
    pub fn validate(&self) -> AgentResult<()> {
        self.identity.validate()?;
        validate_request_key(&self.request_key)?;
        if self.timeout_seconds == Some(0) {
            return Err("invalid_timeout");
        }
        self.operation.validate_business()
    }

    /// 判定业务许可及尚需完成的检查；返回值不是可提交给执行器的启动凭证。
    pub fn authorize(&self, policy: &ConnectionPolicy) -> AgentResult<ExecutionAuthorization> {
        self.validate()?;
        policy.validate()?;
        if self.identity.connection_id != policy.connection_id
            || self.identity.connection_revision != policy.connection_revision
            || self.identity.policy_revision != policy.revision
        {
            return Err("execution_policy_mismatch");
        }
        policy
            .execution
            .limits
            .timeout(self.operation.execution_class(), self.timeout_seconds)?;
        policy.execution.authorize(&self.operation)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum NotExecutedReason {
    Denied,
    Cancelled,
    TimedOut,
    Restarted,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ExecutionStopReason {
    Cancelled,
    TimedOut,
    Disconnected,
    Restarted,
}

/// RequestAccepted 只确认请求被接纳，Partial 和 Unknown 均不得投影成已成功或已回滚。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ExecutionOutcome {
    NotExecuted {
        reason: NotExecutedReason,
    },
    Completed {
        #[cfg_attr(feature = "schema", schemars(range(min = 0, max = 4_294_967_295_u64)))]
        exit_code: u32,
    },
    RequestAccepted {},
    Stopped {
        reason: ExecutionStopReason,
    },
    Partial {
        reason: ExecutionStopReason,
        termination_confirmed: bool,
    },
    Unknown {
        reason: ExecutionStopReason,
    },
}

impl ExecutionOutcome {
    pub fn termination_confirmed(&self) -> bool {
        matches!(
            self,
            Self::NotExecuted { .. }
                | Self::Completed { .. }
                | Self::Stopped { .. }
                | Self::Partial {
                    termination_confirmed: true,
                    ..
                }
        )
    }
}

pub(super) fn validate_identity(value: &str) -> AgentResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
    {
        Err("invalid_execution_identity")
    } else {
        Ok(())
    }
}

pub(super) fn validate_revision(value: u64) -> AgentResult<()> {
    if value > crate::MAX_REVISION {
        Err("invalid_execution_revision")
    } else {
        Ok(())
    }
}

fn validate_generation(value: u64) -> AgentResult<()> {
    if value == 0 {
        return Err("invalid_execution_generation");
    }
    validate_revision(value)
}

fn validate_request_key(value: &str) -> AgentResult<()> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err("invalid_request_key")
    } else {
        Ok(())
    }
}

fn deserialize_generation<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(decoder)?;
    validate_generation(value).map_err(D::Error::custom)?;
    Ok(value)
}

fn deserialize_request_key<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(decoder)?;
    validate_request_key(&value).map_err(D::Error::custom)?;
    Ok(value)
}

pub(super) fn deserialize_identity<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(decoder)?;
    validate_identity(&value).map_err(D::Error::custom)?;
    Ok(value)
}

pub(super) fn deserialize_revision<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<u64, D::Error> {
    let value = u64::deserialize(decoder)?;
    validate_revision(value).map_err(D::Error::custom)?;
    Ok(value)
}

fn deserialize_timeout<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<u32>, D::Error> {
    let value = Option::<u32>::deserialize(decoder)?;
    if value == Some(0) {
        return Err(D::Error::custom("invalid_timeout"));
    }
    Ok(value)
}

fn deserialize_operation<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<RuntimeOperation, D::Error> {
    let value = RuntimeOperation::deserialize(decoder)?;
    value.validate_business().map_err(D::Error::custom)?;
    Ok(value)
}

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
