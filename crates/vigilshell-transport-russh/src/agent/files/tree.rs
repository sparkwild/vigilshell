use super::*;
impl Files<'_> {
    async fn remove_target(&self, path: &str, recursive: bool) -> Result<(), WorkspaceError> {
        self.operation
            .target(path, FileEffect::Remove, recursive)
            .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
        let parent = path
            .rsplit_once('/')
            .map_or("/", |(p, _)| if p.is_empty() { "/" } else { p });
        self.resolve(parent, FileEffect::Read, false, false).await?;
        Ok(())
    }

    async fn tree(
        &self,
        path: &str,
        effect: FileEffect,
    ) -> Result<Vec<(String, FileAttributes, String)>, WorkspaceError> {
        let root_device = self
            .version(path)
            .await?
            .split(':')
            .next()
            .unwrap_or("")
            .to_owned();
        let mut pending = vec![(path.to_string(), 0)];
        let mut entries = vec![];
        while let Some((path, depth)) = pending.pop() {
            self.cancelled()?;
            if depth > 64 || entries.len() >= 20000 {
                return Err(WorkspaceError::FilesTooLarge);
            }
            if effect == FileEffect::Remove {
                self.remove_target(&path, true).await?;
            } else {
                self.resolve(&path, effect, true, false).await?;
            }
            let a = self.sftp().lstat(&path).await.map_err(sftp_error)?.attrs;
            if !(a.is_dir() || a.is_regular() || effect == FileEffect::Remove && a.is_symlink()) {
                return Err(WorkspaceError::FilesUnsupported);
            }
            let version = self.version(&path).await?;
            if version.split(':').next() != Some(root_device.as_str()) {
                return Err(WorkspaceError::FilesUnsupported);
            }
            if a.is_dir() {
                let (_, children) =
                    crate::sftp::list(&self.session.0, path.clone(), &self.control.cancel).await?;
                for child in children {
                    pending.push((child.path, depth + 1));
                }
            }
            entries.push((path, a, version));
        }
        Ok(entries)
    }
    pub(super) async fn mkdir(
        &mut self,
        path: &str,
        parents: bool,
    ) -> Result<Value, WorkspaceError> {
        let mut paths = vec![path.to_owned()];
        if parents {
            let mut current = path.to_owned();
            while self.sftp().lstat(&current).await.is_err() {
                let parent = current.rsplit_once('/').unwrap().0;
                if parent.is_empty() {
                    break;
                }
                paths.push(parent.into());
                current = parent.into();
            }
            paths.reverse();
        }
        for path in paths {
            if parents && let Ok(a) = self.sftp().lstat(&path).await {
                if !a.attrs.is_dir() || a.attrs.is_symlink() {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                continue;
            }
            self.resolve(&path, FileEffect::Write, false, true).await?;
            self.cancelled()?;
            self.mutating();
            self.sftp()
                .mkdir(
                    &path,
                    FileAttributes {
                        permissions: Some(0o700),
                        ..empty_attrs()
                    },
                )
                .await
                .map_err(sftp_error)?;
            self.changed();
        }
        self.report.commit = FileCommitState::Committed;
        self.stat(path, false).await
    }
    pub(super) async fn remove(
        &mut self,
        path: &str,
        recursive: bool,
    ) -> Result<Value, WorkspaceError> {
        let a = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if a.is_symlink() {
            self.operation
                .target(path, FileEffect::Remove, false)
                .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
            let parent = path
                .rsplit_once('/')
                .map_or("/", |(p, _)| if p.is_empty() { "/" } else { p });
            self.resolve(parent, FileEffect::Read, false, false).await?;
            self.cancelled()?;
            self.mutating();
            self.sftp().remove(path).await.map_err(sftp_error)?;
            self.report.commit = FileCommitState::Committed;
            return Ok(json!({"removed":true}));
        }
        self.resolve(path, FileEffect::Remove, recursive, false)
            .await?;
        let entries = if recursive {
            self.tree(path, FileEffect::Remove).await?
        } else {
            vec![(
                path.into(),
                self.sftp().lstat(path).await.map_err(sftp_error)?.attrs,
                self.version(path).await?,
            )]
        };
        // 整棵树先检查后写；执行中每个端点再次核对，不跨越链接。
        for (p, _, v) in &entries {
            if self.version(p).await? != *v {
                return Err(WorkspaceError::Conflict);
            }
        }
        for (p, a, version) in entries.into_iter().rev() {
            self.cancelled()?;
            self.remove_target(&p, false).await?;
            let current = self.version(&p).await?;
            if (a.is_dir() && current.split(':').take(2).ne(version.split(':').take(2)))
                || (!a.is_dir() && current != version)
            {
                return Err(WorkspaceError::Conflict);
            }
            if let Some(expected) = self.operation.removal_hash()
                && (a.is_dir()
                    || self.digest(&p).await?.1 != expected
                    || self.version(&p).await? != version)
            {
                return Err(WorkspaceError::Conflict);
            }
            self.mutating();
            if a.is_dir() {
                self.sftp().rmdir(p).await.map_err(sftp_error)?;
            } else {
                self.sftp().remove(p).await.map_err(sftp_error)?;
            }
            self.changed();
        }
        self.report.commit = FileCommitState::Committed;
        Ok(json!({"removed":true}))
    }
    pub(super) async fn move_file(
        &mut self,
        source: &str,
        target: &str,
        overwrite: bool,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(source, FileEffect::Remove, true, false)
            .await?;
        self.resolve(target, FileEffect::Write, true, true).await?;
        if source == target || target.starts_with(&format!("{source}/")) {
            return Err(WorkspaceError::InvalidInput);
        }
        let entries = self.tree(source, FileEffect::Remove).await?;
        let parent = target.rsplit_once('/').unwrap().0;
        let destination_version = self
            .version(if parent.is_empty() { "/" } else { parent })
            .await?;
        if entries[0].2.split(':').next() != destination_version.split(':').next() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let destination = match self.sftp().lstat(target).await {
            Ok(a) => Some(a.attrs),
            Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => None,
            Err(e) => return Err(sftp_error(e)),
        };
        if destination.is_some() && !overwrite {
            return Err(WorkspaceError::FilesExists);
        }
        if let Some(target_attrs) = &destination {
            if !self.session.1
                || target_attrs.is_dir() != entries[0].1.is_dir()
                || !(target_attrs.is_dir() || target_attrs.is_regular())
            {
                return Err(WorkspaceError::FilesUnsupported);
            }
            if target_attrs.is_dir()
                && !crate::sftp::list(&self.session.0, target.into(), &self.control.cancel)
                    .await?
                    .1
                    .is_empty()
            {
                return Err(WorkspaceError::FilesUnsupported);
            }
        }
        let target_version = if destination.is_some() {
            Some(self.version(target).await?)
        } else {
            None
        };
        for (p, _, v) in &entries {
            if self.version(p).await? != *v {
                return Err(WorkspaceError::Conflict);
            }
        }
        if let Some(v) = target_version
            && self.version(target).await? != v
        {
            return Err(WorkspaceError::Conflict);
        }
        self.cancelled()?;
        self.mutating();
        self.rename(source, target, destination.is_some()).await?;
        self.report.commit = FileCommitState::Committed;
        Ok(json!({"moved":true}))
    }
    pub(super) async fn copy(
        &mut self,
        source: &str,
        target: &str,
        recursive: bool,
        overwrite: bool,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(source, FileEffect::Read, recursive, false)
            .await?;
        self.resolve(target, FileEffect::Write, recursive, true)
            .await?;
        if source == target || target.starts_with(&format!("{source}/")) {
            return Err(WorkspaceError::InvalidInput);
        }
        let entries = self.tree(source, FileEffect::Read).await?;
        if entries[0].1.is_dir() && !recursive {
            return Err(WorkspaceError::FilesUnsupported);
        }
        if !entries[0].1.is_dir() {
            let base = self.baseline(target, None).await?;
            if base.is_some() && !overwrite {
                return Err(WorkspaceError::FilesExists);
            }
            let temp = self.stage(target, None).await?;
            self.native(format!("/usr/bin/cp --remove-destination --no-dereference --preserve=mode,ownership,timestamps,xattr -- {} {}",shell_quote(source),shell_quote(&temp))).await?;
            let copied = self.sftp().lstat(&temp).await.map_err(sftp_error)?.attrs;
            let source_attrs = &entries[0].1;
            if copied.uid != source_attrs.uid
                || copied.gid != source_attrs.gid
                || copied.permissions != source_attrs.permissions
            {
                return Err(WorkspaceError::FilesUnsupported);
            }
            let original = self.digest(source).await?;
            if self.digest(&temp).await? != original
                || self.xattrs(source).await? != self.xattrs(&temp).await?
            {
                return Err(WorkspaceError::Conflict);
            }
            if self.version(source).await? != entries[0].2 {
                return Err(WorkspaceError::Conflict);
            }
            // copy 保留源元数据；覆盖目标仍须比较目标版本，但不将旧目标属性写回副本。
            if let Some(base) = &base {
                return self.commit_copy(&temp, target, base, &original.1).await;
            }
            self.commit(&temp, target, None).await?;
            return Ok(json!({"copied":true,"hash":original.1}));
        }
        let destination = match self.sftp().lstat(target).await {
            Ok(a) => {
                if !overwrite {
                    return Err(WorkspaceError::FilesExists);
                }
                if !a.attrs.is_dir()
                    || !self.session.1
                    || !crate::sftp::list(&self.session.0, target.into(), &self.control.cancel)
                        .await?
                        .1
                        .is_empty()
                {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                Some(self.version(target).await?)
            }
            Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => None,
            Err(e) => return Err(sftp_error(e)),
        };
        // 非空目录合并不具备原子发布能力；新目录/空目录可在同父目录准备后 rename。
        let temp = self.operation.staging_path(target);
        self.sftp()
            .mkdir(
                &temp,
                FileAttributes {
                    permissions: Some(0o700),
                    ..empty_attrs()
                },
            )
            .await
            .map_err(sftp_error)?;
        self.report.recovery_paths.push(temp.clone());
        self.native(format!(
            "/usr/bin/cp -a --no-dereference -- {}/. {}/",
            shell_quote(source),
            shell_quote(&temp)
        ))
        .await?;
        let copied = self.tree(&temp, FileEffect::Write).await?;
        if copied.len() != entries.len() {
            return Err(WorkspaceError::Conflict);
        }
        for (p, a, v) in &entries {
            let destination = format!("{}{}", temp, &p[source.len()..]);
            let copied = self
                .sftp()
                .lstat(&destination)
                .await
                .map_err(sftp_error)?
                .attrs;
            if a.uid != copied.uid
                || a.gid != copied.gid
                || a.permissions != copied.permissions
                || self.xattrs(p).await? != self.xattrs(&destination).await?
                || self.version(p).await? != *v
            {
                return Err(WorkspaceError::Conflict);
            }
            if a.is_regular() && self.digest(p).await? != self.digest(&destination).await? {
                return Err(WorkspaceError::Conflict);
            }
        }
        self.cancelled()?;
        self.resolve(target, FileEffect::Write, true, true).await?;
        if let Some(v) = &destination
            && self.version(target).await? != *v
        {
            return Err(WorkspaceError::Conflict);
        }
        self.mutating();
        self.rename(&temp, target, destination.is_some()).await?;
        self.report.recovery_paths.retain(|p| p != &temp);
        self.report.commit = FileCommitState::Committed;
        Ok(json!({"copied":true}))
    }
    async fn commit_copy(
        &mut self,
        temp: &str,
        target: &str,
        base: &super::write::Baseline,
        hash: &str,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(target, FileEffect::Write, false, false)
            .await?;
        if self.version(target).await? != base.version || self.digest(target).await?.1 != base.hash
        {
            return Err(WorkspaceError::Conflict);
        }
        self.cancelled()?;
        self.mutating();
        self.rename(temp, target, true).await?;
        self.report.recovery_paths.retain(|p| p != temp);
        self.report.commit = FileCommitState::Committed;
        Ok(json!({"copied":true,"hash":hash}))
    }
    pub(super) async fn link(
        &mut self,
        target: &str,
        destination: &str,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(target, FileEffect::Write, false, true).await?;
        self.operation
            .target(destination, FileEffect::Read, false)
            .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
        if self.sftp().lstat(target).await.is_ok() {
            return Err(WorkspaceError::FilesExists);
        }
        self.cancelled()?;
        self.mutating();
        self.sftp()
            .symlink(destination, target)
            .await
            .map_err(sftp_error)?;
        self.report.commit = FileCommitState::Committed;
        Ok(json!({"linked":true}))
    }
    pub(super) async fn attributes(
        &mut self,
        path: &str,
        mode: Option<u32>,
        modified: Option<u64>,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(path, FileEffect::Write, false, false).await?;
        let old = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if modified.is_some() && old.atime.is_none() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        if old.is_regular() && self.version(path).await?.split(':').nth(2) != Some("1") {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let modified = modified
            .map(u32::try_from)
            .transpose()
            .map_err(|_| WorkspaceError::FilesUnsupported)?;
        self.cancelled()?;
        self.mutating();
        self.sftp()
            .setstat(
                path,
                FileAttributes {
                    permissions: mode,
                    mtime: modified,
                    atime: modified.and(old.atime),
                    ..empty_attrs()
                },
            )
            .await
            .map_err(sftp_error)?;
        self.changed();
        let a = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if mode.is_some_and(|m| a.permissions.map(|p| p & 0o777) != Some(m))
            || modified.is_some_and(|m| a.mtime != Some(m))
        {
            return Err(WorkspaceError::Conflict);
        }
        self.report.commit = FileCommitState::Committed;
        self.stat(path, false).await
    }
}
