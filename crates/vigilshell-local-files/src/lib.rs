//! Agent 本地传输只持有已授权目录句柄，不通过校验后的绝对路径重新打开文件。
use cap_std::fs::{Dir, OpenOptions};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vigilshell_application_contracts::ports::{
    AgentDownload, AgentLocalFiles, AgentUpload, FutureResult,
};
use vigilshell_domain::{
    agent::{FILE_CHUNK_LIMIT, LocalTransferGrant},
    workspace::{WorkspaceError, new_id},
};

pub struct LocalFiles {
    protected: Vec<PathBuf>,
}
impl LocalFiles {
    /// 正常产品装配使用此入口，保留应用、凭据和可执行目录的传输保护。
    pub fn for_application(data_root: PathBuf) -> Self {
        let mut protected = vec![data_root];
        if let Ok(executable) = std::env::current_exe()
            && let Some(parent) = executable.parent()
        {
            protected.push(parent.into());
        }
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            for name in [
                ".ssh",
                "Library/Keychains",
                "Library/Application Support/VigilShell",
                "Library/Application Support/VigilShell-dev",
            ] {
                protected.push(home.join(name));
            }
        }
        Self::new(protected)
    }

    pub fn new(protected: Vec<PathBuf>) -> Self {
        Self { protected }
    }
    fn root(&self, grant: &LocalTransferGrant, upload: bool) -> Result<Dir, WorkspaceError> {
        if grant.upload() != upload {
            return Err(WorkspaceError::FilesPermissionDenied);
        }
        let directory =
            Dir::open_ambient_dir(grant.root(), cap_std::ambient_authority()).map_err(map)?;
        for protected in &self.protected {
            if protected_overlap(&directory, protected, !grant.unrestricted())?
                || (grant.unrestricted() && protected_name(&directory, grant.path(), protected)?)
            {
                return Err(WorkspaceError::FilesPermissionDenied);
            }
        }
        if grant.unrestricted()
            && directory
                .symlink_metadata(grant.path())
                .is_ok_and(|m| m.is_symlink())
        {
            return Err(WorkspaceError::FilesPermissionDenied);
        }
        Ok(directory)
    }
}
fn map(error: std::io::Error) -> WorkspaceError {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => WorkspaceError::FilesPermissionDenied,
        std::io::ErrorKind::NotFound => WorkspaceError::NotFound,
        std::io::ErrorKind::AlreadyExists => WorkspaceError::FilesExists,
        _ => WorkspaceError::FilesUnsupported,
    }
}
struct Upload {
    file: tokio::fs::File,
    size: u64,
    read: u64,
    modified: std::time::SystemTime,
}
impl AgentUpload for Upload {
    fn size(&self) -> u64 {
        self.size
    }
    fn read(&mut self) -> FutureResult<'_, Vec<u8>> {
        Box::pin(async move {
            let mut bytes = vec![0; FILE_CHUNK_LIMIT];
            let n = self.file.read(&mut bytes).await.map_err(map)?;
            self.read += n as u64;
            if self.read > self.size || n == 0 && self.read != self.size {
                return Err(WorkspaceError::Conflict);
            }
            let metadata = self.file.metadata().await.map_err(map)?;
            if metadata.len() != self.size || metadata.modified().map_err(map)? != self.modified {
                return Err(WorkspaceError::Conflict);
            }
            bytes.truncate(n);
            Ok(bytes)
        })
    }
}
#[derive(Clone, PartialEq, Eq)]
struct DestinationVersion {
    size: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
fn destination_version(
    dir: &Dir,
    path: &str,
) -> Result<Option<DestinationVersion>, WorkspaceError> {
    let metadata = match dir.symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(map(e)),
    };
    if !metadata.is_file() {
        return Err(WorkspaceError::FilesUnsupported);
    }
    #[cfg(unix)]
    use cap_std::fs::MetadataExt;
    Ok(Some(DestinationVersion {
        size: metadata.len(),
        modified: metadata.modified().map_err(map)?.into_std(),
        #[cfg(unix)]
        identity: (
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    }))
}
struct Download {
    file: tokio::fs::File,
    directory: Arc<Dir>,
    temporary: String,
    target: String,
    overwrite: bool,
    committed: bool,
    original: Option<DestinationVersion>,
}
impl AgentDownload for Download {
    fn write(&mut self, bytes: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if bytes.len() > FILE_CHUNK_LIMIT || self.committed {
                return Err(WorkspaceError::InvalidInput);
            }
            self.file.write_all(&bytes).await.map_err(map)
        })
    }
    fn finish(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if self.committed {
                return Ok(());
            }
            self.file.flush().await.map_err(map)?;
            self.file.sync_all().await.map_err(map)?;
            let (dir, temporary, target, overwrite) = (
                self.directory.clone(),
                self.temporary.clone(),
                self.target.clone(),
                self.overwrite,
            );
            let original = self.original.clone();
            tokio::task::spawn_blocking(move || {
                if destination_version(&dir, &target)? != original {
                    return Err(WorkspaceError::Conflict);
                }
                if let Ok(metadata) = dir.symlink_metadata(&target)
                    && !metadata.is_file()
                {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                if overwrite {
                    dir.rename(&temporary, &dir, &target).map_err(map)?;
                } else {
                    dir.hard_link(&temporary, &dir, &target).map_err(map)?;
                    let _ = dir.remove_file(&temporary);
                }
                Ok::<_, WorkspaceError>(())
            })
            .await
            .map_err(|_| WorkspaceError::StorageUnavailable)??;
            self.committed = true;
            Ok(())
        })
    }
}
impl Drop for Download {
    fn drop(&mut self) {
        let _ = self.directory.remove_file(&self.temporary);
    }
}
impl AgentLocalFiles for LocalFiles {
    fn upload(&self, grant: LocalTransferGrant) -> FutureResult<'_, Box<dyn AgentUpload>> {
        Box::pin(async move {
            let protected = self.protected.clone();
            let file = tokio::task::spawn_blocking(move || {
                let root = LocalFiles::new(protected).root(&grant, true)?;
                if !root.metadata(grant.path()).map_err(map)?.is_file() {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    options.custom_flags(
                        libc::O_NONBLOCK
                            | if grant.unrestricted() {
                                libc::O_NOFOLLOW
                            } else {
                                0
                            },
                    );
                }
                root.open_with(grant.path(), &options).map_err(map)
            })
            .await
            .map_err(|_| WorkspaceError::StorageUnavailable)??;
            let metadata = file.metadata().map_err(map)?;
            if !metadata.is_file() {
                return Err(WorkspaceError::FilesUnsupported);
            }
            #[cfg(unix)]
            {
                use cap_std::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err(WorkspaceError::FilesPermissionDenied);
                }
            }
            Ok(Box::new(Upload {
                size: metadata.len(),
                read: 0,
                modified: metadata.modified().map_err(map)?.into_std(),
                file: tokio::fs::File::from_std(file.into_std()),
            }) as Box<dyn AgentUpload>)
        })
    }
    fn download(
        &self,
        grant: LocalTransferGrant,
        overwrite: bool,
    ) -> FutureResult<'_, Box<dyn AgentDownload>> {
        Box::pin(async move {
            let protected = self.protected.clone();
            let path = Path::new(grant.path());
            let target_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty() && *name != ".")
                .ok_or(WorkspaceError::InvalidInput)?
                .to_owned();
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned();
            let result = tokio::task::spawn_blocking(move || {
                let root = LocalFiles::new(protected).root(&grant, false)?;
                root.create_dir_all(&parent).map_err(map)?;
                let directory = Arc::new(root.open_dir(&parent).map_err(map)?);
                match directory.symlink_metadata(&target_name) {
                    Ok(metadata) if !metadata.is_file() => {
                        return Err(WorkspaceError::FilesUnsupported);
                    }
                    Ok(_) if !overwrite => return Err(WorkspaceError::FilesExists),
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        return Err(map(error));
                    }
                    _ => {}
                }
                let original = destination_version(&directory, &target_name)?;
                let temporary = format!(".vigilshell-transfer-{}.part", new_id());
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let file = directory.open_with(&temporary, &options).map_err(map)?;
                // 在阻塞任务内建立清理所有者；调用方取消 await 后，返回值丢弃也会删临时文件。
                Ok::<_, WorkspaceError>(Download {
                    directory,
                    temporary,
                    target: target_name,
                    file: tokio::fs::File::from_std(file.into_std()),
                    overwrite,
                    committed: false,
                    original,
                })
            })
            .await
            .map_err(|_| WorkspaceError::StorageUnavailable)??;
            Ok(Box::new(result) as Box<dyn AgentDownload>)
        })
    }
}

