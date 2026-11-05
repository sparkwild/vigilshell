//! 具名文件操作只通过 SFTP 与固定系统工具执行，不部署远端 worker。
use super::*;
use crate::sftp::{Handle, Session as SftpSession, empty_attrs, sftp_error};
use base64::{Engine, engine::general_purpose::STANDARD};
use russh_sftp::{
    client::{RawSftpSession, error::Error as SftpError},
    protocol::{FileAttributes, OpenFlags, Packet, StatusCode},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;
use vigilshell_application_contracts::ports::{FileExecutionControl, FileTransferHandles};
use vigilshell_domain::agent::*;
mod read;
mod tree;
pub(super) struct Listing {
    path: String,
    scope: String,
    entries: Vec<Value>,
    at: std::time::Instant,
    bytes: usize,
}
mod write;

pub(super) async fn execute(
    connection: &Connection,
    operation: CheckedFileOperation,
    io: FileTransferHandles,
    control: FileExecutionControl,
) -> Result<FileExecutionReport, WorkspaceError> {
    execute_content(connection, operation, io, control, None).await
}
pub(super) async fn write_maintenance(
    connection: &Connection,
    operation: CheckedFileOperation,
    bytes: Vec<u8>,
    control: FileExecutionControl,
) -> Result<FileExecutionReport, WorkspaceError> {
    if bytes.len() > 1024 * 1024 {
        return Err(WorkspaceError::FilesTooLarge);
    }
    execute_content(
        connection,
        operation,
        FileTransferHandles::default(),
        control,
        Some(bytes),
    )
    .await
}
async fn execute_content(
    connection: &Connection,
    operation: CheckedFileOperation,
    io: FileTransferHandles,
    control: FileExecutionControl,
    bytes: Option<Vec<u8>>,
) -> Result<FileExecutionReport, WorkspaceError> {
    let session = connection
        .sftp
        .get_or_try_init(|| crate::sftp::open(&connection.handle))
        .await?;
    let mut files = Files {
        connection,
        session,
        operation,
        control,
        report: FileExecutionReport {
            commit: FileCommitState::Unchanged,
            error: None,
            json: None,
            recovery_paths: vec![],
            bytes: 0,
            hash: None,
        },
    };
    let action = files.operation.operation().clone();
    let result = if let Some(bytes) = bytes {
        if let RuntimeOperation::FilesWrite {
            target,
            mode,
            expected_hash,
            ..
        } = action
        {
            files
                .write(&Files::target(&target)?, bytes, mode, expected_hash)
                .await
        } else {
            Err(WorkspaceError::InvalidInput)
        }
    } else {
        files.run(action, io).await
    };
    match result {
        Ok(value) => files.report.json = Some(value.to_string()),
        Err(e) => {
            if matches!(
                e,
                WorkspaceError::Timeout
                    | WorkspaceError::Disconnected
                    | WorkspaceError::ToolsUnavailable
            ) {
                files.report.commit = FileCommitState::Unknown;
            }

            files.report.error = Some(
                match e {
                    WorkspaceError::Conflict => "file_changed",
                    WorkspaceError::Cancelled => "cancelled",
                    WorkspaceError::FilesExists => "already_exists",
                    WorkspaceError::FilesPermissionDenied => "file_denied",
                    WorkspaceError::NotFound => "file_not_found",
                    WorkspaceError::FilesTooLarge => "file_too_large",
                    WorkspaceError::FilesUnsupported => "file_capability_unavailable",
                    WorkspaceError::InvalidInput => "invalid_file_request",
                    _ => "file_io_failed",
                }
                .into(),
            );
        }
    }
    if files.report.commit == FileCommitState::Unchanged {
        for path in files.report.recovery_paths.clone() {
            if files
                .resolve(&path, FileEffect::Write, false, true)
                .await
                .is_ok()
                && files.sftp().remove(path.clone()).await.is_ok()
            {
                files.report.recovery_paths.retain(|p| p != &path);
            }
        }
    }
    files.report.bytes = files
        .report
        .bytes
        .max(files.control.bytes.load(Ordering::Acquire));
    Ok(files.report)
}
struct Files<'a> {
    connection: &'a Connection,
    session: &'a SftpSession,
    operation: CheckedFileOperation,
    control: FileExecutionControl,
    report: FileExecutionReport,
}
impl Files<'_> {
    fn sftp(&self) -> &RawSftpSession {
        &self.session.0
    }
    fn cancelled(&self) -> Result<(), WorkspaceError> {
        if self.control.cancel.load(Ordering::Acquire) {
            Err(WorkspaceError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn target(target: &ScopedPath) -> Result<String, WorkspaceError> {
        normalize_path(
            "/",
            if target.path.is_empty() {
                "."
            } else {
                &target.path
            },
        )
        .map_err(|_| WorkspaceError::InvalidInput)
    }
    async fn resolve(
        &self,
        path: &str,
        effect: FileEffect,
        recursive: bool,
        missing: bool,
    ) -> Result<String, WorkspaceError> {
        self.operation
            .target(path, effect, recursive)
            .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
        let mut current = String::new();
        let parts: Vec<_> = path.split('/').filter(|s| !s.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            current.push('/');
            current.push_str(part);
            match self.sftp().lstat(current.clone()).await {
                Ok(a) if a.attrs.is_symlink() => return Err(WorkspaceError::FilesUnsupported),
                Ok(a) if i + 1 < parts.len() && !a.attrs.is_dir() => {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                Ok(_) => {}
                Err(SftpError::Status(s))
                    if missing
                        && i + 1 == parts.len()
                        && s.status_code == StatusCode::NoSuchFile => {}
                Err(e) => return Err(sftp_error(e)),
            }
        }
        let parent = path
            .rsplit_once('/')
            .map_or("/", |(p, _)| if p.is_empty() { "/" } else { p });
        let actual = self
            .sftp()
            .realpath(parent)
            .await
            .map_err(sftp_error)?
            .files
            .into_iter()
            .next()
            .ok_or(WorkspaceError::NotFound)?
            .filename;
        if actual != parent {
            return Err(WorkspaceError::Conflict);
        }
        self.operation
            .target(&actual, FileEffect::Read, false)
            .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
        Ok(path.into())
    }
    async fn native(&self, body: String) -> Result<String, WorkspaceError> {
        let command = format!(
            "exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /bin/sh -c {}",
            shell_quote(&body)
        );
        let mut process = self
            .connection
            .open_exec_encoded(command, false, false, encoding_rs::UTF_8)
            .await?;
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let mut output = Vec::new();
            let mut status = None;
            loop {
                match process.read().await? {
                    AgentProcessEvent::Stdout(b) => {
                        if output.len() + b.len() > 1024 * 1024 {
                            return Err(WorkspaceError::FilesTooLarge);
                        }
                        output.extend(b);
                    }
                    AgentProcessEvent::Stderr(_) | AgentProcessEvent::Eof => {}
                    AgentProcessEvent::Exit(c) => status = Some(c),
                    AgentProcessEvent::ExitSignal { .. } => {
                        return Err(WorkspaceError::ToolsUnavailable);
                    }
                    AgentProcessEvent::Closed => break,
                }
            }
            if status != Some(0) {
                return Err(WorkspaceError::FilesUnsupported);
            }
            String::from_utf8(output).map_err(|_| WorkspaceError::FilesUnsupported)
        })
        .await
        .map_err(|_| WorkspaceError::Timeout)
        .and_then(|r| r);
        let _ = process.stop().await;
        result
    }
    async fn version(&self, path: &str) -> Result<String, WorkspaceError> {
        self.native(format!(
            "/usr/bin/stat -c '%d:%i:%h:%f:%u:%g:%s:%y:%z' -- {}",
            shell_quote(path)
        ))
        .await
    }
    async fn xattrs(&self, path: &str) -> Result<String, WorkspaceError> {
        let text = self
            .native(format!(
                "/usr/bin/getfattr --absolute-names -d -m - -e hex -- {}",
                shell_quote(path)
            ))
            .await?;
        Ok(text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }
    fn mutating(&mut self) {
        self.report.commit = FileCommitState::Unknown;
    }
    fn changed(&mut self) {
        self.report.commit = FileCommitState::Partial;
    }
    async fn run(
        &mut self,
        action: RuntimeOperation,
        mut io: FileTransferHandles,
    ) -> Result<Value, WorkspaceError> {
        match action {
            RuntimeOperation::FilesList {
                target,
                after,
                limit,
            } => self.list(&Self::target(&target)?, after, limit).await,
            RuntimeOperation::FilesStat { target, hash } => {
                self.stat(&Self::target(&target)?, hash).await
            }
            RuntimeOperation::FilesRead {
                target,
                offset,
                length,
            } => self.read(&Self::target(&target)?, offset, length).await,
            RuntimeOperation::FilesWrite {
                target,
                data,
                mode,
                expected_hash,
            } => {
                let bytes = STANDARD
                    .decode(data)
                    .map_err(|_| WorkspaceError::InvalidInput)?;
                if bytes.len() > FILE_CHUNK_LIMIT {
                    return Err(WorkspaceError::FilesTooLarge);
                }
                self.write(&Self::target(&target)?, bytes, mode, expected_hash)
                    .await
            }
            RuntimeOperation::FilesEdit {
                target,
                edits,
                expected_hash,
            } => {
                self.edit(&Self::target(&target)?, edits, &expected_hash)
                    .await
            }
            RuntimeOperation::FilesMkdir { target, parents } => {
                self.mkdir(&Self::target(&target)?, parents).await
            }
            RuntimeOperation::FilesRemove { target, recursive } => {
                self.remove(&Self::target(&target)?, recursive).await
            }
            RuntimeOperation::FilesCopy {
                source,
                target,
                recursive,
                overwrite,
            } => {
                self.copy(
                    &Self::target(&source)?,
                    &Self::target(&target)?,
                    recursive,
                    overwrite,
                )
                .await
            }
            RuntimeOperation::FilesMove {
                source,
                target,
                overwrite,
            } => {
                self.move_file(&Self::target(&source)?, &Self::target(&target)?, overwrite)
                    .await
            }
            RuntimeOperation::FilesLink {
                target,
                destination,
            } => {
                self.link(&Self::target(&target)?, &Self::target(&destination)?)
                    .await
            }
            RuntimeOperation::FilesSetAttributes {
                target,
                mode,
                modified_seconds,
            } => {
                self.attributes(&Self::target(&target)?, mode, modified_seconds)
                    .await
            }
            RuntimeOperation::TransfersUpload {
                remote, overwrite, ..
            } => {
                self.upload(
                    &Self::target(&remote)?,
                    io.upload.take().ok_or(WorkspaceError::InvalidInput)?,
                    overwrite,
                )
                .await
            }
            RuntimeOperation::TransfersDownload { remote, .. } => {
                self.download(
                    &Self::target(&remote)?,
                    io.download.take().ok_or(WorkspaceError::InvalidInput)?,
                )
                .await
            }
            _ => Err(WorkspaceError::InvalidInput),
        }
    }
}
#[cfg(test)]
mod tests;
