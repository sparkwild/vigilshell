use super::*;

/// Live configuration only. Historical task/recording policy snapshots keep their existing shape.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPolicySettings {
    pub access_enabled: bool,
    #[serde(flatten)]
    pub policy: ConnectionPolicy,
}
impl std::ops::Deref for ConnectionPolicySettings {
    type Target = ConnectionPolicy;
    fn deref(&self) -> &Self::Target {
        &self.policy
    }
}
impl ConnectionPolicySettings {
    pub fn new(connection_id: String, connection_revision: u64) -> Self {
        let mut policy = ConnectionPolicy {
            connection_id,
            connection_revision,
            ..Default::default()
        };
        policy.execution.level = OperationLevel::ReadOnly;
        Self {
            access_enabled: false,
            policy,
        }
    }
    pub fn enabled(&self) -> bool {
        self.access_enabled && self.policy.enabled()
    }
    pub fn validate(&self) -> AgentResult<()> {
        self.policy.validate()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum PolicySettingsPatch {
    VerifyConnect(bool),
    Level(OperationLevel),
    FileTransfer(bool),
    ProtectSystem(bool),
    ScriptReview(bool),
    ProtectedPaths(Vec<ProtectedPath>),
    LocalDirectory(Option<String>),
    UploadRoots(Vec<LocalDirectoryGrant>),
    DownloadRoots(Vec<LocalDirectoryGrant>),
    Limit(PolicyLimitPatch),
}
impl PolicySettingsPatch {
    pub fn apply(&self, current: &ConnectionPolicy) -> AgentResult<ConnectionPolicy> {
        let mut p = current.clone();
        match self {
            Self::VerifyConnect(v) => p.verify_connect = *v,
            Self::Level(v) => {
                if *v == OperationLevel::Disabled {
                    return Err("invalid_policy");
                }
                p.execution.level = *v;
            }
            Self::FileTransfer(v) => p.execution.file_transfer = *v,
            Self::ProtectSystem(v) => p.execution.protect_system = *v,
            Self::ScriptReview(v) => p.execution.script_review = *v,
            Self::ProtectedPaths(v) => p.execution.protected_paths = v.clone(),
            Self::LocalDirectory(path) => {
                let roots = path
                    .as_ref()
                    .map(|path| {
                        vec![LocalDirectoryGrant {
                            id: "local".into(),
                            path: path.clone(),
                        }]
                    })
                    .unwrap_or_default();
                p.execution.upload_roots = roots.clone();
                p.execution.download_roots = roots;
            }
            Self::UploadRoots(v) => p.execution.upload_roots = v.clone(),
            Self::DownloadRoots(v) => p.execution.download_roots = v.clone(),
            Self::Limit(v) => v.apply(&mut p.execution.limits)?,
        }
        p.validate()?;
        Ok(p)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyLimitPatch {
    pub key: PolicyLimitKey,
    pub value: Option<u32>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum PolicyLimitKey {
    MaxTaskSeconds,
    TerminalSeconds,
    TerminalIdleSeconds,
    TransferSeconds,
    TransferStallSeconds,
    ConcurrentTasks,
    ConcurrentTerminals,
    ConcurrentTransfers,
}
impl PolicyLimitPatch {
    fn apply(&self, limits: &mut ExecutionLimits) -> AgentResult<()> {
        match self.key {
            PolicyLimitKey::MaxTaskSeconds => limits.max_task_seconds = self.value,
            PolicyLimitKey::TerminalSeconds => limits.terminal_seconds = self.value,
            PolicyLimitKey::TerminalIdleSeconds => limits.terminal_idle_seconds = self.value,
            PolicyLimitKey::TransferSeconds => limits.transfer_seconds = self.value,
            PolicyLimitKey::TransferStallSeconds => limits.transfer_stall_seconds = self.value,
            PolicyLimitKey::ConcurrentTasks
            | PolicyLimitKey::ConcurrentTerminals
            | PolicyLimitKey::ConcurrentTransfers => {
                let v = self
                    .value
                    .filter(|v| (1..=16).contains(v))
                    .ok_or("invalid_limits")? as u16;
                match self.key {
                    PolicyLimitKey::ConcurrentTasks => limits.concurrent_tasks = v,
                    PolicyLimitKey::ConcurrentTerminals => limits.concurrent_terminals = v,
                    _ => limits.concurrent_transfers = v,
                }
            }
        }
        limits.validate()
    }
}
