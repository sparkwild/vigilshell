//! 共用分块、完整摘要及背压；调用者分别拥有授权、暂存和提交。
use crate::sftp::sftp_error;
use russh_sftp::{
    client::{RawSftpSession, error::Error},
    protocol::StatusCode,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vigilshell_application_contracts::ports::{
    AgentDownload, AgentUpload, FileExecutionControl, FutureResult,
};
use vigilshell_domain::{agent::FILE_CHUNK_LIMIT, workspace::WorkspaceError};
pub(crate) async fn upload(
    sftp: &RawSftpSession,
    handle: &str,
    source: &mut dyn AgentUpload,
    control: &FileExecutionControl,
) -> Result<(u64, String), WorkspaceError> {
    control.total.store(source.size(), Ordering::Release);
    let mut offset = 0;
    let mut hash = Sha256::new();
    loop {
        cancelled(control)?;
        let bytes = tokio::select! {biased;_=cancel_signal(control)=>return Err(WorkspaceError::Cancelled),result=source.read()=>result?};
        if bytes.is_empty() {
            break;
        }
        if bytes.len() > FILE_CHUNK_LIMIT {
            return Err(WorkspaceError::FilesTooLarge);
        }
        sftp.write(handle, offset, bytes.clone())
            .await
            .map_err(sftp_error)?;
        offset += bytes.len() as u64;
        hash.update(bytes);
        control.bytes.store(offset, Ordering::Release);
    }
    if offset != source.size() {
        return Err(WorkspaceError::Conflict);
    }
    Ok((offset, format!("{:x}", hash.finalize())))
}
pub(crate) async fn download(
    sftp: &RawSftpSession,
    handle: &str,
    sink: &mut dyn AgentDownload,
    control: &FileExecutionControl,
) -> Result<(u64, String), WorkspaceError> {
    let metadata = sftp.fstat(handle).await.map_err(sftp_error)?.attrs;
    control
        .total
        .store(metadata.size.unwrap_or(u64::MAX), Ordering::Release);
    let mut offset = 0;
    let mut hash = Sha256::new();
    loop {
        cancelled(control)?;
        let bytes = match sftp.read(handle, offset, 32768).await {
            Ok(d) => d.data,
            Err(Error::Status(s)) if s.status_code == StatusCode::Eof => break,
            Err(e) => return Err(sftp_error(e)),
        };
        if bytes.is_empty() {
            break;
        }
        if bytes.len() > FILE_CHUNK_LIMIT {
            return Err(WorkspaceError::FilesTooLarge);
        }
        offset += bytes.len() as u64;
        hash.update(&bytes);
        tokio::select! {biased;_=cancel_signal(control)=>return Err(WorkspaceError::Cancelled),result=sink.write(bytes)=>result?};
        control.bytes.store(offset, Ordering::Release);
    }
    Ok((offset, format!("{:x}", hash.finalize())))
}
pub(crate) fn cancelled(control: &FileExecutionControl) -> Result<(), WorkspaceError> {
    if control.cancel.load(Ordering::Acquire) {
        Err(WorkspaceError::Cancelled)
    } else {
        Ok(())
    }
}
pub(crate) struct UploadFile {
    pub file: tokio::fs::File,
    pub metadata: std::fs::Metadata,
    pub read: u64,
}
impl AgentUpload for UploadFile {
    fn size(&self) -> u64 {
        self.metadata.len()
    }
    fn read(&mut self) -> FutureResult<'_, Vec<u8>> {
        Box::pin(async move {
            let mut bytes = vec![0; 32768];
            let n = self
                .file
                .read(&mut bytes)
                .await
                .map_err(|_| WorkspaceError::StorageUnavailable)?;
            bytes.truncate(n);
            self.read += n as u64;
            let now = self
                .file
                .metadata()
                .await
                .map_err(|_| WorkspaceError::StorageUnavailable)?;
            if now.len() != self.metadata.len()
                || now.modified().ok() != self.metadata.modified().ok()
                || self.read > now.len()
                || n == 0 && self.read != now.len()
            {
                return Err(WorkspaceError::Conflict);
            }
            Ok(bytes)
        })
    }
}
pub(crate) struct DownloadFile(pub tokio::fs::File);
impl AgentDownload for DownloadFile {
    fn write(&mut self, bytes: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if bytes.len() > FILE_CHUNK_LIMIT {
                return Err(WorkspaceError::FilesTooLarge);
            }
            self.0
                .write_all(&bytes)
                .await
                .map_err(|_| WorkspaceError::StorageUnavailable)
        })
    }
    fn finish(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            self.0
                .sync_all()
                .await
                .map_err(|_| WorkspaceError::StorageUnavailable)
        })
    }
}

async fn cancel_signal(control: &FileExecutionControl) {
    while !control.cancel.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

struct Discard;
impl AgentDownload for Discard {
    fn write(&mut self, _: Vec<u8>) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn finish(&mut self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
pub(crate) async fn verify(
    sftp: &RawSftpSession,
    path: &str,
    expected: (u64, String),
    control: &FileExecutionControl,
) -> Result<(), WorkspaceError> {
    let before = sftp.lstat(path).await.map_err(sftp_error)?.attrs;
    if !before.is_regular() || before.size != Some(expected.0) {
        return Err(WorkspaceError::Conflict);
    }
    let handle = sftp
        .open(
            path,
            russh_sftp::protocol::OpenFlags::READ,
            crate::sftp::empty_attrs(),
        )
        .await
        .map_err(sftp_error)?
        .handle;
    let check = FileExecutionControl {
        cancel: control.cancel.clone(),
        ..Default::default()
    };
    let result = download(sftp, &handle, &mut Discard, &check).await;
    let after = sftp.fstat(handle.clone()).await.map_err(sftp_error);
    let closed = sftp.close(handle).await.map_err(sftp_error);
    let actual = result?;
    let after = after?.attrs;
    closed?;
    if actual != expected || before.size != after.size || before.mtime != after.mtime {
        return Err(WorkspaceError::Conflict);
    }
    Ok(())
}
