//! 文件请求的本地准入与结果；真实路径及提交能力由 SSH/SFTP 适配器核对。
use super::*;

#[derive(Clone, Debug)]
pub struct CheckedFileOperation {
    operation: RuntimeOperation,
    authorization: FileAuthorization,
    id: String,
    scope: String,
    removal_hash: Option<String>,
}
#[derive(Clone, Debug)]
enum FileAuthorization {
    Ordinary(ExecutionPolicy),
    Native {
        targets: Vec<OperationTarget>,
        private_root: String,
    },
}
impl CheckedFileOperation {
    pub fn removal_hash(&self) -> Option<&str> {
        self.removal_hash.as_deref()
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    pub fn staging_path(&self, target: &str) -> String {
        format!(
            "{}/.vigilshell-{}.part",
            target.rsplit_once('/').map_or("", |(p, _)| p),
            self.id
        )
    }
    pub fn operation(&self) -> &RuntimeOperation {
        &self.operation
    }
    pub fn target(&self, path: &str, effect: FileEffect, recursive: bool) -> AgentResult<()> {
        match &self.authorization {
            FileAuthorization::Ordinary(policy) => policy.check_explicit_target(&OperationTarget {
                path: path.into(),
                effect,
                recursive,
            }),
            FileAuthorization::Native {
                targets,
                private_root,
            } => {
                if effect == FileEffect::Read
                    || (path != private_root && beneath(private_root, path))
                    || targets.iter().any(|t| {
                        t.effect != FileEffect::Read
                            && (t.path == path || self.staging_path(&t.path) == path)
                    })
                {
                    Ok(())
                } else {
                    Err("native_file_target_denied")
                }
            }
        }
    }
}
pub fn check_file_operation(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<CheckedFileOperation> {
    request.authorize(policy)?;
    if !request.operation.is_file_operation() {
        return Err("file_operation_required");
    }
    Ok(CheckedFileOperation {
        operation: request.operation.clone(),
        authorization: FileAuthorization::Ordinary(policy.execution.clone()),
        id: request.identity.task_id.clone(),
        removal_hash: None,
        scope: format!(
            "{}:{}:{}",
            request.identity.entry_id, request.identity.session_id, policy.revision
        ),
    })
}
impl RuntimeOperation {
    pub fn is_file_operation(&self) -> bool {
        matches!(
            self,
            Self::FilesList { .. }
                | Self::FilesStat { .. }
                | Self::FilesRead { .. }
                | Self::FilesWrite { .. }
                | Self::FilesEdit { .. }
                | Self::FilesMkdir { .. }
                | Self::FilesCopy { .. }
                | Self::FilesMove { .. }
                | Self::FilesRemove { .. }
                | Self::FilesSetAttributes { .. }
                | Self::FilesLink { .. }
                | Self::TransfersUpload { .. }
                | Self::TransfersDownload { .. }
        )
    }
}
#[derive(Clone, Debug)]
pub struct LocalTransferGrant {
    root: String,
    path: String,
    upload: bool,
    unrestricted: bool,
}
impl LocalTransferGrant {
    pub fn unrestricted(&self) -> bool {
        self.unrestricted
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn upload(&self) -> bool {
        self.upload
    }
}
pub fn local_transfer_grant(
    policy: &ExecutionPolicy,
    operation: &RuntimeOperation,
) -> AgentResult<LocalTransferGrant> {
    policy.authorize(operation)?;
    let (target, upload) = match operation {
        RuntimeOperation::TransfersUpload { local, .. } => (local, true),
        RuntimeOperation::TransfersDownload { local, .. } => (local, false),
        _ => return Err("transfer_required"),
    };
    if policy.unrestricted_local_transfers() {
        let absolute = super::execution_policy::resolve_unrestricted_local(target)?;
        let (parent, name) = absolute.rsplit_once('/').ok_or("invalid_path")?;
        return Ok(LocalTransferGrant {
            root: if parent.is_empty() {
                "/".into()
            } else {
                parent.into()
            },
            path: name.into(),
            upload,
            unrestricted: true,
        });
    }
    let roots = if upload {
        &policy.upload_roots
    } else {
        &policy.download_roots
    };
    let root = roots
        .iter()
        .find(|r| r.id == target.scope_id)
        .ok_or("local_scope_missing")?;
    Ok(LocalTransferGrant {
        root: root.path.clone(),
        path: target.path.clone(),
        upload,
        unrestricted: false,
    })
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum FileCommitState {
    Unchanged,
    Committed,
    Partial,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileExecutionReport {
    pub commit: FileCommitState,
    pub error: Option<String>,
    pub json: Option<String>,
    pub recovery_paths: Vec<String>,
    pub bytes: u64,
    pub hash: Option<String>,
}

/// 仅为已判权的具名维护构造固定文件端点，不修改普通操作的保护配置。
pub fn native_file_operation(
    permit: &CheckedNativeOperation,
    operation: RuntimeOperation,
    private_root: &str,
) -> AgentResult<CheckedFileOperation> {
    operation.validate_business()?;
    if !matches!(
        operation,
        RuntimeOperation::FilesStat { .. }
            | RuntimeOperation::FilesRead { .. }
            | RuntimeOperation::FilesWrite { .. }
            | RuntimeOperation::FilesMkdir { .. }
            | RuntimeOperation::FilesRemove { .. }
            | RuntimeOperation::FilesSetAttributes { .. }
    ) || operation.is_transfer()
        || operation.mutating() && !permit.operation().mutating()
    {
        return Err("native_file_operation_denied");
    }
    if normalize_path("/", private_root)? != private_root
        || !private_root.ends_with(&format!(
            "/.local/share/vigilshell/native-staging/{}",
            permit.id()
        ))
    {
        return Err("invalid_native_staging");
    }
    let mut targets = operation.business_targets()?;
    for t in &targets {
        let backup = permit.backup_root(private_root)?;
        let staging = t.path == private_root
            || beneath(private_root, &t.path)
            || t.path == backup
            || beneath(&backup, &t.path);
        let parent = (private_root.starts_with(&(t.path.clone() + "/"))
            || backup.starts_with(&(t.path.clone() + "/")))
            && matches!(operation, RuntimeOperation::FilesMkdir { .. });
        let source = permit.operation().is_package_source()
            && (package_sources::source_path(&t.path)
                || package_sources::key_path(&t.path)
                || [
                    "/etc/apt/sources.list.d",
                    "/etc/yum.repos.d",
                    "/usr/share/keyrings",
                    "/etc/pki/rpm-gpg",
                ]
                .contains(&t.path.as_str()));
        if source
            && t.effect != FileEffect::Read
            && !package_sources::source_path(&t.path)
            && !package_sources::key_path(&t.path)
            && !matches!(operation, RuntimeOperation::FilesMkdir { .. })
        {
            return Err("native_file_target_denied");
        }
        let local = matches!(permit.operation(),RuntimeOperation::PackagesInstall{packages}|RuntimeOperation::PackagesUpdate{packages} if t.effect==FileEffect::Read && packages.iter().any(|p|matches!(p,PackageTarget::Local{path,..} if path==&t.path)));
        let home = private_root
            .strip_suffix(&format!(
                "/.local/share/vigilshell/native-staging/{}",
                permit.id()
            ))
            .ok_or("invalid_native_staging")?;
        let service = service_file_target(permit.operation(), &operation, t, home)?;
        let dns = matches!(permit.operation(), RuntimeOperation::DnsUpdate { .. })
            && t.path == "/etc/resolv.conf"
            && !matches!(
                operation,
                RuntimeOperation::FilesRemove { .. } | RuntimeOperation::FilesMkdir { .. }
            );
        if !(staging || parent || source || local || service || dns) {
            return Err("native_file_target_denied");
        }
        if t.effect != FileEffect::Read
            && source
            && package_sources::key_path(&t.path)
            && !t
                .path
                .rsplit('/')
                .next()
                .unwrap_or("")
                .starts_with("vigilshell-managed-")
        {
            return Err("native_key_target_denied");
        }
    }
    if matches!(
        operation,
        RuntimeOperation::FilesMkdir { parents: true, .. }
    ) {
        let mut path = targets
            .first()
            .ok_or("native_file_target_denied")?
            .path
            .clone();
        while let Some((parent, _)) = path.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            let next = parent.to_string();
            targets.push(OperationTarget {
                path: next.clone(),
                effect: FileEffect::Write,
                recursive: false,
            });
            path = next;
        }
    }
    let removal_hash = if matches!(
        operation,
        RuntimeOperation::FilesRemove {
            recursive: false,
            ..
        }
    ) {
        match permit.operation() {
            RuntimeOperation::ServicesUnregister { expected_hash, .. }
            | RuntimeOperation::PackageSourcesRemove { expected_hash, .. } => {
                Some(expected_hash.clone())
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(CheckedFileOperation {
        operation,
        authorization: FileAuthorization::Native {
            targets,
            private_root: private_root.into(),
        },
        removal_hash,
        id: permit.id().into(),
        scope: format!("native:{}", permit.id()),
    })
}

pub fn native_service_path(name: &str, home: &str, drop_in: Option<&str>) -> AgentResult<String> {
    if !valid_service_name(name) || !name.ends_with(".service") {
        return Err("invalid_service");
    }
    let unit = name.strip_prefix("user:").unwrap_or(name);
    let root = if name.starts_with("user:") {
        format!("{home}/.config/systemd/user")
    } else {
        "/etc/systemd/system".into()
    };
    let base = format!("{root}/{unit}");
    if let Some(drop) = drop_in {
        validate_drop_in(drop)?;
        Ok(format!("{base}.d/{drop}"))
    } else {
        Ok(base)
    }
}
fn service_file_target(
    permit: &RuntimeOperation,
    operation: &RuntimeOperation,
    target: &OperationTarget,
    home: &str,
) -> AgentResult<bool> {
    let (name, drop) = match permit {
        RuntimeOperation::ServicesRegister { definition }
        | RuntimeOperation::ServicesUpdate { definition } => (
            definition.target.identifier()?,
            match &definition.content {
                ServiceDefinitionContent::Unit { .. } => None,
                ServiceDefinitionContent::DropIn { name, .. } => Some(name.as_str()),
            },
        ),
        RuntimeOperation::ServicesUnregister { name, drop_in, .. } => {
            (name.clone(), drop_in.as_deref())
        }
        RuntimeOperation::ServicesDefinition { name } => (name.clone(), None),
        _ => return Ok(false),
    };
    let file = native_service_path(&name, home, drop)?;
    if target.path == file {
        return Ok(true);
    }
    if matches!(operation, RuntimeOperation::FilesMkdir { .. })
        && file.starts_with(&(target.path.clone() + "/"))
    {
        return Ok(true);
    }
    if target.effect != FileEffect::Read {
        return Ok(false);
    }
    let unit = name.strip_prefix("user:").unwrap_or(&name);
    let base = native_service_path(&name, home, None)?;
    Ok(target.path == base
        || target.path.starts_with(&format!("{base}.d/"))
        || [
            "/usr/lib/systemd/system",
            "/lib/systemd/system",
            "/usr/lib/systemd/user",
            "/lib/systemd/user",
        ]
        .iter()
        .any(|root| {
            target.path == format!("{root}/{unit}")
                || target.path.starts_with(&format!("{root}/{unit}.d/"))
        }))
}

pub fn script_file_operation(
    permit: &CheckedScriptOperation,
    operation: RuntimeOperation,
    root: &str,
) -> AgentResult<CheckedFileOperation> {
    operation.validate_business()?;
    if normalize_path("/", root)? != root
        || !root.ends_with(&format!(
            "/.local/share/vigilshell/script-staging/{}",
            permit.request().identity.task_id
        ))
    {
        return Err("invalid_script_staging");
    }
    if !matches!(
        operation,
        RuntimeOperation::FilesStat { .. }
            | RuntimeOperation::FilesRead { .. }
            | RuntimeOperation::FilesWrite { .. }
            | RuntimeOperation::FilesMkdir { .. }
            | RuntimeOperation::FilesSetAttributes { .. }
            | RuntimeOperation::FilesRemove { .. }
    ) {
        return Err("script_file_operation_denied");
    }
    let mut targets = operation.business_targets()?;
    let source = match &permit.request().operation {
        RuntimeOperation::Script {
            source: ScriptSource::RemoteFile { target, .. },
            ..
        } => Some(normalize_path(
            "/",
            if target.path.is_empty() {
                "."
            } else {
                &target.path
            },
        )?),
        _ => None,
    };
    for target in &targets {
        let private = beneath(root, &target.path);
        let parent = matches!(operation, RuntimeOperation::FilesMkdir { .. })
            && root.starts_with(&(target.path.clone() + "/"));
        if !(private
            || parent
            || target.effect == FileEffect::Read && source.as_deref() == Some(&target.path))
        {
            return Err("script_file_target_denied");
        }
    }
    if matches!(
        operation,
        RuntimeOperation::FilesMkdir { parents: true, .. }
    ) {
        let mut path = targets
            .first()
            .ok_or("script_file_target_denied")?
            .path
            .clone();
        while let Some((parent, _)) = path.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            let next = parent.to_owned();
            targets.push(OperationTarget {
                path: next.clone(),
                effect: FileEffect::Write,
                recursive: false,
            });
            path = next;
        }
    }
    Ok(CheckedFileOperation {
        operation,
        authorization: FileAuthorization::Native {
            targets,
            private_root: root.into(),
        },
        id: permit.request().identity.task_id.clone(),
        scope: format!("script:{}", permit.request().identity.task_id),
        removal_hash: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credential_targets_and_local_grants_use_current_policy_only() {
        let mut policy = ExecutionPolicy {
            level: OperationLevel::Operate,
            file_transfer: true,
            upload_roots: vec![LocalDirectoryGrant {
                id: "local".into(),
                path: "/tmp/allowed".into(),
            }],
            download_roots: vec![LocalDirectoryGrant {
                id: "local".into(),
                path: "/tmp/allowed".into(),
            }],
            ..Default::default()
        };
        let credential = RuntimeOperation::FilesWrite {
            target: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "home/operator/.ssh/authorized_keys".into(),
            },
            data: "YQ==".into(),
            mode: FileWriteMode::Replace,
            expected_hash: None,
        };
        assert!(policy.authorize(&credential).is_err());
        policy.protect_system = false;
        assert!(policy.authorize(&credential).is_ok());
        let remote = ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/file".into(),
        };
        let upload = RuntimeOperation::TransfersUpload {
            local: ScopedPath {
                scope_id: "local".into(),
                path: "file".into(),
            },
            remote: remote.clone(),
            overwrite: false,
        };
        assert!(local_transfer_grant(&policy, &upload).unwrap().upload());
        policy.level = OperationLevel::ReadOnly;
        assert!(local_transfer_grant(&policy, &upload).is_err());
        let download = RuntimeOperation::TransfersDownload {
            remote,
            local: ScopedPath {
                scope_id: "local".into(),
                path: "../escape".into(),
            },
            overwrite: true,
        };
        assert!(local_transfer_grant(&policy, &download).is_err());
    }
}
