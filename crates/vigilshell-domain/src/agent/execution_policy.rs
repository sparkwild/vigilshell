//! SSH 执行的单一业务权限；命令检查与脚本审阅是独立门槛，不产生运行器授权。
use super::execution::{
    deserialize_identity, deserialize_revision, validate_identity, validate_revision,
};
use super::*;
use serde::de::Error;
use std::collections::HashSet;

pub const EXECUTION_POLICY_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionPolicy {
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
    #[serde(deserialize_with = "deserialize_revision")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = 0, max = 9_007_199_254_740_991_u64))
    )]
    pub revision: u64,
    pub verify_connect: bool,
    pub execution: ExecutionPolicy,
}

impl ConnectionPolicy {
    pub fn validate(&self) -> AgentResult<()> {
        validate_identity(&self.connection_id)?;
        validate_revision(self.connection_revision)?;
        validate_revision(self.revision)?;
        self.execution.validate()
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(extend("x-vigil-rule" = "executionPolicy")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionPolicy {
    #[cfg_attr(feature = "schema", schemars(extend("const" = 2)))]
    pub version: u32,
    pub level: OperationLevel,
    pub file_transfer: bool,
    pub protect_system: bool,
    #[serde(default)]
    pub script_review: bool,
    pub protected_paths: Vec<ProtectedPath>,
    pub upload_roots: Vec<LocalDirectoryGrant>,
    pub download_roots: Vec<LocalDirectoryGrant>,
    pub limits: ExecutionLimits,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            version: EXECUTION_POLICY_VERSION,
            level: OperationLevel::Disabled,
            file_transfer: false,
            protect_system: true,
            script_review: false,
            protected_paths: vec![],
            upload_roots: vec![],
            download_roots: vec![],
            limits: ExecutionLimits::default(),
        }
    }
}

impl<'de> Deserialize<'de> for ExecutionPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Fields {
            version: u32,
            level: OperationLevel,
            file_transfer: bool,
            protect_system: bool,
            #[serde(default)]
            script_review: bool,
            protected_paths: Vec<ProtectedPath>,
            upload_roots: Vec<LocalDirectoryGrant>,
            download_roots: Vec<LocalDirectoryGrant>,
            limits: ExecutionLimits,
        }
        let fields = Fields::deserialize(decoder)?;
        let policy = Self {
            version: fields.version,
            level: fields.level,
            file_transfer: fields.file_transfer,
            protect_system: fields.protect_system,
            script_review: fields.script_review,
            protected_paths: fields.protected_paths,
            upload_roots: fields.upload_roots,
            download_roots: fields.download_roots,
            limits: fields.limits,
        };
        policy.validate().map_err(D::Error::custom)?;
        Ok(policy)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CommandCheckRequirement {
    /// 请求不是普通命令；不表示脚本正文、PTY 字节流或间接代码已经通过命令检查。
    None,
    /// 解析单个简单命令、校验参数并按只读白名单重建程序与参数。
    ReadOnlyAllowlist,
    /// 始终检查命令语法和参数；系统危险目标检查另受 protectSystem 配置约束。
    HighRisk,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionAuthorization {
    pub targets: Vec<OperationTarget>,
    pub command_check: CommandCheckRequirement,
    pub script_review_required: bool,
    /// 明确文件目标仍须在适配器中核实实际路径、链接、版本和提交能力。
    pub target_check_required: bool,
}

impl ExecutionPolicy {
    pub fn unrestricted_local_transfers(&self) -> bool {
        self.upload_roots.is_empty() && self.download_roots.is_empty()
    }
    pub fn validate(&self) -> AgentResult<()> {
        if self.version != EXECUTION_POLICY_VERSION {
            return Err("policy_model_version_unsupported");
        }
        if self.protected_paths.len() > 128 {
            return Err("scope_limit");
        }
        let mut paths = HashSet::new();
        for target in &self.protected_paths {
            validate_absolute_path(&target.path)?;
            if target.kind == ScopeKind::File && target.path == "/" {
                return Err("invalid_protected_path");
            }
            if !paths.insert(&target.path) {
                return Err("invalid_protected_path");
            }
        }
        validate_grants(&self.upload_roots)?;
        validate_grants(&self.download_roots)?;
        self.limits.validate()
    }

    pub fn authorize(&self, operation: &RuntimeOperation) -> AgentResult<ExecutionAuthorization> {
        self.validate()?;
        operation.validate_business()?;
        if self.level == OperationLevel::Disabled {
            return Err("connection_denied");
        }
        if self.level == OperationLevel::ReadOnly
            && (matches!(
                operation,
                RuntimeOperation::Script { .. } | RuntimeOperation::Terminal { .. }
            ) || operation.mutating() && !matches!(operation, RuntimeOperation::Command { .. }))
        {
            return Err("read_only_operation_denied");
        }
        let mut targets = operation.business_targets()?;
        match operation {
            RuntimeOperation::TransfersUpload { local, .. } => {
                if !self.file_transfer || self.level != OperationLevel::Operate {
                    return Err("transfer_denied");
                }
                if self.unrestricted_local_transfers() {
                    resolve_unrestricted_local(local)?;
                } else {
                    resolve_local_grant(&self.upload_roots, local)?;
                }
            }
            RuntimeOperation::TransfersDownload { local, .. } => {
                if !self.file_transfer {
                    return Err("transfer_denied");
                }
                if self.unrestricted_local_transfers() {
                    resolve_unrestricted_local(local)?;
                } else {
                    resolve_local_grant(&self.download_roots, local)?;
                }
            }
            _ => {}
        }
        for target in &targets {
            self.check_explicit_target(target)?;
        }
        // 原生维护不能用一份貌似完整的路径清单描述包管理器与 systemd 的全部效果。
        if operation.is_maintenance() {
            targets.clear();
        }
        Ok(ExecutionAuthorization {
            command_check: if matches!(operation, RuntimeOperation::Command { .. }) {
                if self.level == OperationLevel::ReadOnly {
                    CommandCheckRequirement::ReadOnlyAllowlist
                } else {
                    CommandCheckRequirement::HighRisk
                }
            } else {
                CommandCheckRequirement::None
            },
            script_review_required: self.script_review
                && matches!(operation, RuntimeOperation::Script { .. }),
            target_check_required: !targets.is_empty() || operation.is_maintenance(),
            targets,
        })
    }

    pub(super) fn check_explicit_target(&self, target: &OperationTarget) -> AgentResult<()> {
        if target.effect == FileEffect::Read {
            return Ok(());
        }
        if target.path == "/" {
            return Err("dangerous_target");
        }
        if self.protected_paths.iter().any(|protected| {
            protected.path == target.path
                || protected.kind == ScopeKind::Directory && beneath(&protected.path, &target.path)
                || target.recursive && beneath(&target.path, &protected.path)
        }) {
            return Err("protected_target");
        }
        if self.protect_system
            && target
                .path
                .split('/')
                .any(|part| matches!(part, ".ssh" | ".gnupg"))
        {
            return Err("dangerous_system_target");
        }
        if self.protect_system
            && system_protections()
                .iter()
                .any(|rule| rule.blocks(&target.path, target.effect, target.recursive))
        {
            return Err("dangerous_system_target");
        }
        Ok(())
    }
}

pub(super) fn validate_absolute_path(path: &str) -> AgentResult<()> {
    if !path.starts_with('/') || normalize_path("/", path)? != path {
        Err("invalid_path")
    } else {
        Ok(())
    }
}

fn validate_grants(grants: &[LocalDirectoryGrant]) -> AgentResult<()> {
    if grants.len() > 128 {
        return Err("scope_limit");
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for grant in grants {
        validate_identity(&grant.id)?;
        validate_absolute_path(&grant.path)?;
        if grant.id == ACCOUNT_SCOPE_ID
            || grant.path == "/"
            || !ids.insert(&grant.id)
            || !paths.insert(&grant.path)
        {
            return Err("invalid_local_scope");
        }
    }
    Ok(())
}

pub(super) fn resolve_unrestricted_local(target: &ScopedPath) -> AgentResult<String> {
    if target.scope_id != "local" || target.path.starts_with('/') || target.path.is_empty() {
        return Err("invalid_local_scope");
    }
    let path = normalize_path("/", &target.path)?;
    if path == "/" {
        return Err("invalid_path");
    }
    Ok(path)
}

fn resolve_local_grant(grants: &[LocalDirectoryGrant], target: &ScopedPath) -> AgentResult<String> {
    let grant = grants
        .iter()
        .find(|grant| grant.id == target.scope_id)
        .ok_or("scope_not_found")?;
    if target.path.starts_with('/') {
        return Err("invalid_relative_path");
    }
    let resolved = normalize_path(
        &grant.path,
        if target.path.is_empty() {
            "."
        } else {
            &target.path
        },
    )?;
    if !beneath(&grant.path, &resolved) {
        return Err("path_denied");
    }
    Ok(resolved)
}

#[cfg(test)]
#[path = "execution_policy_tests.rs"]
mod tests;

impl ExecutionPolicy {
    pub fn permits_existing(&self, old: &Self) -> bool {
        self.level >= old.level
            && (!old.file_transfer || self.file_transfer)
            && (!self.protect_system || old.protect_system)
            && self
                .protected_paths
                .iter()
                .all(|p| old.protected_paths.contains(p))
            && (self.unrestricted_local_transfers()
                || (!old.unrestricted_local_transfers()
                    && old
                        .upload_roots
                        .iter()
                        .all(|r| self.upload_roots.contains(r))
                    && old
                        .download_roots
                        .iter()
                        .all(|r| self.download_roots.contains(r))))
    }
}
