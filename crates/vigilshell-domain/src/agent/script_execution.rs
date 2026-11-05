//! 脚本许可与入口快照由应用构造；批准不能来自工具参数。
use super::*;
#[derive(Clone, Debug)]
pub struct CheckedScriptOperation {
    request: ExecutionRequest,
}
impl CheckedScriptOperation {
    pub fn request(&self) -> &ExecutionRequest {
        &self.request
    }
    pub fn bind(&self, snapshot: ScriptSnapshot) -> AgentResult<ScriptReviewBinding> {
        let binding = ScriptReviewBinding {
            identity: self.request.identity.clone(),
            snapshot,
        };
        binding.matches_request(&self.request)?;
        Ok(binding)
    }
}
pub fn check_script(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<CheckedScriptOperation> {
    request.authorize(policy)?;
    if !matches!(request.operation, RuntimeOperation::Script { .. }) {
        return Err("script_required");
    }
    Ok(CheckedScriptOperation {
        request: request.clone(),
    })
}
#[derive(Clone, Debug)]
pub struct CheckedScriptLaunch {
    operation: CheckedScriptOperation,
    snapshot: Option<ScriptSnapshot>,
}
impl CheckedScriptLaunch {
    pub fn operation(&self) -> &CheckedScriptOperation {
        &self.operation
    }
    pub fn snapshot(&self) -> Option<&ScriptSnapshot> {
        self.snapshot.as_ref()
    }
}
pub fn check_script_launch(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
    state: &ExecutionState,
    snapshot: Option<ScriptSnapshot>,
    staging_path: Option<String>,
) -> AgentResult<CheckedScriptLaunch> {
    let checked = check_script(request, policy)?;
    state.validate_for(request, policy)?;
    if state.phase != ExecutionPhase::Queued {
        return Err("execution_not_queued");
    }
    if !policy.execution.script_review {
        return Ok(CheckedScriptLaunch {
            operation: checked,
            snapshot: None,
        });
    }
    let snapshot = snapshot.ok_or("script_snapshot_missing")?;
    if !matches!(state.review, ScriptReviewState::Approved { .. }) {
        return Err("script_review_required");
    }
    if let ScriptReviewState::Approved { binding } = &state.review
        && (binding.snapshot.hash != snapshot.hash
            || binding.snapshot.path != snapshot.path
            || binding.snapshot.interpreter != snapshot.interpreter
            || binding.snapshot.arguments != snapshot.arguments
            || binding.snapshot.directory != snapshot.directory)
    {
        return Err("script_review_binding_changed");
    }
    match (&request.operation, &staging_path) {
        (
            RuntimeOperation::Script {
                source: ScriptSource::Inline { .. },
                ..
            },
            Some(path),
        ) if normalize_path("/", path)? == *path
            && *path == snapshot.path
            && path.ends_with(&format!(
                "/.local/share/vigilshell/script-staging/{}/entry",
                request.identity.task_id
            )) => {}
        (
            RuntimeOperation::Script {
                source: ScriptSource::RemoteFile { .. },
                ..
            },
            None,
        ) => {}
        _ => return Err("invalid_script_staging"),
    }
    checked.bind(snapshot.clone())?;
    Ok(CheckedScriptLaunch {
        operation: checked,
        snapshot: Some(snapshot),
    })
}
