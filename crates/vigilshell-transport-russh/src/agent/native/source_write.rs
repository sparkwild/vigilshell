use super::*;
use std::path::Path;
impl Run<'_> {
    async fn fingerprints(&mut self, key: &str) -> AgentResult<Vec<String>> {
        let root = self.root.clone();
        self.mkdir(&root).await?;
        let input = format!("{root}/public.asc");
        self.put(&input, key.as_bytes(), None, true, false).await?;
        let home = format!("{root}/gpg");
        self.mkdir(&home).await?;
        let program = if self.programs.path("gpg").is_ok() {
            "gpg"
        } else {
            "gpg2"
        };
        let result = self
            .exec(
                program,
                &[
                    "--no-options",
                    "--batch",
                    "--no-autostart",
                    "--homedir",
                    &home,
                    "--with-colons",
                    "--import-options",
                    "show-only",
                    "--dry-run",
                    "--import",
                    &input,
                ]
                .map(str::to_owned),
                false,
            )
            .await?;
        if result.code != 0 {
            return Err("package_key_invalid");
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "package_key_invalid")?
            .as_secs();
        let mut primary = false;
        let mut keys = Vec::new();
        for row in result.text.lines() {
            let fields: Vec<_> = row.split(':').collect();
            match fields.first().copied() {
                Some("sec" | "ssb") => return Err("package_private_key_forbidden"),
                Some("pub") => {
                    if fields.get(1).is_some_and(|s| matches!(*s, "r" | "e"))
                        || fields
                            .get(6)
                            .and_then(|s| s.parse::<u64>().ok())
                            .is_some_and(|t| t != 0 && t <= now)
                    {
                        return Err("package_key_invalid");
                    }
                    primary = true;
                }
                Some("fpr") if primary => {
                    let key = fields.get(9).ok_or("package_key_invalid")?;
                    if !matches!(key.len(), 40 | 64) || !key.bytes().all(|c| c.is_ascii_hexdigit())
                    {
                        return Err("package_key_invalid");
                    }
                    keys.push((*key).to_owned());
                    primary = false;
                }
                _ => {}
            }
        }
        if keys.len() != 1 {
            return Err("package_key_invalid");
        }
        Ok(keys)
    }
    async fn stage_key(&mut self, path: &str, key: &str) -> AgentResult<()> {
        let parent = Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .ok_or("invalid_package_source")?;
        self.mkdir(parent).await?;
        match self.read(path).await {
            Ok(bytes) => {
                if hash(&bytes) != hash(key.as_bytes()) {
                    return Err("package_source_key_changed");
                }
                return Ok(());
            }
            Err("native_file_not_found") => {}
            Err(error) => return Err(error),
        }
        self.report.recovery_paths.push(path.into());
        self.put(path, key.as_bytes(), None, true, true).await?;
        self.file(
            RuntimeOperation::FilesSetAttributes {
                target: scoped(path),
                mode: Some(0o644),
                modified_seconds: None,
            },
            true,
        )
        .await?;
        Ok(())
    }
    async fn import_key(&mut self, path: &str) -> AgentResult<()> {
        self.report.effects = FileCommitState::Partial;
        if self
            .exec("rpm", &["--import".into(), path.into()], true)
            .await?
            .code
            != 0
        {
            return Err("package_key_import_failed");
        }
        Ok(())
    }
    pub(super) async fn create_source(
        &mut self,
        definition: PackageSourceDefinition,
    ) -> AgentResult<Value> {
        let rpm = matches!(definition, PackageSourceDefinition::Rpm { .. });
        self.programs.path(if rpm { "rpm" } else { "apt-get" })?;
        let template = package_sources::template(&definition, None)?;
        let parent = Path::new(&template.path)
            .parent()
            .unwrap()
            .to_str()
            .unwrap();
        self.mkdir(parent).await?;
        let session = self
            .connection
            .sftp
            .get()
            .ok_or("package_sources_unavailable")?;
        match session.0.lstat(template.path.clone()).await {
            Ok(_) => return Err("package_source_exists"),
            Err(russh_sftp::client::error::Error::Status(s))
                if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile => {}
            Err(_) => return Err("package_sources_unavailable"),
        }
        let fingerprints = self.fingerprints(&template.key).await?;
        let text = format!(
            "# VigilShell source instance: {}\n{}",
            self.operation.id(),
            template.text
        );
        self.backup(&template.path, None, Some(&text)).await?;
        self.stage_key(&template.key_path, &template.key).await?;
        if rpm {
            self.import_key(&template.key_path).await?;
        }
        self.put(&template.path, text.as_bytes(), None, true, true)
            .await?;
        self.file(
            RuntimeOperation::FilesSetAttributes {
                target: scoped(&template.path),
                mode: Some(0o644),
                modified_seconds: None,
            },
            true,
        )
        .await?;
        let id = self.source_identity(&template.path, &text).await?;
        Ok(
            json!({"name":template.name,"path":template.path,"keyPath":template.key_path,"sourceId":id,"hash":hash(text.as_bytes()),"fingerprints":fingerprints,"enabled":false,"globalKeyImported":rpm,"keyTrustScope":if rpm{"rpmDatabase"}else{"aptSource"}}),
        )
    }
    pub(super) async fn change_source(&mut self, source: sources::Source) -> AgentResult<Value> {
        let action = self.operation.operation().clone();
        if self
            .source_identity(&source.path, &source.text)
            .await?
            .as_str()
            != source.id.as_deref().ok_or("package_source_changed")?
        {
            return Err("package_source_changed");
        }
        if matches!(action, RuntimeOperation::PackageSourcesRemove { .. }) {
            let backup = self.backup(&source.path, Some(&source.text), None).await?;
            if hash(&self.read(&source.path).await?) != source.hash {
                return Err("package_source_changed");
            }
            self.file(
                RuntimeOperation::FilesRemove {
                    target: scoped(&source.path),
                    recursive: false,
                },
                true,
            )
            .await?;
            return Ok(
                json!({"removed":true,"path":source.path,"backup":backup,"keysRetained":true,"packagesChanged":false}),
            );
        }
        if let RuntimeOperation::PackageSourcesManage { enabled, .. } = action {
            let updated = package_sources::toggle(Path::new(&source.path), &source.text, enabled)?;
            if updated == source.text {
                return Ok(json!({"changed":false,"sourceId":source.id,"hash":source.hash}));
            }
            let backup = self
                .backup(&source.path, Some(&source.text), Some(&updated))
                .await?;
            if self
                .source_identity(&source.path, &source.text)
                .await?
                .as_str()
                != source.id.as_deref().unwrap()
            {
                return Err("package_source_changed");
            }
            self.put(
                &source.path,
                updated.as_bytes(),
                Some(source.hash),
                false,
                true,
            )
            .await?;
            let id = self.source_identity(&source.path, &updated).await?;
            return Ok(
                json!({"changed":true,"sourceId":id,"hash":hash(updated.as_bytes()),"enabled":enabled,"backup":backup,"keysImported":false}),
            );
        }
        let RuntimeOperation::PackageSourcesUpdate { patch, .. } = action else {
            return Err("invalid_native_operation");
        };
        if !package_sources::managed(Path::new(&source.path)) {
            return Err("package_source_contract_unsupported");
        }
        let keys = package_sources::key_paths(&source.text);
        if keys.len() != 1 {
            return Err("package_source_contract_unsupported");
        }
        let old_key = &keys[0];
        let old_material =
            String::from_utf8(self.read(old_key).await?).map_err(|_| "package_key_invalid")?;
        let (mut definition, _) =
            package_sources::parse(Path::new(&source.path), &source.text, old_material.clone())?;
        let rotated = patch
            .public_key
            .as_ref()
            .is_some_and(|key| key != &old_material);
        match &mut definition {
            PackageSourceDefinition::Apt {
                url,
                suite,
                components,
                public_key,
                ..
            } => {
                if let Some(v) = patch.url {
                    *url = v;
                }
                if let Some(v) = patch.suite {
                    *suite = v;
                }
                if let Some(v) = patch.components {
                    *components = v;
                }
                if let Some(v) = patch.public_key {
                    *public_key = v;
                }
            }
            PackageSourceDefinition::Rpm {
                url, public_key, ..
            } => {
                if patch.suite.is_some() || patch.components.is_some() {
                    return Err("package_source_contract_unsupported");
                }
                if let Some(v) = patch.url {
                    *url = v;
                }
                if let Some(v) = patch.public_key {
                    *public_key = v;
                }
            }
        }
        let template =
            package_sources::template(&definition, if rotated { None } else { Some(old_key) })?;
        let fingerprints = self.fingerprints(&template.key).await?;
        let enabled = source
            .text
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("enabled=")
                    .map(|v| v.trim() == "1")
                    .or_else(|| {
                        line.trim()
                            .strip_prefix("Enabled:")
                            .map(|v| v.trim().eq_ignore_ascii_case("yes"))
                    })
            })
            .unwrap_or(true);
        let comments = source
            .text
            .lines()
            .filter(|l| l.trim_start().starts_with('#'))
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        let updated = package_sources::toggle(
            Path::new(&source.path),
            &(comments + &template.text),
            enabled,
        )?;
        if updated == source.text && !rotated {
            return Ok(json!({"changed":false,"sourceId":source.id,"hash":source.hash}));
        }
        let backup = self
            .backup(&source.path, Some(&source.text), Some(&updated))
            .await?;
        self.stage_key(&template.key_path, &template.key).await?;
        if self
            .source_identity(&source.path, &source.text)
            .await?
            .as_str()
            != source.id.as_deref().unwrap()
        {
            return Err("package_source_changed");
        }
        if rotated && matches!(definition, PackageSourceDefinition::Rpm { .. }) {
            self.import_key(&template.key_path).await?;
        }
        self.put(
            &source.path,
            updated.as_bytes(),
            Some(source.hash),
            false,
            true,
        )
        .await?;
        let id = self.source_identity(&source.path, &updated).await?;
        Ok(
            json!({"changed":true,"sourceId":id,"hash":hash(updated.as_bytes()),"enabled":enabled,"fingerprints":fingerprints,"keyPath":template.key_path,"retainedKeyPath":old_key,"backup":backup,"globalKeyImported":rotated&&matches!(definition,PackageSourceDefinition::Rpm{..})}),
        )
    }
}
