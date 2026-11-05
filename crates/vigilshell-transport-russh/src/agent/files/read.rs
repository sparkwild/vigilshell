use super::*;
impl Files<'_> {
    pub(super) async fn open(
        &self,
        path: &str,
        flags: OpenFlags,
    ) -> Result<Handle, WorkspaceError> {
        let id = self
            .sftp()
            .open(
                path,
                flags,
                FileAttributes {
                    permissions: Some(0o600),
                    ..empty_attrs()
                },
            )
            .await
            .map_err(sftp_error)?
            .handle;
        Ok(Handle {
            sftp: self.session.0.clone(),
            id: Some(id),
        })
    }
    pub(super) async fn bytes(
        &self,
        handle: &Handle,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, WorkspaceError> {
        self.cancelled()?;
        match self.sftp().read(handle.id(), offset, length).await {
            Ok(d) => Ok(d.data.to_vec()),
            Err(SftpError::Status(s)) if s.status_code == StatusCode::Eof => Ok(vec![]),
            Err(e) => Err(sftp_error(e)),
        }
    }
    pub(super) async fn digest(&self, path: &str) -> Result<(u64, String), WorkspaceError> {
        let before = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if !before.is_regular() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let file = self.open(path, OpenFlags::READ).await?;
        let mut hash = Sha256::new();
        let mut offset = 0;
        loop {
            let bytes = self.bytes(&file, offset, 32768).await?;
            if bytes.is_empty() {
                break;
            }
            offset += bytes.len() as u64;
            hash.update(bytes);
            if Some(offset) > before.size {
                return Err(WorkspaceError::Conflict);
            }
        }
        let after = self
            .sftp()
            .fstat(file.id())
            .await
            .map_err(sftp_error)?
            .attrs;
        file.close().await?;
        if before.size != Some(offset) || before.size != after.size || before.mtime != after.mtime {
            return Err(WorkspaceError::Conflict);
        }
        Ok((offset, format!("{:x}", hash.finalize())))
    }
    pub(super) async fn list(
        &self,
        path: &str,
        after: Option<String>,
        limit: u16,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(path, FileEffect::Read, false, false).await?;
        let (key, offset) = if let Some(cursor) = after {
            let (key, index) = cursor.rsplit_once(':').ok_or(WorkspaceError::Conflict)?;
            (
                key.to_owned(),
                index
                    .parse::<usize>()
                    .map_err(|_| WorkspaceError::Conflict)?,
            )
        } else {
            let version = self.version(path).await?;
            let (_, mut entries) =
                crate::sftp::list(&self.session.0, path.into(), &self.control.cancel).await?;
            if self.version(path).await? != version {
                return Err(WorkspaceError::Conflict);
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            entries.dedup_by(|a, b| a.name == b.name);
            let entries: Vec<_> = entries
                .into_iter()
                .map(|e| {
                    let entry = crate::sftp::agent_entry(e)?;
                    serde_json::to_value(entry).map_err(|_| WorkspaceError::InvalidInput)
                })
                .collect::<Result<_, _>>()?;
            let bytes = serde_json::to_vec(&entries)
                .map_err(|_| WorkspaceError::InvalidInput)?
                .len();
            if bytes > 4 * 1024 * 1024 {
                return Err(WorkspaceError::FilesTooLarge);
            }
            let key = new_id();
            let mut cache = self.connection.listings.lock().unwrap();
            cache.retain(|_, v| v.at.elapsed() < Duration::from_secs(60));
            while !cache.is_empty()
                && (cache.len() >= 8
                    || cache.values().map(|v| v.bytes).sum::<usize>() + bytes > 4 * 1024 * 1024)
            {
                let oldest = cache.iter().min_by_key(|(_, v)| v.at).unwrap().0.clone();
                cache.remove(&oldest);
            }
            cache.insert(
                key.clone(),
                Listing {
                    path: path.into(),
                    scope: self.operation.scope().into(),
                    entries,
                    at: std::time::Instant::now(),
                    bytes,
                },
            );
            (key, 0)
        };
        let cache = self.connection.listings.lock().unwrap();
        let snapshot = cache.get(&key).ok_or(WorkspaceError::Conflict)?;
        if snapshot.path != path
            || snapshot.scope != self.operation.scope()
            || snapshot.at.elapsed() > Duration::from_secs(60)
            || offset > snapshot.entries.len()
        {
            return Err(WorkspaceError::Conflict);
        }
        let maximum = offset
            .saturating_add(limit as usize)
            .min(snapshot.entries.len());
        let mut end = offset;
        let mut bytes = 0;
        while end < maximum {
            let size = serde_json::to_vec(&snapshot.entries[end])
                .map_err(|_| WorkspaceError::InvalidInput)?
                .len();
            if bytes + size > 384 * 1024 {
                break;
            }
            bytes += size;
            end += 1;
        }
        if end == offset && end < snapshot.entries.len() {
            return Err(WorkspaceError::FilesTooLarge);
        }
        Ok(
            json!({"entries":snapshot.entries[offset..end],"nextCursor":if end<snapshot.entries.len(){Some(format!("{key}:{end}"))}else{None}}),
        )
    }
    pub(super) async fn stat(&self, path: &str, hash: bool) -> Result<Value, WorkspaceError> {
        // stat 最后一级符号链接只展示本身；读取内容及写入一律不跟随链接。
        let parent = path
            .rsplit_once('/')
            .map_or("/", |(p, _)| if p.is_empty() { "/" } else { p });
        self.resolve(parent, FileEffect::Read, false, false).await?;
        self.operation
            .target(path, FileEffect::Read, false)
            .map_err(|_| WorkspaceError::FilesPermissionDenied)?;
        let a = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        let digest = if hash {
            self.resolve(path, FileEffect::Read, false, false).await?;
            Some(self.digest(path).await?.1)
        } else {
            None
        };
        Ok(
            json!({"path":path,"name":path.rsplit('/').next().unwrap_or(""),"kind":if a.is_dir(){"directory"}else if a.is_symlink(){"symlink"}else if a.is_regular(){"file"}else{"other"},"size":a.size,"mode":a.permissions.map(|m|m&0o7777),"uid":a.uid,"gid":a.gid,"modifiedSeconds":a.mtime,"hash":digest}),
        )
    }
    pub(super) async fn read(
        &self,
        path: &str,
        offset: u64,
        length: u32,
    ) -> Result<Value, WorkspaceError> {
        self.resolve(path, FileEffect::Read, false, false).await?;
        let before = self.version(path).await?;
        let a = self.sftp().lstat(path).await.map_err(sftp_error)?.attrs;
        if !a.is_regular() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let file = self.open(path, OpenFlags::READ).await?;
        let bytes = self.bytes(&file, offset, length).await?;
        file.close().await?;
        if self.version(path).await? != before {
            return Err(WorkspaceError::Conflict);
        }
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let end = offset + bytes.len() as u64;
        let eof = Some(end) >= a.size;
        Ok(
            json!({"data":STANDARD.encode(bytes),"nextOffset":end,"eof":eof,"chunkHash":hash,"hash":if offset==0&&eof{Some(hash)}else{None}}),
        )
    }
}
