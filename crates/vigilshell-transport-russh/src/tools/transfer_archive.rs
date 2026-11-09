use super::*;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use vigilshell_application_contracts::ports::{ArchiveIo, TransferTreeEntry};
use vigilshell_domain::agent::shell_quote;

fn common_parent(paths: &[String]) -> Result<String, WorkspaceError> {
    let first = paths.first().ok_or(WorkspaceError::InvalidInput)?;
    let mut parent = Path::new(first)
        .parent()
        .unwrap_or(Path::new("/"))
        .to_path_buf();
    for path in paths.iter().skip(1) {
        while !Path::new(path).starts_with(&parent) {
            if !parent.pop() {
                return Err(WorkspaceError::InvalidInput);
            }
        }
    }
    Ok(if parent.as_os_str().is_empty() {
        "/".into()
    } else {
        parent.to_string_lossy().into_owned()
    })
}
fn archive_manifest(entries: &[TransferTreeEntry], base: &str) -> Result<Vec<u8>, WorkspaceError> {
    let mut input = Vec::new();
    for entry in entries.iter().filter(|e| e.phase != "skipped") {
        let relative = Path::new(&entry.remote)
            .strip_prefix(base)
            .map_err(|_| WorkspaceError::InvalidInput)?;
        let relative = relative.to_str().ok_or(WorkspaceError::FilesUnsupported)?;
        input.extend_from_slice(b"./");
        input.extend_from_slice(relative.as_bytes());
        input.push(0);
    }
    Ok(input)
}
pub(super) async fn run(tools: &Tools, archive: ArchiveIo) -> Result<(), WorkspaceError> {
    let mut io = archive.io;
    io.local = super::transfer_tree::local_root(&io.local).await?;
    let (sftp, _) = tools.sftp().await?;
    archive.state.lock().unwrap().stage = "enumerating".into();
    let mut paths = archive.paths.clone();
    paths.sort();
    paths.dedup();
    let roots: Vec<_> = paths
        .iter()
        .filter(|p| {
            !paths.iter().any(|other| {
                *p != other && p.starts_with(&format!("{}/", other.trim_end_matches('/')))
            })
        })
        .cloned()
        .collect();
    let mut entries = Vec::new();
    for remote in &roots {
        let mut branch = super::transfer_tree::enumerate(
            sftp,
            PathBuf::from("/unused"),
            remote.clone(),
            false,
            &io.cancel,
        )
        .await?;
        if entries.len() + branch.len() > 20_000 {
            return Err(WorkspaceError::FilesTooLarge);
        }
        entries.append(&mut branch);
    }
    {
        let mut state = archive.state.lock().unwrap();
        state.entries = entries.clone();
        state.enumerated = true;
    }
    if entries.iter().all(|e| e.phase == "skipped") {
        archive.state.lock().unwrap().stage = "finished".into();
        return Ok(());
    }
    if !tools
        .exec("LC_ALL=C tar --version")
        .await?
        .contains("GNU tar")
    {
        return Err(WorkspaceError::FilesUnsupported);
    }
    let destination = match tokio::fs::symlink_metadata(&io.local).await {
        Ok(m) if !m.is_file() || m.is_symlink() => return Err(WorkspaceError::FilesUnsupported),
        Ok(_) if !io.overwrite => return Err(WorkspaceError::FilesExists),
        Ok(m) => Some((m.len(), m.modified().ok())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(local_error(e)),
    };
    let base = common_parent(&roots)?;
    let input = archive_manifest(&entries, &base)?;
    let parent = io.local.parent().ok_or(WorkspaceError::InvalidInput)?;
    let temporary = parent.join(format!(".vigilshell-{}.part", new_id()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await
        .map_err(local_error)?;
    let result = async {
        archive.state.lock().unwrap().stage = "archiving".into();
        let channel = deadline(tools.handle.channel_open_session(), 10).await?;
        let command = format!("LC_ALL=C tar --format=posix --null --verbatim-files-from --no-recursion -czf - -C {} -T -", shell_quote(&base));
        channel.exec(true, command).await.map_err(error)?;
        let (mut reader, writer) = channel.split();
        let result = {
        let exchange = async {
            let send = async { writer.data(input.as_slice()).await.map_err(error)?; writer.eof().await.map_err(error) };
            let receive = async {
                let mut exit = None;
                loop {
                    let event = tokio::time::timeout(Duration::from_secs(30), reader.wait()).await.map_err(|_| WorkspaceError::Timeout)?;
                    match event {
                        Some(ChannelMsg::Data { data }) => {
                            file.write_all(&data).await.map_err(local_error)?;
                            io.progress.fetch_add(data.len() as u64, Ordering::AcqRel);
                        },
                        Some(ChannelMsg::ExtendedData { .. }) => {},
                        Some(ChannelMsg::ExitStatus { exit_status }) => exit = Some(exit_status),
                        Some(ChannelMsg::Failure | ChannelMsg::ExitSignal { .. }) => return Err(WorkspaceError::ToolsUnavailable),
                        Some(ChannelMsg::Close) | None => break,
                        _ => {},
                    }
                }
                if exit == Some(0) { Ok(()) } else { Err(WorkspaceError::ToolsUnavailable) }
            };
            tokio::try_join!(send, receive).map(|_| ())
        };
        tokio::pin!(exchange);
        loop { tokio::select! { result = &mut exchange => break result, _ = tokio::time::sleep(Duration::from_millis(100)) => {
            if io.cancel.load(Ordering::Acquire) { break Err(WorkspaceError::Cancelled); }
        } } }
        };
        let _ = writer.close().await;
        result?;
        archive.state.lock().unwrap().stage = "verifying".into();
        for entry in entries.iter().filter(|e| e.phase != "skipped") {
            if io.cancel.load(Ordering::Acquire) { return Err(WorkspaceError::Cancelled); }
            super::transfer_tree::remote_parents(sftp, &entry.remote).await?;
            let after = super::transfer_tree::remote_entry(sftp, &entry.remote).await?;
            if after.kind != entry.source.kind || after.size != entry.source.size || after.modified != entry.source.modified { return Err(WorkspaceError::Conflict); }
        }
        file.sync_all().await.map_err(local_error)?;
        if io.cancel.load(Ordering::Acquire) { return Err(WorkspaceError::Cancelled); }
        if let Some(expected) = destination {
            let current = tokio::fs::symlink_metadata(&io.local).await.map_err(local_error)?;
            if !current.is_file() || current.is_symlink() || (current.len(), current.modified().ok()) != expected { return Err(WorkspaceError::Conflict); }
            tokio::fs::rename(&temporary, &io.local).await.map_err(local_error)?;
        } else {
            tokio::fs::hard_link(&temporary, &io.local).await.map_err(local_error)?;
        }
        io.total.store(io.progress.load(Ordering::Acquire), Ordering::Release);
        let mut state = archive.state.lock().unwrap();
        state.stage = "finished".into();
        for entry in &mut state.entries { if entry.phase != "skipped" { entry.phase = "complete".into(); } }
        Ok(())
    }.await;
    drop(file);
    let _ = tokio::fs::remove_file(&temporary).await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn computes_common_parent_without_losing_selected_roots() {
        assert_eq!(
            common_parent(&["/srv/a".into(), "/srv/b/c".into()]).unwrap(),
            "/srv"
        );
        assert_eq!(
            common_parent(&["/srv/a".into(), "/etc/b".into()]).unwrap(),
            "/"
        );
    }
    #[test]
    fn nul_manifest_preserves_option_like_and_newline_names() {
        let entries = vec![TransferTreeEntry {
            local: PathBuf::new(),
            remote: "/srv/--name\nx".into(),
            source: RemoteEntry {
                name: String::new(),
                path: String::new(),
                kind: "file".into(),
                size: None,
                modified: None,
                permissions: None,
                uid: None,
                gid: None,
            },
            phase: "pending".into(),
            error: None,
            transferred: 0,
        }];
        assert_eq!(
            archive_manifest(&entries, "/srv").unwrap(),
            b"./--name\nx\0"
        );
    }
}
