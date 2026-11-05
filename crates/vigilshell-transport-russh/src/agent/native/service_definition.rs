use super::*;
use std::path::Path;
impl Run<'_> {
    pub(super) async fn configuration_backup(
        &mut self,
        path: &str,
        bytes: &[u8],
    ) -> AgentResult<String> {
        let root = self.operation.backup_root(&self.root)?;
        self.mkdir(&root).await?;
        let backup = format!("{root}/before");
        self.put(&backup, bytes, None, true, false).await?;
        let receipt = json!({"path":path,"hash":hash(bytes),"backup":backup,"stage":"prepared"});
        self.put(
            &format!("{root}/receipt.json"),
            receipt.to_string().as_bytes(),
            None,
            true,
            false,
        )
        .await?;
        self.report.recovery_paths.push(root);
        Ok(backup)
    }
    pub(super) async fn service_definition(&mut self, name: &str) -> AgentResult<Value> {
        let text = self.service_capture(name, "cat").await?;
        let path = self.service_path(name, None).await?;
        let mut paths = vec![path.clone()];
        let dir = format!("{path}.d");
        let sftp = self
            .connection
            .sftp
            .get_or_try_init(|| crate::sftp::open(&self.connection.handle))
            .await
            .map_err(|_| "service_definition_unavailable")?;
        match sftp.0.lstat(dir.clone()).await {
            Ok(attrs) if attrs.attrs.is_dir() => {
                let (_, entries) = crate::sftp::list(&sftp.0, dir, &self.control.cancel)
                    .await
                    .map_err(|_| "service_definition_unavailable")?;
                for entry in entries {
                    if validate_drop_in(&entry.name).is_ok() {
                        paths.push(entry.path);
                    }
                }
            }
            Err(russh_sftp::client::error::Error::Status(s))
                if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile => {}
            _ => return Err("service_definition_unavailable"),
        }
        if paths.len() > 128 {
            return Err("service_definition_limit");
        }
        paths.sort();
        let mut editable = vec![];
        for path in paths {
            match self.read(&path).await{Ok(bytes)=>editable.push(json!({"path":path,"hash":hash(&bytes),"text":String::from_utf8(bytes).map_err(|_|"service_definition_encoding")?})),Err("native_file_not_found")=>{},Err(e)=>return Err(e)}
        }
        Ok(
            json!({"target":name,"definition":text,"definitionHash":hash(text.as_bytes()),"editableFiles":editable}),
        )
    }
    pub(super) async fn define_service(
        &mut self,
        definition: NativeServiceDefinition,
        update: bool,
    ) -> AgentResult<Value> {
        definition.validate(update)?;
        let name = definition.target.identifier()?;
        let user = name.starts_with("user:");
        self.service_manager_ready(&name).await?;
        let (drop, text) = match &definition.content {
            ServiceDefinitionContent::Unit { text } => (None, text),
            ServiceDefinitionContent::DropIn { name, text } => (Some(name.as_str()), text),
        };
        let path = self.service_path(&name, drop).await?;
        let before = match self.read(&path).await {
            Ok(bytes) => Some(bytes),
            Err("native_file_not_found") => None,
            Err(e) => return Err(e),
        };
        if update && before.as_ref().map(|b| hash(b)) != definition.expected_hash {
            return Err("service_definition_changed");
        }
        if !update && before.is_some() {
            return Err("service_definition_exists");
        }
        let root = self.root.clone();
        self.mkdir(&root).await?;
        let unit = services::unit(&name);
        let original = if drop.is_some() {
            let state = self.service_capture(&name, "show").await?;
            let fragment = state
                .lines()
                .find_map(|l| l.strip_prefix("FragmentPath="))
                .filter(|p| p.starts_with('/'))
                .ok_or("service_definition_unavailable")?;
            self.read(fragment).await?
        } else {
            text.as_bytes().to_vec()
        };
        self.put(&format!("{root}/{unit}"), &original, None, true, false)
            .await?;
        if let Some(drop) = drop {
            let dir = format!("{root}/{unit}.d");
            self.mkdir(&dir).await?;
            let state = self.service_capture(&name, "show").await?;
            let paths = state
                .lines()
                .find_map(|l| l.strip_prefix("DropInPaths="))
                .unwrap_or("");
            if paths.split_whitespace().count() > 128 {
                return Err("service_definition_limit");
            }
            for source in paths.split_whitespace() {
                let file = Path::new(source)
                    .file_name()
                    .and_then(|p| p.to_str())
                    .ok_or("service_definition_unavailable")?;
                validate_drop_in(file)?;
                if file == drop {
                    continue;
                }
                let bytes = self.read(source).await?;
                self.put(&format!("{dir}/{file}"), &bytes, None, true, false)
                    .await?;
            }
            self.put(&format!("{dir}/{drop}"), text.as_bytes(), None, true, false)
                .await?;
        }
        let mut env = self.service_env(user).await?;
        env.push(("SYSTEMD_UNIT_PATH", format!("{root}:")));
        let mut args = vec!["--man=no".into(), "verify".into(), format!("{root}/{unit}")];
        if user {
            args.insert(0, "--user".into());
        }
        if self
            .exec_env("systemd-analyze", &args, true, &env)
            .await?
            .code
            != 0
        {
            return Err("invalid_service_definition");
        }
        let backup = if let Some(bytes) = &before {
            Some(self.configuration_backup(&path, bytes).await?)
        } else {
            None
        };
        let parent = Path::new(&path)
            .parent()
            .and_then(|p| p.to_str())
            .ok_or("invalid_service_target")?;
        self.file(
            RuntimeOperation::FilesMkdir {
                target: scoped(parent),
                parents: true,
            },
            true,
        )
        .await?;
        self.put(
            &path,
            text.as_bytes(),
            definition.expected_hash,
            !update,
            true,
        )
        .await?;
        if !update {
            self.file(
                RuntimeOperation::FilesSetAttributes {
                    target: scoped(&path),
                    mode: Some(0o644),
                    modified_seconds: None,
                },
                true,
            )
            .await?;
        }
        self.daemon_reload(&name).await?;
        Ok(
            json!({"target":definition.target,"path":path,"hash":hash(text.as_bytes()),"backup":backup,"serviceRestarted":false}),
        )
    }
}
