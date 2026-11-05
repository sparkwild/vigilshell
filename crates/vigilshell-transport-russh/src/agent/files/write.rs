use super::*;
use vigilshell_application_contracts::ports::{AgentDownload, AgentUpload};
pub(super) struct Baseline {
    pub(super) version: String,
    attrs: FileAttributes,
    xattrs: String,
    pub(super) hash: String,
}
impl Files<'_> {
    pub(super) async fn baseline(
        &self,
        path: &str,
        expected: Option<&str>,
    ) -> Result<Option<Baseline>, WorkspaceError> {
        self.resolve(path, FileEffect::Write, false, true).await?;
        let a = match self.sftp().lstat(path).await {
            Ok(a) => a.attrs,
            Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => {
                return if expected.is_some() {
                    Err(WorkspaceError::Conflict)
                } else {
                    Ok(None)
                };
            }
            Err(e) => return Err(sftp_error(e)),
        };
        if !a.is_regular()
            || a.uid.is_none()
            || a.gid.is_none()
            || a.size.is_none()
            || a.mtime.is_none()
        {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let version = self.version(path).await?;
        if version.split(':').nth(2) != Some("1") {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let xattrs = self.xattrs(path).await?;
        let hash = self.digest(path).await?.1;
        if expected.is_some_and(|e| e != hash) || self.version(path).await? != version {
            return Err(WorkspaceError::Conflict);
        }
        Ok(Some(Baseline {
            version,
            attrs: a,
            xattrs,
            hash,
        }))
    }
    pub(super) async fn stage(
        &mut self,
        path: &str,
        base: Option<&Baseline>,
    ) -> Result<String, WorkspaceError> {
        if base.is_some() && !self.session.1 {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let temporary = self.operation.staging_path(path);
        self.resolve(&temporary, FileEffect::Write, false, true)
            .await?;
        self.open(
            &temporary,
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
        )
        .await?
        .close()
        .await?;
        self.report.recovery_paths.push(temporary.clone());
        if let Some(base) = base {
            // GNU cp 负责 ACL/xattr；随后逐项核对，不把成功退出当成元数据已保全。
            self.native(format!("/usr/bin/cp --remove-destination --no-dereference --preserve=mode,ownership,timestamps,xattr -- {} {}",shell_quote(path),shell_quote(&temporary))).await?;
            self.preserved(&temporary, base).await?;
            self.sftp()
                .setstat(
                    temporary.clone(),
                    FileAttributes {
                        permissions: Some(0o600),
                        ..empty_attrs()
                    },
                )
                .await
                .map_err(sftp_error)?;
        }
        Ok(temporary)
    }
    async fn preserved(&self, path: &str, base: &Baseline) -> Result<(), WorkspaceError> {
        let a = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if !a.is_regular()
            || a.uid != base.attrs.uid
            || a.gid != base.attrs.gid
            || a.permissions != base.attrs.permissions
            || self.xattrs(path).await? != base.xattrs
        {
            return Err(WorkspaceError::FilesUnsupported);
        }
        Ok(())
    }
    pub(super) async fn commit(
        &mut self,
        temporary: &str,
        path: &str,
        base: Option<&Baseline>,
    ) -> Result<(), WorkspaceError> {
        self.cancelled()?;
        self.resolve(path, FileEffect::Write, false, true).await?;
        self.resolve(temporary, FileEffect::Write, false, false)
            .await?;
        if let Some(base) = base {
            self.sftp()
                .setstat(
                    temporary,
                    FileAttributes {
                        uid: base.attrs.uid,
                        gid: base.attrs.gid,
                        permissions: base.attrs.permissions,
                        ..empty_attrs()
                    },
                )
                .await
                .map_err(sftp_error)?;
            self.preserved(temporary, base).await?;
            if self.version(path).await? != base.version || self.digest(path).await?.1 != base.hash
            {
                return Err(WorkspaceError::Conflict);
            }
        } else if self.sftp().lstat(path).await.is_ok() {
            return Err(WorkspaceError::FilesExists);
        }
        self.cancelled()?;
        self.mutating();
        self.rename(temporary, path, base.is_some()).await?;
        self.report.commit = FileCommitState::Committed;
        self.report.recovery_paths.retain(|p| p != temporary);
        Ok(())
    }
    pub(super) async fn rename(
        &mut self,
        source: &str,
        target: &str,
        replace: bool,
    ) -> Result<(), WorkspaceError> {
        let result = if !replace {
            self.sftp().rename(source, target).await.map(Packet::Status)
        } else {
            if !self.session.1 {
                self.report.commit = FileCommitState::Unchanged;
                return Err(WorkspaceError::FilesUnsupported);
            }
            let mut data = vec![];
            for p in [source, target] {
                data.extend_from_slice(&(p.len() as u32).to_be_bytes());
                data.extend_from_slice(p.as_bytes());
            }
            self.sftp().extended("posix-rename@openssh.com", data).await
        };
        match result {
            Ok(Packet::Status(s)) if s.status_code == StatusCode::Ok => Ok(()),
            Ok(Packet::Status(s)) | Err(SftpError::Status(s)) => {
                // rename 的明确否定应答没有发布目标；失联/超时仍是未知。
                self.report.commit = FileCommitState::Unchanged;
                Err(match s.status_code {
                    StatusCode::OpUnsupported => WorkspaceError::FilesUnsupported,
                    StatusCode::PermissionDenied => WorkspaceError::FilesPermissionDenied,
                    StatusCode::NoSuchFile => WorkspaceError::NotFound,
                    _ => WorkspaceError::Conflict,
                })
            }
            Err(e) => Err(sftp_error(e)),
            _ => Err(WorkspaceError::ToolsUnavailable),
        }
    }
    async fn fill(&self, path: &str, bytes: &[u8]) -> Result<(), WorkspaceError> {
        let file = self
            .open(path, OpenFlags::WRITE | OpenFlags::TRUNCATE)
            .await?;
        for (i, chunk) in bytes.chunks(32768).enumerate() {
            self.cancelled()?;
            self.sftp()
                .write(file.id(), (i * 32768) as u64, chunk.to_vec())
                .await
                .map_err(sftp_error)?;
        }
        file.close().await
    }
    pub(super) async fn write(
        &mut self,
        path: &str,
        bytes: Vec<u8>,
        mode: FileWriteMode,
        expected: Option<String>,
    ) -> Result<Value, WorkspaceError> {
        let base = self.baseline(path, expected.as_deref()).await?;
        if matches!(mode, FileWriteMode::Create) && base.is_some() {
            return Err(WorkspaceError::FilesExists);
        }
        if matches!(mode, FileWriteMode::Append)
            && let Some(base) = base.as_ref()
        {
            self.resolve(path, FileEffect::Write, false, false).await?;
            let file = self
                .open(path, OpenFlags::WRITE | OpenFlags::APPEND)
                .await?;
            if self.version(path).await? != base.version {
                return Err(WorkspaceError::Conflict);
            }
            self.cancelled()?;
            self.mutating();
            self.sftp()
                .write(file.id(), base.attrs.size.unwrap_or(0), bytes.clone())
                .await
                .map_err(sftp_error)?;
            self.changed();
            file.close().await?;
            let after_version = self.version(path).await?;
            let check = self.open(path, OpenFlags::READ).await?;
            let mut prefix = Sha256::new();
            let mut offset = 0;
            let original = base.attrs.size.unwrap_or(0);
            while offset < original {
                let b = self
                    .bytes(&check, offset, (original - offset).min(32768) as u32)
                    .await?;
                if b.is_empty() {
                    return Err(WorkspaceError::Conflict);
                }
                offset += b.len() as u64;
                prefix.update(b);
            }
            let tail = self.bytes(&check, original, bytes.len() as u32).await?;
            check.close().await?;
            if format!("{:x}", prefix.finalize()) != base.hash || tail != bytes {
                return Err(WorkspaceError::Conflict);
            }
            let (size, hash) = self.digest(path).await?;
            if self.version(path).await? != after_version {
                return Err(WorkspaceError::Conflict);
            }
            if size != base.attrs.size.unwrap_or(0) + bytes.len() as u64 {
                return Err(WorkspaceError::Conflict);
            }
            self.preserved(path, base).await?;
            self.report.commit = FileCommitState::Committed;
            return Ok(json!({"size":size,"hash":hash}));
        }
        let temp = self.stage(path, base.as_ref()).await?;
        self.fill(&temp, &bytes).await?;
        let (_, hash) = self.digest(&temp).await?;
        if hash != format!("{:x}", Sha256::digest(&bytes)) {
            return Err(WorkspaceError::Conflict);
        }
        self.commit(&temp, path, base.as_ref()).await?;
        Ok(json!({"size":bytes.len(),"hash":hash}))
    }
    pub(super) async fn edit(
        &mut self,
        path: &str,
        edits: Vec<TextReplacement>,
        expected: &str,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(path, FileEffect::Write, false, false).await?;
        let file = self.open(path, OpenFlags::READ).await?;
        let mut bytes = vec![];
        loop {
            let b = self.bytes(&file, bytes.len() as u64, 32768).await?;
            if b.is_empty() {
                break;
            }
            bytes.extend(b);
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(WorkspaceError::FilesTooLarge);
            }
        }
        file.close().await?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err(WorkspaceError::Conflict);
        }
        let mut text = String::from_utf8(bytes).map_err(|_| WorkspaceError::FilesUnsupported)?;
        for edit in edits {
            if text.matches(&edit.before).count() != 1 {
                return Err(WorkspaceError::Conflict);
            }
            text = text.replacen(&edit.before, &edit.after, 1);
            if text.len() > 16 * 1024 * 1024 {
                return Err(WorkspaceError::FilesTooLarge);
            }
        }
        self.write(
            path,
            text.into_bytes(),
            FileWriteMode::Replace,
            Some(expected.into()),
        )
        .await
    }
    pub(super) async fn upload(
        &mut self,
        path: &str,
        mut input: Box<dyn AgentUpload>,
        overwrite: bool,
    ) -> Result<Value, WorkspaceError> {
        let base = self.baseline(path, None).await?;
        if base.is_some() && !overwrite {
            return Err(WorkspaceError::FilesExists);
        }
        let temp = self.stage(path, base.as_ref()).await?;
        let file = self
            .open(&temp, OpenFlags::WRITE | OpenFlags::TRUNCATE)
            .await?;
        let (size, hash) =
            crate::sftp_transfer::upload(self.sftp(), &file.id(), input.as_mut(), &self.control)
                .await?;
        file.close().await?;
        if self.digest(&temp).await? != (size, hash.clone()) {
            return Err(WorkspaceError::Conflict);
        }
        self.commit(&temp, path, base.as_ref()).await?;
        self.report.bytes = size;
        self.report.hash = Some(hash.clone());
        Ok(json!({"size":size,"hash":hash}))
    }
    pub(super) async fn download(
        &mut self,
        path: &str,
        mut output: Box<dyn AgentDownload>,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(path, FileEffect::Read, false, false).await?;
        let version = self.version(path).await?;
        let attrs = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if !attrs.is_regular() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let file = self.open(path, OpenFlags::READ).await?;
        let (size, hash) =
            crate::sftp_transfer::download(self.sftp(), &file.id(), output.as_mut(), &self.control)
                .await?;
        file.close().await?;
        if Some(size) != attrs.size
            || self.version(path).await? != version
            || self.digest(path).await? != (size, hash.clone())
        {
            return Err(WorkspaceError::Conflict);
        }
        self.cancelled()?;
        self.mutating();
        output.finish().await?;
        self.report.commit = FileCommitState::Committed;
        self.report.bytes = size;
        self.report.hash = Some(hash.clone());
        Ok(json!({"size":size,"hash":hash}))
    }
}
