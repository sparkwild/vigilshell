use super::*;
impl WorkspaceService {
    pub(in crate::workspace) async fn transfers_action(
        self: &Arc<Self>,
        owner: &str,
        action: ToolAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let (action, archive_paths) = match action {
            ToolAction::Archive {
                session,
                grant,
                paths,
            } => (
                ToolAction::Transfer {
                    session,
                    grant,
                    paths: paths.clone(),
                    overwrite: false,
                },
                Some(paths),
            ),
            other => (other, None),
        };
        let reply = match action {
            ToolAction::Grant { direction, paths } => {
                if !["upload", "download"].contains(&direction.as_str())
                    || paths.is_empty()
                    || paths.len() > 100
                    || paths.iter().any(|p| !p.is_absolute())
                    || (direction == "download" && paths.len() != 1)
                {
                    return Err(WorkspaceError::InvalidInput);
                }
                let id = new_id();
                let names = paths
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                let mut state = self.state.lock().unwrap();
                state.grants.retain(|_, grant| grant.owner != owner);
                state.grants.insert(
                    id.clone(),
                    FileGrant {
                        owner: owner.into(),
                        direction,
                        paths,
                    },
                );
                ToolResult::Grant(FileGrantView { id, names })
            }
            ToolAction::Transfer {
                session,
                grant,
                paths,
                overwrite,
            } => {
                let _gate = self.group_gate.lock().await;
                let group = self.tool_group(owner, &session)?;
                if paths.is_empty() || paths.len() > 100 {
                    return Err(WorkspaceError::InvalidInput);
                }
                for path in &paths {
                    valid_remote_path(path)?;
                }
                let grant = {
                    let mut state = self.state.lock().unwrap();
                    let grant_count = state
                        .grants
                        .get(&grant)
                        .map(|g| {
                            if archive_paths.is_some() {
                                1
                            } else if g.direction == "upload" {
                                g.paths.len()
                            } else {
                                paths.len()
                            }
                        })
                        .unwrap_or(0);
                    if state.transfers.len() > 200 {
                        let expired: Vec<_> = state
                            .transfers
                            .iter()
                            .filter(|(_, t)| !transfer_active(&t.view.phase))
                            .take(state.transfers.len() - 200)
                            .map(|(id, _)| id.clone())
                            .collect();
                        for id in expired {
                            state.transfers.remove(&id);
                        }
                    }
                    if state
                        .transfers
                        .values()
                        .filter(|t| t.owner == owner && transfer_active(&t.view.phase))
                        .count()
                        + grant_count
                        > 100
                    {
                        return Err(WorkspaceError::Busy);
                    }
                    if state.grants.get(&grant).is_none_or(|g| g.owner != owner) {
                        return Err(WorkspaceError::NotFound);
                    }
                    state.grants.remove(&grant).unwrap()
                };
                let upload = grant.direction == "upload";
                if archive_paths.is_some() && upload {
                    return Err(WorkspaceError::InvalidInput);
                }
                if upload && paths.len() != 1 {
                    return Err(WorkspaceError::InvalidInput);
                }
                let pairs: Vec<(PathBuf, String)> = if archive_paths.is_some() {
                    vec![(
                        grant.paths[0].join(format!("selection-{}.tar.gz", new_id())),
                        paths[0].clone(),
                    )]
                } else if upload {
                    grant
                        .paths
                        .into_iter()
                        .map(|local| {
                            let name = local.file_name().unwrap_or_default().to_string_lossy();
                            let remote = format!("{}/{}", paths[0].trim_end_matches('/'), name);
                            (local, remote)
                        })
                        .collect()
                } else {
                    paths
                        .into_iter()
                        .map(|remote| {
                            let name = remote.rsplit('/').next().unwrap_or("");
                            if name.is_empty() || name == "." || name == ".." {
                                return Err(WorkspaceError::InvalidInput);
                            }
                            Ok((grant.paths[0].join(name), remote))
                        })
                        .collect::<Result<_, _>>()?
                };
                let identity = format!(
                    "{}@{}:{}",
                    group.profile.input.username,
                    group.profile.input.host,
                    group.profile.input.port
                );
                if pairs.iter().any(|(_, path)| {
                    1024 + json_string_size(path)
                        + json_string_size(path.rsplit('/').next().unwrap_or(""))
                        + json_string_size(&identity)
                        > 60 * 1024
                }) {
                    return Err(WorkspaceError::FilesTooLarge);
                }
                let batch_id = new_id();
                let batch_size = pairs.len() as u32;
                let mut accepted = Vec::with_capacity(pairs.len());
                let batch_policy = Arc::new(Mutex::new(if overwrite { Some(true) } else { None }));
                for (local, remote) in pairs {
                    let id = new_id();
                    let view = TransferView {
                        conflict: None,
                        created_at: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        details: None,
                        batch_size,
                        batch_id: batch_id.clone(),
                        remote_path: remote.clone(),
                        remote_identity: format!(
                            "{}@{}:{}",
                            group.profile.input.username,
                            group.profile.input.host,
                            group.profile.input.port
                        ),
                        attempt: 1,
                        revision: 0,
                        conflict_version: 0,
                        can_retry: false,
                        id: id.clone(),
                        connection_id: group.profile.id.clone(),
                        group_id: group.tools.id.clone(),
                        name: if archive_paths.is_some() {
                            local
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            remote.rsplit('/').next().unwrap_or("").into()
                        },
                        direction: grant.direction.clone(),
                        phase: "queued".into(),
                        bytes: 0,
                        total: None,
                        error: None,
                    };
                    self.state.lock().unwrap().transfers.insert(
                        id.clone(),
                        TransferTask {
                            tree: Arc::new(Mutex::new(TransferTreeState {
                                stage: "enumerating".into(),
                                ..Default::default()
                            })),
                            archive_paths: archive_paths.clone(),
                            batch_policy: batch_policy.clone(),
                            owner: owner.into(),
                            group: group.clone(),
                            view: view.clone(),
                            local,
                            remote,
                            overwrite,
                            cancel: Arc::new(AtomicBool::new(false)),
                            progress: Arc::new(AtomicU64::new(0)),
                            total: Arc::new(AtomicU64::new(u64::MAX)),
                        },
                    );

                    accepted.push(id);
                }
                for id in accepted {
                    self.begin_transfer(owner.into(), id);
                }
                // Progress notifications carry each accepted task independently.
                ToolResult::Done
            }
            ToolAction::TransferItems { id, offset } => {
                let state = self.state.lock().unwrap();
                let task = state
                    .transfers
                    .get(&id)
                    .filter(|t| t.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                let tree = task.tree.lock().unwrap();
                let offset = offset as usize;
                if offset > tree.entries.len() {
                    return Err(WorkspaceError::InvalidInput);
                }
                let mut items = Vec::new();
                let mut bytes = 1024;
                for entry in tree.entries.iter().skip(offset).take(100) {
                    let path = if task.view.direction == "upload" {
                        entry.local.to_string_lossy().into_owned()
                    } else {
                        entry.remote.clone()
                    };
                    let size = 256 + json_string_size(&path);
                    if size + bytes > 60 * 1024 {
                        if items.is_empty() {
                            return Err(WorkspaceError::FilesTooLarge);
                        }
                        break;
                    }
                    bytes += size;
                    items.push(TransferItem {
                        path,
                        phase: entry.phase.clone(),
                        error: entry.error.clone(),
                    });
                }
                let end = offset + items.len();
                ToolResult::TransferItems(TransferItemsPage {
                    items,
                    next: (end < tree.entries.len()).then_some(end as u32),
                })
            }
            ToolAction::CancelTransfer { id } => {
                let group = {
                    let mut state = self.state.lock().unwrap();
                    let task = state
                        .transfers
                        .get_mut(&id)
                        .filter(|t| t.owner == owner)
                        .ok_or(WorkspaceError::NotFound)?;
                    if transfer_active(&task.view.phase) {
                        task.cancel.store(true, Ordering::Release);
                        task.view.phase = if task.view.phase == "awaitingDecision" {
                            "cancelled"
                        } else {
                            "cancelling"
                        }
                        .into();
                    }
                    task.group.clone()
                };
                self.publish_transfer(owner, &id).await;
                self.stop_group_tools(&group).await;
                ToolResult::Done
            }
            ToolAction::ResolveConflict {
                id,
                version,
                decision,
                batch,
            } => {
                if !["skip", "overwrite", "cancel"].contains(&decision.as_str()) {
                    return Err(WorkspaceError::InvalidInput);
                }
                let (changed, restart, group) = {
                    let mut state = self.state.lock().unwrap();
                    let task = state
                        .transfers
                        .get(&id)
                        .filter(|t| t.owner == owner)
                        .ok_or(WorkspaceError::NotFound)?;
                    if task.view.phase != "awaitingDecision"
                        || task.view.conflict_version != version
                    {
                        return Err(WorkspaceError::Conflict);
                    }
                    let batch_id = task.view.batch_id.clone();
                    let group = task.group.clone();
                    if batch && decision != "cancel" {
                        *task.batch_policy.lock().unwrap() = Some(decision == "overwrite");
                    }
                    let mut changed = Vec::new();
                    let mut restart = Vec::new();
                    for task in state.transfers.values_mut().filter(|t| {
                        t.owner == owner
                            && t.view.phase == "awaitingDecision"
                            && (t.view.id == id
                                || (batch && decision != "cancel" && t.view.batch_id == batch_id))
                    }) {
                        task.view.error = None;
                        let tree_conflict = task.archive_paths.is_none()
                            && task
                                .tree
                                .lock()
                                .unwrap()
                                .entries
                                .iter()
                                .any(|e| e.phase == "conflict");
                        task.view.phase = if decision == "cancel" {
                            "cancelled"
                        } else if decision == "skip" && !tree_conflict {
                            "skipped"
                        } else {
                            "queued"
                        }
                        .into();
                        if task.view.phase == "queued" {
                            if tree_conflict {
                                task.tree.lock().unwrap().conflict_decision =
                                    Some(decision == "overwrite");
                            } else {
                                task.overwrite = decision == "overwrite";
                            }
                            task.view.attempt += 1;
                            task.progress.store(0, Ordering::Release);
                            task.total.store(u64::MAX, Ordering::Release);
                            restart.push(task.view.id.clone());
                        }
                        changed.push(task.view.id.clone());
                    }
                    (changed, restart, group)
                };
                for id in changed {
                    self.publish_transfer(owner, &id).await;
                }
                for id in restart {
                    self.begin_transfer(owner.into(), id);
                }
                self.stop_group_tools(&group).await;
                ToolResult::Done
            }
            ToolAction::TransferPage { request, cursor } => {
                let mut state = self.state.lock().unwrap();
                state
                    .transfer_pages
                    .retain(|_, p| p.touched.elapsed() < Duration::from_secs(60));
                let request = if cursor.is_none() {
                    if state
                        .transfer_pages
                        .values()
                        .filter(|p| p.owner == owner)
                        .count()
                        >= 4
                    {
                        return Err(WorkspaceError::Busy);
                    }
                    let id = new_id();
                    let tasks = state
                        .transfers
                        .values()
                        .filter(|t| t.owner == owner)
                        .map(|t| {
                            let mut v = t.view.clone();
                            v.can_retry = ["failed", "cancelled"].contains(&v.phase.as_str())
                                && t.group.refs.load(Ordering::Acquire) > 0;
                            v
                        })
                        .collect();
                    state.transfer_pages.insert(
                        id.clone(),
                        TransferSnapshot {
                            owner: owner.into(),
                            touched: Instant::now(),
                            tasks,
                        },
                    );
                    id
                } else {
                    request.ok_or(WorkspaceError::InvalidInput)?
                };
                let snapshot = state
                    .transfer_pages
                    .get_mut(&request)
                    .filter(|p| p.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                let offset = cursor
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<usize>()
                    .map_err(|_| WorkspaceError::InvalidInput)?;
                if offset > snapshot.tasks.len() {
                    return Err(WorkspaceError::InvalidInput);
                }
                let mut bytes = 1024;
                let mut tasks = Vec::new();
                for task in snapshot.tasks.iter().skip(offset).take(100) {
                    let size = transfer_size(task);
                    if bytes + size > 60 * 1024 {
                        if tasks.is_empty() {
                            return Err(WorkspaceError::FilesTooLarge);
                        }
                        break;
                    }
                    bytes += size;
                    tasks.push(task.clone());
                }
                let end = offset + tasks.len();
                let next = (end < snapshot.tasks.len()).then(|| end.to_string());
                snapshot.touched = Instant::now();
                if next.is_none() {
                    state.transfer_pages.remove(&request);
                }
                ToolResult::TransferPage {
                    request,
                    tasks,
                    next,
                }
            }
            ToolAction::RetryTransfer { id } => {
                let mut state = self.state.lock().unwrap();
                let task = state
                    .transfers
                    .get_mut(&id)
                    .filter(|t| t.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if !["failed", "cancelled"].contains(&task.view.phase.as_str())
                    || task.group.refs.load(Ordering::Acquire) == 0
                {
                    return Err(WorkspaceError::Disconnected);
                }
                {
                    let mut tree = task.tree.lock().unwrap();
                    tree.conflict_decision = None;
                    for entry in &mut tree.entries {
                        if entry.phase == "failed" {
                            entry.phase = "pending".into();
                            entry.error = None;
                        }
                    }
                }
                task.group.retry_credentials();
                task.cancel = Arc::new(AtomicBool::new(false));
                task.progress.store(0, Ordering::Release);
                task.total.store(u64::MAX, Ordering::Release);
                task.view.attempt += 1;
                task.view.phase = "queued".into();
                task.view.error = None;
                drop(state);
                self.begin_transfer(owner.into(), id);
                ToolResult::Done
            }
            ToolAction::Transfers => ToolResult::Transfers(
                self.state
                    .lock()
                    .unwrap()
                    .transfers
                    .values()
                    .filter(|t| t.owner == owner)
                    .map(|t| t.view.clone())
                    .collect(),
            ),
            _ => return Err(WorkspaceError::InvalidInput),
        };
        Ok(WorkspaceResult::Tools(reply))
    }
    pub(in crate::workspace) fn begin_transfer(self: &Arc<Self>, owner: String, id: String) {
        let service = self.clone();
        self.tasks.spawn(async move {
            service.publish_transfer(&owner, &id).await;
            let (group, io, tree, archive, policy) = {
                let state = service.state.lock().unwrap(); let task = &state.transfers[&id];
                (task.group.clone(), TransferIo { local: task.local.clone(), remote: task.remote.clone(), upload: task.view.direction == "upload", overwrite: task.overwrite, cancel: task.cancel.clone(), progress: task.progress.clone(), total: task.total.clone() }, task.tree.clone(), task.archive_paths.clone(), task.batch_policy.clone())
            };
            let cancel = io.cancel.clone();
            let result = async {
                let permit = loop {
                    if cancel.load(Ordering::Acquire) { return Err(WorkspaceError::Cancelled); }
                    tokio::select! { p = group.transfers.clone().acquire_owned() => break p.map_err(|_| WorkspaceError::Cancelled)?, _ = tokio::time::sleep(Duration::from_millis(100)) => {} }
                };
                service.state.lock().unwrap().transfers.get_mut(&id).unwrap().view.phase = "running".into();
                let client = service.tools_client(&group).await?;
                let transfer = if let Some(paths) = archive { client.archive(ArchiveIo { io, paths, state: tree }) } else { client.transfer_tree(TransferTreeIo { io, state: tree, batch_policy: policy }) }; tokio::pin!(transfer);
                let result = loop {
                    tokio::select! { result = &mut transfer => break result, _ = tokio::time::sleep(Duration::from_millis(200)) => { service.publish_transfer(&owner, &id).await; } }
                };
                drop(permit); result
            }.await;
            let restart = {
                let mut state = service.state.lock().unwrap();
                let task = state.transfers.get_mut(&id).unwrap();
                let mut restart = false;
                let all_skipped = { let tree = task.tree.lock().unwrap(); !tree.entries.is_empty() && tree.entries.iter().all(|e| e.phase == "skipped") };
                task.view.phase = if cancel.load(Ordering::Acquire) { "cancelled" } else { match result {
                    Ok(()) => if all_skipped { "skipped" } else { "complete" }, Err(WorkspaceError::Cancelled) => "cancelled",
                    Err(WorkspaceError::FilesExists) => {
                        task.view.conflict_version += 1;
                        match *task.batch_policy.lock().unwrap() {
                            Some(false) if task.archive_paths.is_some() || task.tree.lock().unwrap().entries.is_empty() => "skipped",
                            Some(choice) => { if task.archive_paths.is_none() { task.tree.lock().unwrap().conflict_decision = Some(choice); } else { task.overwrite = choice; } task.view.attempt += 1; task.progress.store(0, Ordering::Release); task.total.store(u64::MAX, Ordering::Release); restart = true; "queued" },
                            None => "awaitingDecision",
                        }
                    }, Err(_) => "failed",
                }}.into();
                task.view.error = if task.view.phase == "failed" { result.err().map(|e| e.code().into()) } else { None };
                restart
            };
            if restart { service.begin_transfer(owner.clone(), id.clone()); }
            if matches!(result, Err(WorkspaceError::ToolsUnavailable | WorkspaceError::Timeout | WorkspaceError::Disconnected)) {
                let other_active = service.state.lock().unwrap().transfers.values().any(|t| Arc::ptr_eq(&t.group, &group) && transfer_active(&t.view.phase));
                if !other_active && let Some(client) = group.tools.client.lock().await.take() { let _ = client.close().await; }
            }
            service.publish_transfer(&owner, &id).await;
            if group.refs.load(Ordering::Acquire) == 0 { service.stop_group_tools(&group).await; }
            let _ = service.recover_credentials().await;
            service.changed(&owner).await;
        });
    }
    pub(in crate::workspace) async fn publish_transfer(&self, owner: &str, id: &str) {
        let view = {
            let mut state = self.state.lock().unwrap();
            state.transfers.get_mut(id).map(|t| {
                let tree = t.tree.lock().unwrap();
                t.view.conflict = if t.view.phase == "awaitingDecision" {
                    conflict_paths(
                        &t.view.direction,
                        &tree,
                        t.archive_paths.is_some(),
                        &t.local,
                    )
                } else {
                    None
                };
                t.view.details = Some(TransferSummary {
                    stage: tree.stage.clone(),
                    completed: tree
                        .entries
                        .iter()
                        .filter(|e| e.phase == "complete")
                        .count() as u32,
                    failed: tree.entries.iter().filter(|e| e.phase == "failed").count() as u32,
                    skipped: tree.entries.iter().filter(|e| e.phase == "skipped").count() as u32,
                    pending: tree
                        .entries
                        .iter()
                        .filter(|e| ["pending", "conflict"].contains(&e.phase.as_str()))
                        .count() as u32,
                });
                t.view.bytes = t.progress.load(Ordering::Acquire);
                let total = t.total.load(Ordering::Acquire);
                t.view.total = (total != u64::MAX).then_some(total);
                t.view.revision += 1;
                t.view.can_retry = ["failed", "cancelled"].contains(&t.view.phase.as_str())
                    && t.group.refs.load(Ordering::Acquire) > 0;
                t.view.clone()
            })
        };
        if let Some(view) = view {
            let _ = self
                .sink
                .event(owner.into(), WorkspaceNotice::Transfer(Box::new(view)))
                .await;
        }
    }
}

// Read the exact conflicting child from the frozen manifest; archive sources are not a single path.
fn conflict_paths(
    direction: &str,
    tree: &crate::ports::TransferTreeState,
    archive: bool,
    local: &std::path::Path,
) -> Option<TransferConflict> {
    tree.entries
        .iter()
        .find(|entry| entry.phase == "conflict")
        .map(|entry| {
            let local = entry.local.to_string_lossy().into_owned();
            let (source, target) = if direction == "upload" {
                (local, entry.remote.clone())
            } else {
                (entry.remote.clone(), local)
            };
            TransferConflict {
                source_path: Some(source),
                target_path: target,
            }
        })
        .or_else(|| {
            archive.then(|| TransferConflict {
                source_path: None,
                target_path: local.to_string_lossy().into_owned(),
            })
        })
}

#[cfg(test)]
mod conflict_tests {
    use super::*;
    #[test]
    fn conflict_paths_use_the_child_and_preserve_upload_download_direction() {
        let mut tree = crate::ports::TransferTreeState::default();
        assert!(
            conflict_paths(
                "download",
                &tree,
                false,
                std::path::Path::new("/local/root")
            )
            .is_none()
        );
        tree.entries.push(crate::ports::TransferTreeEntry {
            local: "/local/root/nested/file.txt".into(),
            remote: "/remote/root/nested/file.txt".into(),
            source: RemoteEntry {
                name: "file.txt".into(),
                path: "/remote/root/nested/file.txt".into(),
                kind: "file".into(),
                size: None,
                modified: None,
                permissions: None,
                uid: None,
                gid: None,
            },
            phase: "conflict".into(),
            error: None,
            transferred: 0,
        });
        let upload =
            conflict_paths("upload", &tree, false, std::path::Path::new("/local/root")).unwrap();
        assert_eq!(
            upload.source_path.as_deref(),
            Some("/local/root/nested/file.txt")
        );
        assert_eq!(upload.target_path, "/remote/root/nested/file.txt");
        let download = conflict_paths(
            "download",
            &tree,
            false,
            std::path::Path::new("/local/root"),
        )
        .unwrap();
        assert_eq!(
            download.source_path.as_deref(),
            Some("/remote/root/nested/file.txt")
        );
        assert_eq!(download.target_path, "/local/root/nested/file.txt");
        tree.entries[0].phase = "complete".into();
        assert!(
            conflict_paths(
                "download",
                &tree,
                false,
                std::path::Path::new("/local/root")
            )
            .is_none()
        );
        let archive = conflict_paths(
            "download",
            &tree,
            true,
            std::path::Path::new("/local/selection.tar.gz"),
        )
        .unwrap();
        assert!(archive.source_path.is_none());
        assert_eq!(archive.target_path, "/local/selection.tar.gz");
    }
}
