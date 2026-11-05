use super::*;
pub(super) fn unit(name: &str) -> &str {
    name.strip_prefix("user:").unwrap_or(name)
}
pub(super) fn args(name: &str, action: &str) -> Vec<String> {
    vec![
        "--no-pager".into(),
        "--no-ask-password".into(),
        if name.starts_with("user:") {
            "--user"
        } else {
            "--system"
        }
        .into(),
        action.into(),
        "--".into(),
        unit(name).into(),
    ]
}
impl Run<'_> {
    pub(super) async fn service_manager_ready(&self, name: &str) -> AgentResult<()> {
        self.account_probe().service_manager_ready(name).await
    }

    pub(super) async fn service_capture(&self, name: &str, action: &str) -> AgentResult<String> {
        let result = self
            .service_exec(
                "systemctl",
                &args(name, action),
                false,
                name.starts_with("user:"),
            )
            .await?;
        if result.code != 0 {
            return Err("native_service_query_failed");
        }
        Ok(result.text)
    }
    pub(super) async fn daemon_reload(&self, name: &str) -> AgentResult<()> {
        let args = vec![
            "--no-ask-password".into(),
            if name.starts_with("user:") {
                "--user"
            } else {
                "--system"
            }
            .into(),
            "daemon-reload".into(),
        ];
        if self
            .service_exec("systemctl", &args, true, name.starts_with("user:"))
            .await?
            .code
            != 0
        {
            return Err("service_reload_failed");
        }
        Ok(())
    }
    pub(super) async fn services(&mut self) -> AgentResult<Value> {
        match self.operation.operation().clone() {
            RuntimeOperation::ServicesList {} => {
                let mut entries = vec![];
                let mut managers = vec![];
                for (user, manager) in [(false, "system"), (true, "currentUser")] {
                    let args = [
                        "--no-pager",
                        "--no-legend",
                        if user { "--user" } else { "--system" },
                        "list-unit-files",
                        "--type=service",
                    ]
                    .map(str::to_owned);
                    match self.service_exec("systemctl", &args, false, user).await {
                        Ok(result) if result.code == 0 => {
                            managers.push(json!({"manager": manager, "available": true}));
                            for line in result.text.lines() {
                                let mut fields = line.split_whitespace();
                                if let (Some(name), Some(state)) = (fields.next(), fields.next())
                                    && valid_service_name(name) && name.ends_with(".service") {
                                    entries.push(json!({"manager":manager,"name":name,"enabledState":state}));
                                }
                            }
                        }
                        Ok(_) => managers.push(json!({"manager":manager,"available":false,"reason":"native_service_query_failed"})),
                        Err("user_service_manager_unavailable") => managers.push(json!({"manager":manager,"available":false,"reason":"user_service_manager_unavailable"})),
                        Err(error) => return Err(error),
                    }
                }
                Ok(json!({"services":entries,"managers":managers}))
            }
            RuntimeOperation::ServicesStatus { name } => {
                Ok(json!({"target":name,"state":self.service_capture(&name,"show").await?}))
            }
            RuntimeOperation::ServicesDefinition { name } => self.service_definition(&name).await,
            RuntimeOperation::ServicesLogs { name, lines } => {
                let user = name.starts_with("user:");
                let mut args = vec![
                    "--no-pager".into(),
                    "--output=short-iso".into(),
                    format!("--lines={lines}"),
                    format!("--{}unit={}", if user { "user-" } else { "" }, unit(&name)),
                ];
                if user {
                    args.push("--user".into());
                }
                let result = self.service_exec("journalctl", &args, true, user).await?;
                if result.code != 0 {
                    return Err("service_logs_failed");
                }
                Ok(json!({"exitCode":0}))
            }
            RuntimeOperation::ServicesControl { name, action } => {
                self.service_env(name.starts_with("user:")).await?;
                self.report.effects = FileCommitState::Partial;
                if self
                    .service_exec(
                        "systemctl",
                        &args(&name, action.as_str()),
                        true,
                        name.starts_with("user:"),
                    )
                    .await?
                    .code
                    != 0
                {
                    return Err("native_service_failed");
                }
                let state = self
                    .service_capture(&name, "show")
                    .await
                    .map_err(|_| "native_service_result_unknown")?;
                Ok(json!({"target":name,"action":action,"state":state}))
            }
            RuntimeOperation::ServicesSetEnabled {
                name,
                enabled,
                expected_version,
            } => {
                if let Some(expected) = expected_version
                    && hash(self.service_capture(&name, "cat").await?.as_bytes()) != expected
                {
                    return Err("service_definition_changed");
                }
                self.service_env(name.starts_with("user:")).await?;
                self.report.effects = FileCommitState::Partial;
                if self
                    .service_exec(
                        "systemctl",
                        &args(&name, if enabled { "enable" } else { "disable" }),
                        true,
                        name.starts_with("user:"),
                    )
                    .await?
                    .code
                    != 0
                {
                    return Err("native_service_failed");
                }
                let state = self
                    .service_capture(&name, "show")
                    .await
                    .map_err(|_| "native_service_result_unknown")?;
                Ok(json!({"target":name,"enabled":enabled,"state":state,"serviceRestarted":false}))
            }
            RuntimeOperation::ServicesRegister { definition } => {
                self.define_service(definition, false).await
            }
            RuntimeOperation::ServicesUpdate { definition } => {
                self.define_service(definition, true).await
            }
            RuntimeOperation::ServicesUnregister {
                name,
                drop_in,
                expected_hash,
            } => {
                self.service_manager_ready(&name).await?;
                let path = self.service_path(&name, drop_in.as_deref()).await?;
                let bytes = self.read(&path).await?;
                if hash(&bytes) != expected_hash {
                    return Err("service_definition_changed");
                }
                let backup = self.configuration_backup(&path, &bytes).await?;
                if hash(&self.read(&path).await?) != expected_hash {
                    return Err("service_definition_changed");
                }
                self.file(
                    RuntimeOperation::FilesRemove {
                        target: scoped(&path),
                        recursive: false,
                    },
                    true,
                )
                .await?;
                self.daemon_reload(&name).await?;
                Ok(json!({"removed":path,"backup":backup,"serviceStopped":false}))
            }
            _ => Err("invalid_native_operation"),
        }
    }
}
