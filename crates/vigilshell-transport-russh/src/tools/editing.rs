use super::*;
use vigilshell_application_contracts::ports::AgentDownload;
use vigilshell_domain::tools::RemoteFileVersion;
struct Discard;
impl AgentDownload for Discard {
    fn write(&mut self, _: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn finish(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
impl Tools {
    pub(super) async fn editor_version(
        &self,
        path: &str,
    ) -> Result<RemoteFileVersion, WorkspaceError> {
        valid_remote_path(path)?;
        let sftp = &self.sftp().await?.0;
        let before = sftp.lstat(path).await.map_err(sftp_error)?.attrs;
        if !before.is_regular() || before.is_symlink() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let handle = sftp
            .open(path, OpenFlags::READ, crate::sftp::empty_attrs())
            .await
            .map_err(sftp_error)?
            .handle;
        let result = crate::sftp_transfer::download(
            sftp,
            &handle,
            &mut Discard,
            &FileExecutionControl::default(),
        )
        .await;
        let after = sftp.fstat(handle.clone()).await.map_err(sftp_error);
        let closed = sftp.close(handle).await.map_err(sftp_error);
        let (size, hash) = result?;
        let after = after?.attrs;
        closed?;
        if before.size != Some(size)
            || before.size != after.size
            || before.mtime != after.mtime
            || before.permissions != after.permissions
            || before.uid != after.uid
            || before.gid != after.gid
        {
            return Err(WorkspaceError::Conflict);
        }
        Ok(RemoteFileVersion {
            hash,
            size,
            modified: before
                .mtime
                .map(u64::from)
                .ok_or(WorkspaceError::FilesUnsupported)?,
            permissions: before.permissions.ok_or(WorkspaceError::FilesUnsupported)?,
            uid: before.uid.ok_or(WorkspaceError::FilesUnsupported)?,
            gid: before.gid.ok_or(WorkspaceError::FilesUnsupported)?,
        })
    }
    pub(super) async fn editor_download(
        &self,
        path: String,
        local: std::path::PathBuf,
    ) -> Result<RemoteFileVersion, WorkspaceError> {
        let version = self.editor_version(&path).await?;
        let (sftp, atomic) = self.sftp().await?;
        transfer_file(
            sftp,
            *atomic,
            TransferIo {
                local,
                remote: path.clone(),
                upload: false,
                overwrite: false,
                cancel: Default::default(),
                progress: Default::default(),
                total: Default::default(),
            },
        )
        .await?;
        if self.editor_version(&path).await? != version {
            return Err(WorkspaceError::Conflict);
        }
        Ok(version)
    }
    pub(super) async fn editor_upload(
        &self,
        path: String,
        local: std::path::PathBuf,
        expected: Option<RemoteFileVersion>,
    ) -> Result<RemoteFileVersion, WorkspaceError> {
        let (sftp, atomic) = self.sftp().await?;
        if expected.is_some() && !atomic {
            return Err(WorkspaceError::FilesUnsupported);
        }
        if let Some(base) = &expected {
            if self.editor_version(&path).await? != *base {
                return Err(WorkspaceError::Conflict);
            }
        } else {
            match sftp.lstat(&path).await {
                Ok(_) => return Err(WorkspaceError::FilesExists),
                Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => {}
                Err(e) => return Err(sftp_error(e)),
            }
        }
        let parent = path
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .ok_or(WorkspaceError::InvalidInput)?;
        let staging = format!(
            "{}/.vigilshell-edit-{}.part",
            parent.trim_end_matches('/'),
            new_id()
        );
        let work = async {
            transfer_file(
                sftp,
                *atomic,
                TransferIo {
                    local,
                    remote: staging.clone(),
                    upload: true,
                    overwrite: false,
                    cancel: Default::default(),
                    progress: Default::default(),
                    total: Default::default(),
                },
            )
            .await?;
            if let Some(base) = &expected {
                sftp.setstat(
                    &staging,
                    russh_sftp::protocol::FileAttributes {
                        permissions: Some(base.permissions),
                        uid: Some(base.uid),
                        gid: Some(base.gid),
                        ..crate::sftp::empty_attrs()
                    },
                )
                .await
                .map_err(sftp_error)?;
                let attrs = sftp.lstat(&staging).await.map_err(sftp_error)?.attrs;
                if attrs.permissions != Some(base.permissions)
                    || attrs.uid != Some(base.uid)
                    || attrs.gid != Some(base.gid)
                    || self.editor_version(&path).await? != *base
                {
                    return Err(WorkspaceError::Conflict);
                }
                let mut data = Vec::new();
                for v in [&staging, &path] {
                    data.extend_from_slice(&(v.len() as u32).to_be_bytes());
                    data.extend_from_slice(v.as_bytes());
                }
                match sftp.extended("posix-rename@openssh.com", data).await {
                    Ok(Packet::Status(s)) if s.status_code == StatusCode::Ok => {}
                    Err(e) => return Err(sftp_error(e)),
                    _ => return Err(WorkspaceError::FilesUnsupported),
                }
            } else {
                sftp.rename(&staging, &path).await.map_err(sftp_error)?;
            }
            self.editor_version(&path).await
        }
        .await;
        if work.is_err() {
            let _ = sftp.remove(&staging).await;
        }
        work
    }
}