// Check the protected name against the held parent, including aliases of that parent.
#[cfg(unix)]
fn protected_name(root: &Dir, name: &str, protected: &Path) -> Result<bool, WorkspaceError> {
    use cap_std::fs::MetadataExt;
    if protected.file_name() != Some(std::ffi::OsStr::new(name)) {
        return Ok(false);
    }
    let Some(parent) = protected.parent() else {
        return Ok(false);
    };
    let parent = match Dir::open_ambient_dir(parent, cap_std::ambient_authority()) {
        Ok(parent) => parent,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(map(e)),
    };
    let a = root.dir_metadata().map_err(map)?;
    let b = parent.dir_metadata().map_err(map)?;
    Ok(a.dev() == b.dev() && a.ino() == b.ino())
}
#[cfg(not(unix))]
fn protected_name(_root: &Dir, _name: &str, _protected: &Path) -> Result<bool, WorkspaceError> {
    Err(WorkspaceError::FilesUnsupported)
}

#[cfg(unix)]
fn protected_overlap(
    root: &Dir,
    protected: &Path,
    whole_root: bool,
) -> Result<bool, WorkspaceError> {
    use cap_std::fs::MetadataExt;
    fn identity(dir: &Dir) -> Result<(u64, u64), WorkspaceError> {
        let metadata = dir.dir_metadata().map_err(map)?;
        Ok((metadata.dev(), metadata.ino()))
    }
    fn inside(dir: &Dir, ancestor: (u64, u64)) -> Result<bool, WorkspaceError> {
        let mut current = dir.try_clone().map_err(map)?;
        for _ in 0..256 {
            let id = identity(&current)?;
            if id == ancestor {
                return Ok(true);
            }
            // Ambient parent traversal is confined to this protection check, never used for transfer IO.
            let parent = current
                .open_parent_dir(cap_std::ambient_authority())
                .map_err(map)?;
            if identity(&parent)? == id {
                return Ok(false);
            }
            current = parent;
        }
        Err(WorkspaceError::FilesPermissionDenied)
    }
    let mut path = protected;
    let mut exact = true;
    let protected = loop {
        match Dir::open_ambient_dir(path, cap_std::ambient_authority()) {
            Ok(directory) => break directory,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.kind() == std::io::ErrorKind::NotADirectory =>
            {
                let Some(parent) = path.parent() else {
                    return Ok(false);
                };
                path = parent;
                exact = false;
            }
            Err(error) => return Err(map(error)),
        }
    };
    Ok((whole_root && inside(&protected, identity(root)?)?)
        || exact && inside(root, identity(&protected)?)?)
}
#[cfg(not(unix))]
fn protected_overlap(
    _root: &Dir,
    _protected: &Path,
    _whole_root: bool,
) -> Result<bool, WorkspaceError> {
    Err(WorkspaceError::FilesUnsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vigilshell_domain::agent::*;
    fn grant(root: &Path, upload: bool, path: &str) -> LocalTransferGrant {
        let local = ScopedPath {
            scope_id: "local".into(),
            path: path.into(),
        };
        let remote = ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "srv/file".into(),
        };
        let policy = ExecutionPolicy {
            level: OperationLevel::Operate,
            file_transfer: true,
            upload_roots: vec![LocalDirectoryGrant {
                id: "local".into(),
                path: root.to_str().unwrap().into(),
            }],
            download_roots: vec![LocalDirectoryGrant {
                id: "local".into(),
                path: root.to_str().unwrap().into(),
            }],
            ..Default::default()
        };
        local_transfer_grant(
            &policy,
            &if upload {
                RuntimeOperation::TransfersUpload {
                    local,
                    remote,
                    overwrite: true,
                }
            } else {
                RuntimeOperation::TransfersDownload {
                    local,
                    remote,
                    overwrite: true,
                }
            },
        )
        .unwrap()
    }

    fn unrestricted(file: &Path, upload: bool) -> LocalTransferGrant {
        let policy = ExecutionPolicy {
            level: OperationLevel::Operate,
            file_transfer: true,
            ..Default::default()
        };
        let local = ScopedPath {
            scope_id: "local".into(),
            path: file.to_str().unwrap().trim_start_matches('/').into(),
        };
        let remote = ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "tmp/file".into(),
        };
        local_transfer_grant(
            &policy,
            &if upload {
                RuntimeOperation::TransfersUpload {
                    local,
                    remote,
                    overwrite: false,
                }
            } else {
                RuntimeOperation::TransfersDownload {
                    local,
                    remote,
                    overwrite: false,
                }
            },
        )
        .unwrap()
    }
    #[tokio::test]
    async fn unrestricted_transfers_allow_other_directories_but_keep_private_boundaries() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let private = a.path().join("private");
        std::fs::create_dir(&private).unwrap();
        std::fs::write(private.join("secret"), b"synthetic-private").unwrap();
        let files = LocalFiles::new(vec![private.clone()]);
        for root in [a.path(), b.path()] {
            let file = root.join("source");
            std::fs::write(&file, b"synthetic-public").unwrap();
            let mut source = files.upload(unrestricted(&file, true)).await.unwrap();
            assert_eq!(source.read().await.unwrap(), b"synthetic-public");
            let output = root.join("output");
            let mut destination = files
                .download(unrestricted(&output, false), false)
                .await
                .unwrap();
            destination.write(b"output".to_vec()).await.unwrap();
            destination.finish().await.unwrap();
            assert_eq!(std::fs::read(output).unwrap(), b"output");
        }
        assert!(
            files
                .upload(unrestricted(&private.join("secret"), true))
                .await
                .is_err()
        );
        assert!(
            files
                .download(unrestricted(&private.join("new"), false), false)
                .await
                .is_err()
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(private.join("secret"), a.path().join("alias")).unwrap();
            assert!(
                files
                    .upload(unrestricted(&a.path().join("alias"), true))
                    .await
                    .is_err()
            );
            std::os::unix::fs::symlink(&private, b.path().join("alias-dir")).unwrap();
            assert!(
                files
                    .upload(unrestricted(&b.path().join("alias-dir/secret"), true))
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn cancellation_keeps_original_and_commit_is_atomic() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), "original").unwrap();
        let files = LocalFiles::new(vec![]);
        let mut download = files
            .download(grant(root.path(), false, "file"), true)
            .await
            .unwrap();
        download.write(b"new".to_vec()).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("file")).unwrap(),
            "original"
        );
        drop(download);
        assert_eq!(
            std::fs::read_to_string(root.path().join("file")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        let mut download = files
            .download(grant(root.path(), false, "file"), true)
            .await
            .unwrap();
        download.write(b"new".to_vec()).await.unwrap();
        download.finish().await.unwrap();
        download.finish().await.unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("file")).unwrap(),
            "new"
        );
    }
    #[tokio::test]
    async fn a_racing_destination_is_not_overwritten_without_permission() {
        let root = tempfile::tempdir().unwrap();
        let files = LocalFiles::new(vec![]);
        let mut download = files
            .download(grant(root.path(), false, "file"), false)
            .await
            .unwrap();
        download.write(b"incoming".to_vec()).await.unwrap();
        std::fs::write(root.path().join("file"), "other writer").unwrap();
        assert!(download.finish().await.is_err());
        drop(download);
        assert_eq!(
            std::fs::read_to_string(root.path().join("file")).unwrap(),
            "other writer"
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn links_cannot_escape_the_transfer_root_or_alias_protected_data() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        let files = LocalFiles::new(vec![]);
        assert!(
            files
                .upload(grant(root.path(), true, "escape/secret"))
                .await
                .is_err()
        );
        assert!(
            files
                .download(grant(root.path(), false, "escape/new"), false)
                .await
                .is_err()
        );
        assert!(!outside.path().join("new").exists());
        let protected = LocalFiles::new(vec![outside.path().into()]);
        assert!(
            protected
                .upload(grant(&root.path().join("escape"), true, "secret"))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn uploads_keep_chunks_bounded_and_validate_the_source_size() {
        let root = tempfile::tempdir().unwrap();
        let files = LocalFiles::new(vec![]);
        let contents = vec![42; FILE_CHUNK_LIMIT * 3 + 17];
        std::fs::write(root.path().join("file"), &contents).unwrap();
        let mut upload = files
            .upload(grant(root.path(), true, "file"))
            .await
            .unwrap();
        assert_eq!(upload.size(), contents.len() as u64);
        let mut read = Vec::new();
        loop {
            let chunk = upload.read().await.unwrap();
            assert!(chunk.len() <= FILE_CHUNK_LIMIT);
            if chunk.is_empty() {
                break;
            }
            read.extend(chunk);
        }
        assert_eq!(read, contents);
    }
    #[tokio::test]
    async fn changed_source_or_destination_never_commits_unverified_bytes() {
        let root = tempfile::tempdir().unwrap();
        let files = LocalFiles::new(vec![]);
        std::fs::write(root.path().join("file"), b"before").unwrap();
        let mut download = files
            .download(grant(root.path(), false, "file"), true)
            .await
            .unwrap();
        download.write(b"new bytes".to_vec()).await.unwrap();
        std::fs::write(root.path().join("file"), b"other writer").unwrap();
        assert_eq!(download.finish().await, Err(WorkspaceError::Conflict));
        drop(download);
        assert_eq!(
            std::fs::read(root.path().join("file")).unwrap(),
            b"other writer"
        );
        let mut upload = files
            .upload(grant(root.path(), true, "file"))
            .await
            .unwrap();
        std::fs::write(root.path().join("file"), b"changed source length").unwrap();
        assert!(matches!(upload.read().await, Err(WorkspaceError::Conflict)));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
