use super::*;
use vigilshell_application_contracts::file_editing::FileEditCommand;
pub(in crate::workspace) struct EditLease {
    pub owner: String,
    pub group: Arc<RuntimeGroup>,
    value: tokio::sync::Mutex<Option<FileEditRemote>>,
}
impl WorkspaceService {
    pub(super) async fn edit_action(
        self: &Arc<Self>,
        owner: &str,
        command: FileEditCommand,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        match command {
            FileEditCommand::Open {
                id,
                lease_owner,
                session,
                path,
                local,
            } => {
                if !valid_id(&id) || !valid_id(&lease_owner) || !local.is_absolute() {
                    return Err(WorkspaceError::InvalidInput);
                }
                valid_remote_path(&path)?;
                let group = self.tool_group(owner, &session)?;
                let lease = Arc::new(EditLease {
                    owner: lease_owner,
                    group: group.clone(),
                    value: tokio::sync::Mutex::new(None),
                });
                {
                    let mut state = self.state.lock().unwrap();
                    if state.file_edits.contains_key(&id) || state.file_edits.len() >= 32 {
                        return Err(WorkspaceError::Busy);
                    }
                    state.file_edits.insert(id.clone(), lease.clone());
                }
                let mut value = lease.value.lock().await;
                let downloaded = async {
                    self.tools_client(&group)
                        .await?
                        .edit_download(path.clone(), local)
                        .await
                }
                .await;
                match downloaded {
                    Ok(version) => {
                        let edit = FileEditRemote {
                            id,
                            connection_id: group.profile.id.clone(),
                            group_id: group.tools.id.clone(),
                            path,
                            version,
                        };
                        *value = Some(edit.clone());
                        Ok(WorkspaceResult::Tools(ToolResult::FileEdit(edit)))
                    }
                    Err(e) => {
                        self.state.lock().unwrap().file_edits.remove(&id);
                        Err(e)
                    }
                }
            }
            other => {
                let id = match &other {
                    FileEditCommand::Inspect { id }
                    | FileEditCommand::Upload { id, .. }
                    | FileEditCommand::Reload { id, .. }
                    | FileEditCommand::Close { id } => id,
                    _ => unreachable!(),
                };
                let lease = self
                    .state
                    .lock()
                    .unwrap()
                    .file_edits
                    .get(id)
                    .filter(|e| e.owner == owner)
                    .cloned()
                    .ok_or(WorkspaceError::NotFound)?;
                let mut held = lease.value.try_lock().map_err(|_| WorkspaceError::Busy)?;
                let edit = held.as_mut().ok_or(WorkspaceError::Busy)?;
                match other {
                    FileEditCommand::Inspect { .. } => {
                        let mut current = edit.clone();
                        current.version = self
                            .tools_client(&lease.group)
                            .await?
                            .edit_version(edit.path.clone())
                            .await?;
                        return Ok(WorkspaceResult::Tools(ToolResult::FileEdit(current)));
                    }
                    FileEditCommand::Upload {
                        local,
                        expected,
                        destination,
                        ..
                    } => {
                        if !local.is_absolute() {
                            return Err(WorkspaceError::InvalidInput);
                        }
                        let client = self.tools_client(&lease.group).await?;
                        let target = destination.as_deref().unwrap_or(&edit.path).to_owned();
                        valid_remote_path(&target)?;
                        let baseline = if destination.is_some() && target != edit.path {
                            None
                        } else {
                            let current = client.edit_version(target.clone()).await?;
                            if current != expected {
                                return Err(WorkspaceError::Conflict);
                            }
                            Some(current)
                        };
                        edit.version = client.edit_upload(target.clone(), local, baseline).await?;
                        edit.path = target;
                    }
                    FileEditCommand::Reload { local, .. } => {
                        if !local.is_absolute() {
                            return Err(WorkspaceError::InvalidInput);
                        }
                        edit.version = self
                            .tools_client(&lease.group)
                            .await?
                            .edit_download(edit.path.clone(), local)
                            .await?;
                    }
                    FileEditCommand::Close { id } => {
                        self.state.lock().unwrap().file_edits.remove(&id);
                        let result = edit.clone();
                        drop(held);
                        self.stop_group_tools(&lease.group).await;
                        let _ = self.recover_credentials().await;
                        return Ok(WorkspaceResult::Tools(ToolResult::FileEdit(result)));
                    }
                    FileEditCommand::Open { .. } => unreachable!(),
                }
                Ok(WorkspaceResult::Tools(ToolResult::FileEdit(edit.clone())))
            }
        }
    }
}
