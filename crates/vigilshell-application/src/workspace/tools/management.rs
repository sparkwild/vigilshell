use super::*;

pub(in crate::workspace) struct FileJob {
    pub group: Arc<RuntimeGroup>,
    pub(super) owner: String,
    roots: Vec<String>,
    operation: String,
    recursive: bool,
    set_mask: u32,
    clear_mask: u32,
    pub phase: String,
    stage: String,
    error: Option<WorkspaceError>,
    items: Vec<FileOperationItem>,
    scanned: bool,
    scan_failures: HashSet<String>,
    attempt: u32,
    started: std::time::Instant,
}
fn checked_path(path: &str) -> Result<(), WorkspaceError> {
    valid_remote_path(path)?;
    if !path.starts_with('/') || path.split('/').any(|p| p == "." || p == "..") {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(())
}
fn same_entry(a: &RemoteEntry, b: &RemoteEntry) -> bool {
    a.path == b.path
        && a.kind == b.kind
        && a.uid == b.uid
        && a.gid == b.gid
        && a.permissions == b.permissions
        && (a.kind == "directory" || (a.size == b.size && a.modified == b.modified))
}
async fn parents(client: &dyn ServerTools, path: &str) -> Result<(), WorkspaceError> {
    let mut parent = String::new();
    let parts: Vec<_> = path.split('/').filter(|s| !s.is_empty()).collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        parent.push('/');
        parent.push_str(part);
        if client.stat(parent.clone()).await?.kind != "directory" {
            return Err(WorkspaceError::FilesUnsupported);
        }
    }
    Ok(())
}
impl WorkspaceService {
    pub(super) async fn file_management(
        self: &Arc<Self>,
        owner: &str,
        action: ToolAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        match action {
            ToolAction::Stat { session, path } => {
                checked_path(&path)?;
                let group = self.tool_group(owner, &session)?;
                Ok(WorkspaceResult::Tools(ToolResult::FileEntry(
                    self.tools_client(&group).await?.stat(path).await?,
                )))
            }
            ToolAction::CreateFile { session, path } => {
                checked_path(&path)?;
                if path == "/" {
                    return Err(WorkspaceError::InvalidInput);
                }
                let group = self.tool_group(owner, &session)?;
                let client = self.tools_client(&group).await?;
                parents(client.as_ref(), &path).await?;
                client.create_file(path).await?;
                Ok(WorkspaceResult::Tools(ToolResult::Done))
            }
            ToolAction::StartFileOperation {
                session,
                paths,
                operation,
                recursive,
                set_mask,
                clear_mask,
            } => {
                if paths.is_empty()
                    || paths.len() > 100
                    || !["remove", "permissions"].contains(&operation.as_str())
                    || set_mask > 0o777
                    || clear_mask > 0o777
                    || set_mask & clear_mask != 0
                {
                    return Err(WorkspaceError::InvalidInput);
                }
                for path in &paths {
                    checked_path(path)?;
                    if path.trim_end_matches('/').is_empty() && (operation == "remove" || recursive)
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                }
                let group = self.tool_group(owner, &session)?;
                let mut op = self.begin(owner, "files", true)?;
                op.session_id = Some(session);
                op.file_summary = Some(
                    vigilshell_application_contracts::workspace::FileOperationSummary {
                        connection_id: group.profile.id.clone(),
                        attempt: 0,
                        elapsed_ms: 0,
                        completed: 0,
                        failed: 0,
                        skipped: 0,
                    },
                );
                {
                    let mut state = self.state.lock().unwrap();
                    state.operations.get_mut(&op.id).unwrap().view = op.clone();
                    let retained: HashSet<_> = state.operations.keys().cloned().collect();
                    state
                        .file_jobs
                        .retain(|id, j| j.phase == "working" || retained.contains(id));
                    state.file_jobs.insert(
                        op.id.clone(),
                        FileJob {
                            group,
                            owner: owner.into(),
                            roots: paths,
                            operation,
                            recursive,
                            set_mask,
                            clear_mask,
                            phase: "working".into(),
                            stage: "scanning".into(),
                            error: None,
                            items: vec![],
                            scanned: false,
                            scan_failures: HashSet::new(),
                            attempt: 0,
                            started: std::time::Instant::now(),
                        },
                    );
                }
                self.update(&op.id, |_| {}).await;
                self.run_file_job(op.id.clone());
                Ok(WorkspaceResult::Operation(Box::new(op)))
            }
            ToolAction::FileOperation { id, offset } => {
                let state = self.state.lock().unwrap();
                let job = state
                    .file_jobs
                    .get(&id)
                    .filter(|j| j.owner == owner)
                    .ok_or(WorkspaceError::NotFound)?;
                if offset as usize > job.items.len() {
                    return Err(WorkspaceError::InvalidInput);
                }
                let mut items = Vec::new();
                let mut bytes = 1024;
                for item in job.items.iter().skip(offset as usize).take(100) {
                    let size = 512
                        + json_string_size(&item.entry.path)
                        + json_string_size(&item.entry.name);
                    if bytes + size > 60 * 1024 {
                        break;
                    }
                    bytes += size;
                    items.push(item.clone());
                }
                if items.is_empty() && (offset as usize) < job.items.len() {
                    return Err(WorkspaceError::FilesTooLarge);
                }
                let next = offset as usize + items.len();
                let count = |status| job.items.iter().filter(|i| i.status == status).count() as u32;
                Ok(WorkspaceResult::Tools(ToolResult::FileOperation(
                    FileOperationPage {
                        id,
                        phase: job.phase.clone(),
                        stage: job.stage.clone(),
                        error: job.error,
                        completed: count("complete"),
                        failed: count("failed"),
                        skipped: count("skipped"),
                        pending: count("pending"),
                        items,
                        next: (next < job.items.len()).then_some(next as u32),
                    },
                )))
            }
            ToolAction::RetryFileOperation { id } => {
                let view = {
                    let mut state = self.state.lock().unwrap();
                    state
                        .operations
                        .get(&id)
                        .filter(|o| o.owner == owner)
                        .ok_or(WorkspaceError::NotFound)?;
                    let job = state
                        .file_jobs
                        .get_mut(&id)
                        .filter(|j| j.owner == owner)
                        .ok_or(WorkspaceError::NotFound)?;
                    if !["failed", "cancelled"].contains(&job.phase.as_str()) {
                        return Err(WorkspaceError::Conflict);
                    }
                    job.attempt += 1;
                    job.started = std::time::Instant::now();
                    let attempt = job.attempt;
                    job.phase = "working".into();
                    job.stage = "scanning".into();
                    job.error = None;
                    let op = state
                        .operations
                        .get_mut(&id)
                        .ok_or(WorkspaceError::NotFound)?;
                    op.flag.store(false, Ordering::Release);
                    op.cancel = CancellationToken::new();
                    if let Some(summary) = op.view.file_summary.as_mut() {
                        summary.attempt = attempt;
                        summary.elapsed_ms = 0;
                    }
                    op.view.phase = "working".into();
                    op.view.error = None;
                    op.view.clone()
                };
                self.update(&id, |_| {}).await;
                self.run_file_job(id);
                Ok(WorkspaceResult::Operation(Box::new(view)))
            }
            _ => Err(WorkspaceError::InvalidInput),
        }
    }
    fn run_file_job(self: &Arc<Self>, id: String) {
        let service = self.clone();
        self.tasks.spawn(async move {
            let (group, cancel) = {
                let state = service.state.lock().unwrap();
                (
                    state.file_jobs[&id].group.clone(),
                    state.operations[&id].flag.clone(),
                )
            };
            let result = service.execute_file_job(&id, &group, cancel).await;
            {
                let mut state = service.state.lock().unwrap();
                if let Some(job) = state.file_jobs.get_mut(&id) {
                    job.phase = match result {
                        Ok(()) => "success",
                        Err(WorkspaceError::Cancelled) => "cancelled",
                        Err(_) => "failed",
                    }
                    .into();
                    job.stage = "finished".into();
                    job.error = result.err();
                }
            }
            {
                let mut state = service.state.lock().unwrap();
                let summary = state.file_jobs.get(&id).map(|job| {
                    vigilshell_application_contracts::workspace::FileOperationSummary {
                        connection_id: job.group.profile.id.clone(),
                        attempt: job.attempt,
                        elapsed_ms: job.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        completed: job.items.iter().filter(|i| i.status == "complete").count()
                            as u32,
                        failed: job.items.iter().filter(|i| i.status == "failed").count() as u32,
                        skipped: job.items.iter().filter(|i| i.status == "skipped").count() as u32,
                    }
                });
                if let Some(op) = state.operations.get_mut(&id) {
                    op.view.file_summary = summary;
                }
            }
            service.finish(&id, result).await;
            if group.refs.load(Ordering::Acquire) == 0 {
                service.stop_group_tools(&group).await;
            }
        });
    }
    async fn execute_file_job(
        &self,
        id: &str,
        group: &Arc<RuntimeGroup>,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), WorkspaceError> {
        let client = self.tools_client(group).await?;
        let (roots, recursive, operation, set_mask, clear_mask, mut items) = {
            let state = self.state.lock().unwrap();
            let job = &state.file_jobs[id];
            let roots = if job.scanned {
                job.scan_failures
                    .iter()
                    .map(|path| {
                        let depth = job
                            .roots
                            .iter()
                            .filter(|root| {
                                path == *root
                                    || path
                                        .starts_with(&(root.trim_end_matches('/').to_owned() + "/"))
                            })
                            .map(|root| {
                                path.split('/')
                                    .filter(|s| !s.is_empty())
                                    .count()
                                    .saturating_sub(
                                        root.split('/').filter(|s| !s.is_empty()).count(),
                                    )
                            })
                            .min()
                            .unwrap_or(0);
                        (path.clone(), depth)
                    })
                    .collect::<Vec<_>>()
            } else {
                job.roots.iter().cloned().map(|path| (path, 0)).collect()
            };
            let items = job
                .items
                .iter()
                .filter(|i| !job.scan_failures.contains(&i.entry.path))
                .cloned()
                .map(|mut i| {
                    if i.status == "failed" {
                        i.status = "pending".into();
                        i.error = None;
                    }
                    i
                })
                .collect::<Vec<_>>();
            (
                roots,
                job.recursive,
                job.operation.clone(),
                job.set_mask,
                job.clear_mask,
                items,
            )
        };
        // Already enumerated branches are frozen. Retry only failed writes or branches
        // whose enumeration never succeeded, never newly created children of a scanned branch.
        let mut seen: HashSet<_> = items.iter().map(|i| i.entry.path.clone()).collect();
        let mut scan_failures = HashSet::new();
        let mut bytes: usize = items.iter().map(|i| i.entry.path.len() + 256).sum();
        let mut pending = roots;
        bytes += pending.iter().map(|(p, _)| p.len() + 256).sum::<usize>();
        while let Some((path, depth)) = pending.pop() {
            if cancel.load(Ordering::Acquire) {
                return Err(WorkspaceError::Cancelled);
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            if depth > 64 || seen.len() > 20000 || bytes > 4 * 1024 * 1024 {
                return Err(WorkspaceError::FilesTooLarge);
            }
            let entry = match async {
                parents(client.as_ref(), &path).await?;
                client.stat(path.clone()).await
            }
            .await
            {
                Ok(e) => e,
                Err(e) => {
                    scan_failures.insert(path.clone());
                    items.push(FileOperationItem {
                        entry: RemoteEntry {
                            name: path.rsplit('/').next().unwrap_or("/").into(),
                            path,
                            kind: "other".into(),
                            size: None,
                            modified: None,
                            permissions: None,
                            uid: None,
                            gid: None,
                        },
                        status: "failed".into(),
                        error: Some(e),
                    });
                    continue;
                }
            };
            let mut item = FileOperationItem {
                entry: entry.clone(),
                status: "pending".into(),
                error: None,
            };
            if entry.kind == "other" || (operation == "permissions" && entry.kind == "symlink") {
                item.status = "skipped".into();
            }
            if entry.kind == "directory" && recursive {
                match client.list(path.clone(), cancel.clone()).await {
                    Ok((canonical, entries)) if canonical == path => {
                        for child in entries {
                            if seen.len() + pending.len() >= 20000 {
                                return Err(WorkspaceError::FilesTooLarge);
                            }
                            bytes += child.path.len() + 256;
                            if bytes > 4 * 1024 * 1024 {
                                return Err(WorkspaceError::FilesTooLarge);
                            }
                            pending.push((child.path, depth + 1));
                        }
                    }
                    Ok(_) => {
                        scan_failures.insert(path.clone());
                        item.status = "failed".into();
                        item.error = Some(WorkspaceError::Conflict);
                    }
                    Err(e) => {
                        scan_failures.insert(path.clone());
                        item.status = "failed".into();
                        item.error = Some(e);
                    }
                }
            }
            items.push(item);
        }
        // Complete the bounded scan before applying any mutation, including on retries.
        items.sort_by_key(|i| std::cmp::Reverse(i.entry.path.matches('/').count()));
        {
            let mut state = self.state.lock().unwrap();
            let job = state.file_jobs.get_mut(id).unwrap();
            job.scanned = true;
            job.scan_failures = scan_failures;
            job.items = items;
            job.stage = "applying".into();
        }
        let len = self.state.lock().unwrap().file_jobs[id].items.len();
        for index in 0..len {
            if cancel.load(Ordering::Acquire) {
                return Err(WorkspaceError::Cancelled);
            }
            let item = self.state.lock().unwrap().file_jobs[id].items[index].clone();
            if item.status != "pending" {
                continue;
            }
            let result = async {
                parents(client.as_ref(), &item.entry.path).await?;
                let now = client.stat(item.entry.path.clone()).await?;
                if !same_entry(&item.entry, &now) {
                    return Err(WorkspaceError::Conflict);
                }
                if operation == "remove" {
                    client.remove(now.path, now.kind == "directory").await
                } else {
                    client
                        .permissions(now.path.clone(), set_mask, clear_mask, now)
                        .await
                }
            }
            .await;
            let mut state = self.state.lock().unwrap();
            let item = &mut state.file_jobs.get_mut(id).unwrap().items[index];
            item.status = if result.is_ok() { "complete" } else { "failed" }.into();
            item.error = result.err();
        }
        let state = self.state.lock().unwrap();
        state.file_jobs[id]
            .items
            .iter()
            .find_map(|i| i.error)
            .map_or(Ok(()), Err)
    }
}
