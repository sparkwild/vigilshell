use super::*;
use std::path::Path;
pub(super) struct Source {
    pub path: String,
    pub text: String,
    pub id: Option<String>,
    pub hash: String,
    pub reason: Option<String>,
}
impl Run<'_> {
    pub(super) async fn sources(&mut self) -> AgentResult<Value> {
        if let RuntimeOperation::PackageSourcesCreate { definition } =
            self.operation.operation().clone()
        {
            return self.create_source(definition).await;
        }
        let sources = self.catalog().await?;
        if matches!(
            self.operation.operation(),
            RuntimeOperation::PackageSourcesList {}
        ) {
            return Ok(
                json!({"sources":sources.into_iter().map(|s|json!({"id":s.id,"hash":s.hash,"path":s.path,"name":Path::new(&s.path).file_name().unwrap().to_string_lossy(),"supported":s.id.is_some(),"reason":s.reason})).collect::<Vec<_>>()}),
            );
        }
        let (id, expected) = match self.operation.operation() {
            RuntimeOperation::PackageSourcesManage {
                source_id,
                expected_hash,
                ..
            }
            | RuntimeOperation::PackageSourcesUpdate {
                source_id,
                expected_hash,
                ..
            }
            | RuntimeOperation::PackageSourcesRemove {
                source_id,
                expected_hash,
            } => (source_id.clone(), expected_hash.clone()),
            _ => return Err("package_source_unavailable"),
        };
        let source = sources
            .into_iter()
            .find(|s| s.id.as_ref() == Some(&id))
            .ok_or("package_source_unavailable")?;
        if source.hash != expected {
            return Err("package_source_changed");
        }
        self.change_source(source).await
    }
    pub(super) async fn catalog(&mut self) -> AgentResult<Vec<Source>> {
        let mut paths = Vec::new();
        let config = self
            .connection
            .sftp
            .get_or_try_init(|| crate::sftp::open(&self.connection.handle))
            .await
            .map_err(|_| "package_sources_unavailable")?;
        if config.0.lstat("/etc/apt/sources.list").await.is_ok() {
            paths.push("/etc/apt/sources.list".into());
        }
        for (root, suffix) in [
            ("/etc/apt/sources.list.d", vec![".sources", ".list"]),
            ("/etc/yum.repos.d", vec![".repo"]),
        ] {
            match config.0.lstat(root).await {
                Ok(_) => {}
                Err(russh_sftp::client::error::Error::Status(s))
                    if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
                {
                    continue;
                }
                Err(_) => return Err("package_sources_unavailable"),
            }
            let (_, entries) = crate::sftp::list(&config.0, root.into(), &self.control.cancel)
                .await
                .map_err(|_| "package_sources_unavailable")?;
            for entry in entries {
                if suffix.iter().any(|s| entry.name.ends_with(s)) {
                    paths.push(entry.path);
                }
            }
        }
        paths.sort();
        if paths.len() > 128 {
            return Err("package_source_limit");
        }
        let mut result = Vec::new();
        for path in paths {
            let text = String::from_utf8(self.read(&path).await?)
                .map_err(|_| "package_source_encoding")?;
            let id = self.source_identity(&path, &text).await;
            result.push(Source {
                path,
                hash: hash(text.as_bytes()),
                text,
                id: id.clone().ok(),
                reason: id.err().map(str::to_owned),
            });
        }
        Ok(result)
    }
    pub(super) async fn source_identity(&mut self, path: &str, text: &str) -> AgentResult<String> {
        package_sources::validate_document(Path::new(path), text)?;
        let mut keys = Vec::new();
        for key in package_sources::key_paths(text) {
            let session = self
                .connection
                .sftp
                .get()
                .ok_or("package_source_key_unavailable")?;
            let actual = session
                .0
                .realpath(key.clone())
                .await
                .map_err(|_| "package_source_key_unavailable")?
                .files
                .into_iter()
                .next()
                .ok_or("package_source_key_unavailable")?
                .filename;
            if !package_sources::key_path(&actual) {
                return Err("package_source_key_unavailable");
            }
            let bytes = self.read(&actual).await?;
            keys.push((key, hash(&bytes)));
        }
        package_sources::identity(Path::new(path), text, &keys)
    }
    pub(super) async fn backup(
        &mut self,
        path: &str,
        before: Option<&str>,
        after: Option<&str>,
    ) -> AgentResult<String> {
        let root = self.operation.backup_root(&self.root)?;
        self.mkdir(&root).await?;
        let backup = format!("{root}/source.before");
        if let Some(text) = before {
            self.put(&backup, text.as_bytes(), None, true, false)
                .await?;
        }
        let receipt = json!({"keyTrustScope":if path.ends_with(".repo"){"rpmDatabase"}else{"aptSource"},"beforeKeys":before.map(package_sources::key_paths),"afterKeys":after.map(package_sources::key_paths),"stage":"prepared","action":self.operation.operation().label(),"path":path,"backup":before.map(|_|&backup),"before":before.map(|s|hash(s.as_bytes())),"after":after.map(|s|hash(s.as_bytes()))});
        self.put(
            &format!("{root}/receipt.json"),
            receipt.to_string().as_bytes(),
            None,
            true,
            false,
        )
        .await?;
        Ok(backup)
    }
}
