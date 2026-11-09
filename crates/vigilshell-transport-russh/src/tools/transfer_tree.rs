use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64},
};
use vigilshell_application_contracts::ports::{
    TransferTreeEntry, TransferTreeIo, TransferTreeState,
};

fn stopped(cancel: &AtomicBool) -> Result<(), WorkspaceError> {
    if cancel.load(Ordering::Acquire) {
        Err(WorkspaceError::Cancelled)
    } else {
        Ok(())
    }
}
fn local_entry(path: &std::path::Path, metadata: &std::fs::Metadata) -> RemoteEntry {
    RemoteEntry {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: if metadata.is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        }
        .into(),
        size: Some(metadata.len()),
        modified: metadata
            .modified()
            .ok()
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|v| v.as_secs()),
        permissions: None,
        uid: None,
        gid: None,
    }
}
pub(super) async fn remote_entry(
    sftp: &Arc<RawSftpSession>,
    path: &str,
) -> Result<RemoteEntry, WorkspaceError> {
    valid_remote_path(path)?;
    let attrs = sftp.lstat(path).await.map_err(sftp_error)?.attrs;
    Ok(RemoteEntry {
        name: path.rsplit('/').next().unwrap_or("").into(),
        path: path.into(),
        kind: if attrs.is_symlink() {
            "symlink"
        } else if attrs.is_dir() {
            "directory"
        } else if attrs.is_regular() {
            "file"
        } else {
            "other"
        }
        .into(),
        size: attrs.size,
        modified: attrs.mtime.map(u64::from),
        permissions: attrs.permissions,
        uid: attrs.uid,
        gid: attrs.gid,
    })
}
fn same(a: &RemoteEntry, b: &RemoteEntry) -> bool {
    a.kind == b.kind && (a.kind != "file" || (a.size == b.size && a.modified == b.modified))
}
pub(super) async fn remote_parents(
    sftp: &Arc<RawSftpSession>,
    path: &str,
) -> Result<(), WorkspaceError> {
    if !path.starts_with('/') || path.split('/').any(|s| s == ".." || s == ".") {
        return Err(WorkspaceError::InvalidInput);
    }
    let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
    let mut parent = String::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        parent.push('/');
        parent.push_str(part);
        let entry = remote_entry(sftp, &parent).await?;
        if entry.kind != "directory" {
            return Err(WorkspaceError::FilesUnsupported);
        }
    }
    Ok(())
}
pub(super) async fn local_root(
    path: &std::path::Path,
) -> Result<std::path::PathBuf, WorkspaceError> {
    if !path.is_absolute() {
        return Err(WorkspaceError::InvalidInput);
    }
    let Some(name) = path.file_name() else {
        return tokio::fs::canonicalize(path).await.map_err(local_error);
    };
    let parent = tokio::fs::canonicalize(path.parent().ok_or(WorkspaceError::InvalidInput)?)
        .await
        .map_err(local_error)?;
    Ok(parent.join(name))
}
fn child_path(parent: &std::path::Path, name: &str) -> Result<std::path::PathBuf, WorkspaceError> {
    let mut parts = std::path::Path::new(name).components();
    if !matches!(parts.next(), Some(std::path::Component::Normal(_))) || parts.next().is_some() {
        return Err(WorkspaceError::FilesUnsupported);
    }
    Ok(parent.join(name))
}
async fn local_parents(path: &std::path::Path) -> Result<(), WorkspaceError> {
    let mut parent = std::path::PathBuf::new();
    let parts: Vec<_> = path.components().collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        parent.push(part.as_os_str());
        let attrs = tokio::fs::symlink_metadata(&parent)
            .await
            .map_err(local_error)?;
        if !attrs.is_dir() || attrs.is_symlink() {
            return Err(WorkspaceError::FilesUnsupported);
        }
    }
    Ok(())
}
pub(super) async fn enumerate(
    sftp: &Arc<RawSftpSession>,
    local: std::path::PathBuf,
    remote: String,
    upload: bool,
    cancel: &AtomicBool,
) -> Result<Vec<TransferTreeEntry>, WorkspaceError> {
    let mut queue = vec![(local, remote, 0usize)];
    let mut entries = Vec::new();
    let mut bytes = 0usize;
    while let Some((local, remote, depth)) = queue.pop() {
        stopped(cancel)?;
        if depth > 64 || entries.len() >= 20_000 {
            return Err(WorkspaceError::FilesTooLarge);
        }
        let source = if upload {
            local_parents(&local).await?;
            local_entry(
                &local,
                &tokio::fs::symlink_metadata(&local)
                    .await
                    .map_err(local_error)?,
            )
        } else {
            remote_parents(sftp, &remote).await?;
            remote_entry(sftp, &remote).await?
        };
        bytes += local.as_os_str().len() + remote.len() + 256;
        if bytes > 16 * 1024 * 1024 {
            return Err(WorkspaceError::FilesTooLarge);
        }
        if source.kind == "directory" {
            if upload {
                let mut directory = tokio::fs::read_dir(&local).await.map_err(local_error)?;
                while let Some(child) = directory.next_entry().await.map_err(local_error)? {
                    let name = child
                        .file_name()
                        .into_string()
                        .map_err(|_| WorkspaceError::FilesUnsupported)?;
                    if queue.len() + entries.len() >= 20_000 {
                        return Err(WorkspaceError::FilesTooLarge);
                    }
                    queue.push((
                        child.path(),
                        format!("{}/{}", remote.trim_end_matches('/'), name),
                        depth + 1,
                    ));
                }
            } else {
                let (canonical, children) = crate::sftp::list(sftp, remote.clone(), cancel).await?;
                if canonical.trim_end_matches('/') != remote.trim_end_matches('/') {
                    return Err(WorkspaceError::Conflict);
                }
                for child in children.into_iter().rev() {
                    if queue.len() + entries.len() >= 20_000 {
                        return Err(WorkspaceError::FilesTooLarge);
                    }
                    queue.push((child_path(&local, &child.name)?, child.path, depth + 1));
                }
            }
        }
        let supported = ["file", "directory"].contains(&source.kind.as_str());
        entries.push(TransferTreeEntry {
            local,
            remote,
            source,
            phase: if supported { "pending" } else { "skipped" }.into(),
            error: (!supported).then(|| "files_unsupported".into()),
            transferred: 0,
        });
    }
    Ok(entries)
}
fn update(
    state: &Mutex<TransferTreeState>,
    index: usize,
    phase: &str,
    error: Option<&WorkspaceError>,
) {
    let mut state = state.lock().unwrap();
    state.entries[index].phase = phase.into();
    state.entries[index].error = error.map(|e| e.code().into());
}
pub(super) async fn run(tools: &Tools, tree: TransferTreeIo) -> Result<(), WorkspaceError> {
    let (sftp, replace) = tools.sftp().await?;
    run_session(sftp, *replace, tree).await
}
pub(super) async fn run_session(
    sftp: &Arc<RawSftpSession>,
    replace: bool,
    tree: TransferTreeIo,
) -> Result<(), WorkspaceError> {
    let mut io = tree.io;
    io.local = local_root(&io.local).await?;
    let need_enumeration = !tree.state.lock().unwrap().enumerated;
    if need_enumeration {
        tree.state.lock().unwrap().stage = "enumerating".into();
        let entries = enumerate(
            sftp,
            io.local.clone(),
            io.remote.clone(),
            io.upload,
            &io.cancel,
        )
        .await?;
        let mut state = tree.state.lock().unwrap();
        state.entries = entries;
        state.enumerated = true;
    }
    tree.state.lock().unwrap().stage = "transferring".into();
    let total = tree
        .state
        .lock()
        .unwrap()
        .entries
        .iter()
        .filter(|e| e.source.kind == "file" && e.phase != "skipped")
        .try_fold(0u64, |n, e| n.checked_add(e.source.size.unwrap_or(0)))
        .ok_or(WorkspaceError::FilesTooLarge)?;
    io.total.store(total, Ordering::Release);
    let count = tree.state.lock().unwrap().entries.len();
    for index in 0..count {
        stopped(&io.cancel)?;
        let entry = tree.state.lock().unwrap().entries[index].clone();
        if ["complete", "skipped"].contains(&entry.phase.as_str()) {
            continue;
        }
        let base: u64 = tree
            .state
            .lock()
            .unwrap()
            .entries
            .iter()
            .map(|e| e.transferred)
            .sum();
        io.progress.store(base, Ordering::Release);
        let result = async {
            let actual = if io.upload { local_parents(&entry.local).await?; local_entry(&entry.local, &tokio::fs::symlink_metadata(&entry.local).await.map_err(local_error)?) } else { remote_parents(sftp, &entry.remote).await?; remote_entry(sftp, &entry.remote).await? };
            if !same(&entry.source, &actual) { return Err(WorkspaceError::Conflict); }
            if io.upload { remote_parents(sftp, &entry.remote).await?; } else { local_parents(&entry.local).await?; }
            if actual.kind == "directory" {
                if io.upload {
                    match remote_entry(sftp, &entry.remote).await {
                        Ok(v) if v.kind == "directory" => {},
                        Ok(_) => return Err(WorkspaceError::FilesUnsupported),
                        Err(WorkspaceError::NotFound) => { sftp.mkdir(entry.remote.clone(), crate::sftp::empty_attrs()).await.map_err(sftp_error)?; },
                        Err(e) => return Err(e),
                    }
                } else {
                    match tokio::fs::symlink_metadata(&entry.local).await {
                        Ok(v) if v.is_dir() && !v.is_symlink() => {},
                        Ok(_) => return Err(WorkspaceError::FilesUnsupported),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => tokio::fs::create_dir(&entry.local).await.map_err(local_error)?,
                        Err(e) => return Err(local_error(e)),
                    }
                }
                return Ok(());
            }
            let decision = if entry.phase == "conflict" { tree.state.lock().unwrap().conflict_decision.take() } else { None }.or(*tree.batch_policy.lock().unwrap());
            if entry.phase == "conflict" && decision == Some(false) { update(&tree.state, index, "skipped", None); return Ok(()); }
            let progress = Arc::new(AtomicU64::new(0));
            let item = TransferIo { local: entry.local.clone(), remote: entry.remote.clone(), upload: io.upload, overwrite: decision.unwrap_or(io.overwrite), cancel: io.cancel.clone(), progress: progress.clone(), total: Arc::new(AtomicU64::new(u64::MAX)) };
            let transfer = transfer_file(sftp, replace, item); tokio::pin!(transfer);
            let result = loop { tokio::select! { r = &mut transfer => break r, _ = tokio::time::sleep(Duration::from_millis(100)) => { let done = progress.load(Ordering::Acquire);
                io.progress.store(base.saturating_add(done), Ordering::Release);
                tree.state.lock().unwrap().stage = if actual.size.is_some_and(|size| done >= size) { "verifying" } else { "transferring" }.into(); } } };
            if result.is_ok() { tree.state.lock().unwrap().entries[index].transferred = progress.load(Ordering::Acquire); }
            result
        }.await;
        match result {
            Ok(()) => {
                if tree.state.lock().unwrap().entries[index].phase != "skipped" {
                    update(&tree.state, index, "complete", None);
                }
            }
            Err(WorkspaceError::FilesExists) => {
                update(&tree.state, index, "conflict", None);
                return Err(WorkspaceError::FilesExists);
            }
            Err(WorkspaceError::Cancelled) => return Err(WorkspaceError::Cancelled),
            Err(error) => update(&tree.state, index, "failed", Some(&error)),
        }
    }
    let mut state = tree.state.lock().unwrap();
    state.stage = "finished".into();
    io.progress.store(
        state.entries.iter().map(|e| e.transferred).sum(),
        Ordering::Release,
    );
    if state.entries.iter().any(|e| e.phase == "failed") {
        Err(WorkspaceError::ToolsUnavailable)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn resolves_authorized_parent_alias_without_following_selected_link() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        std::os::unix::fs::symlink("missing", real.join("selected-link")).unwrap();
        let resolved = local_root(&alias.join("selected-link")).await.unwrap();
        assert_eq!(resolved, real.canonicalize().unwrap().join("selected-link"));
        assert!(
            tokio::fs::symlink_metadata(resolved)
                .await
                .unwrap()
                .is_symlink()
        );
    }
}
