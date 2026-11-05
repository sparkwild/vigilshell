//! 人工与 Agent 共用 SFTP 建链及目录读取，不共用账号授权或业务会话。
use super::*;
use russh_sftp::{
    client::{RawSftpSession, error::Error as SftpError},
    protocol::StatusCode,
};
use std::sync::atomic::{AtomicBool, Ordering};
use vigilshell_domain::tools::RemoteEntry;
pub(crate) type Session = (Arc<RawSftpSession>, bool);
pub(crate) async fn open(handle: &client::Handle<Handler>) -> Result<Session, WorkspaceError> {
    let channel = deadline(handle.channel_open_session(), 10).await?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(error)?;
    let session = RawSftpSession::new(channel.into_stream());
    session.set_timeout(10);
    let version = session.init().await.map_err(sftp_error)?;
    let atomic = version
        .extensions
        .get("posix-rename@openssh.com")
        .is_some_and(|v| v == "1");
    Ok((Arc::new(session), atomic))
}
pub(crate) fn sftp_error(e: SftpError) -> WorkspaceError {
    match e {
        SftpError::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => WorkspaceError::NotFound,
            StatusCode::PermissionDenied => WorkspaceError::FilesPermissionDenied,
            StatusCode::OpUnsupported => WorkspaceError::FilesUnsupported,
            _ => WorkspaceError::ToolsUnavailable,
        },
        SftpError::Timeout => WorkspaceError::Timeout,
        _ => WorkspaceError::ToolsUnavailable,
    }
}
pub(crate) async fn list(
    sftp: &Arc<RawSftpSession>,
    path: String,
    cancel: &AtomicBool,
) -> Result<(String, Vec<RemoteEntry>), WorkspaceError> {
    let canonical = sftp
        .realpath(path)
        .await
        .map_err(sftp_error)?
        .files
        .into_iter()
        .next()
        .ok_or(WorkspaceError::NotFound)?
        .filename;
    let handle = sftp
        .opendir(canonical.clone())
        .await
        .map_err(sftp_error)?
        .handle;
    let owned = Handle {
        sftp: sftp.clone(),
        id: Some(handle),
    };
    let handle = owned.id();
    let result = async {
        let mut entries = Vec::new();
        let mut retained_bytes = 0usize;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(WorkspaceError::Cancelled);
            }
            let page = match sftp.readdir(handle.clone()).await {
                Ok(p) => p,
                Err(SftpError::Status(s)) if s.status_code == StatusCode::Eof => break,
                Err(e) => return Err(sftp_error(e)),
            };
            if page.files.is_empty() {
                break;
            }
            for entry in page.files {
                if [".", ".."].contains(&entry.filename.as_str()) {
                    continue;
                }
                if entry.filename.contains(['/', '\0']) {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                retained_bytes += canonical.len() + entry.filename.len() + 128;
                if entries.len() >= 20000 || retained_bytes > 4 * 1024 * 1024 {
                    return Err(WorkspaceError::FilesTooLarge);
                }
                let kind = if entry.attrs.is_symlink() {
                    "symlink"
                } else if entry.attrs.is_dir() {
                    "directory"
                } else if entry.attrs.is_regular() {
                    "file"
                } else {
                    "other"
                };
                entries.push(RemoteEntry {
                    path: format!("{}/{}", canonical.trim_end_matches('/'), entry.filename),
                    name: entry.filename,
                    kind: kind.into(),
                    size: entry.attrs.size,
                    modified: entry.attrs.mtime.map(u64::from),
                    permissions: entry.attrs.permissions,
                    uid: entry.attrs.uid,
                    gid: entry.attrs.gid,
                });
            }
        }
        entries.sort_by(|a, b| {
            (a.kind != "directory", &a.name).cmp(&(b.kind != "directory", &b.name))
        });
        Ok((canonical, entries))
    }
    .await;
    let _ = owned.close().await;
    result
}

/// 上游 Default 含 size=0、uid/gid=0，不能用于部分属性更新。
pub(crate) fn empty_attrs() -> russh_sftp::protocol::FileAttributes {
    russh_sftp::protocol::FileAttributes {
        size: None,
        uid: None,
        user: None,
        gid: None,
        group: None,
        permissions: None,
        atime: None,
        mtime: None,
    }
}

pub(crate) fn agent_entry(
    entry: RemoteEntry,
) -> Result<vigilshell_domain::agent::AgentFileEntry, WorkspaceError> {
    Ok(vigilshell_domain::agent::AgentFileEntry {
        path: entry.path,
        name: entry.name,
        kind: entry.kind,
        size: entry.size.ok_or(WorkspaceError::FilesUnsupported)?,
        mode: entry.permissions.ok_or(WorkspaceError::FilesUnsupported)? & 0o7777,
        uid: entry.uid.ok_or(WorkspaceError::FilesUnsupported)?,
        gid: entry.gid.ok_or(WorkspaceError::FilesUnsupported)?,
        modified_seconds: entry.modified,
    })
}

pub(crate) struct Handle {
    pub(crate) sftp: Arc<RawSftpSession>,
    pub(crate) id: Option<String>,
}
impl Handle {
    pub(crate) fn id(&self) -> String {
        self.id.as_ref().unwrap().clone()
    }
    pub(crate) async fn close(mut self) -> Result<(), WorkspaceError> {
        self.sftp
            .close(self.id.take().unwrap())
            .await
            .map_err(sftp_error)?;
        Ok(())
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let sftp = self.sftp.clone();
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                rt.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(3), sftp.close(id)).await;
                });
            }
        }
    }
}
