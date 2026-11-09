use super::*;
impl Run<'_> {
    pub(super) async fn settings(&mut self) -> AgentResult<Value> {
        match self.operation.operation().clone() {
            RuntimeOperation::SystemGet {} => self
                .connection
                .overview()
                .await
                .map_err(|_| "system_info_unavailable"),
            RuntimeOperation::ProcessesList {} => self
                .connection
                .processes()
                .await
                .map_err(|_| "process_info_unavailable"),
            RuntimeOperation::PowerControl { action } => {
                self.report.effects = FileCommitState::Partial;
                let args = [
                    "--no-ask-password",
                    "--no-block",
                    match action {
                        PowerAction::Reboot => "reboot",
                        PowerAction::PowerOff => "poweroff",
                    },
                ]
                .map(str::to_owned);
                let result = self.exec("systemctl", &args, true).await?;
                if result.code != 0 {
                    return Err("power_request_failed");
                }
                self.report.outcome = Some(ExecutionOutcome::RequestAccepted {});
                Ok(json!({"action":action,"accepted":true,"effectConfirmed":false}))
            }
            RuntimeOperation::SystemSettingsUpdate { setting, value } => {
                let before = self.setting_state(setting).await?;
                let backup = self
                    .configuration_backup(setting.program(), before.as_bytes())
                    .await?;
                self.report.effects = FileCommitState::Partial;
                if self
                    .exec(setting.program(), &setting.arguments(&value)?, true)
                    .await?
                    .code
                    != 0
                {
                    return Err("native_setting_failed");
                }
                let state = self
                    .setting_state(setting)
                    .await
                    .map_err(|_| "native_setting_result_unknown")?;
                let matched = match setting {
                    SystemSetting::Locale => state
                        .split_whitespace()
                        .any(|s| s == format!("LANG={value}")),
                    _ => state.trim() == value,
                };
                if !matched {
                    return Err("native_setting_result_unknown");
                }
                Ok(
                    json!({"setting":setting,"value":value,"state":state,"before":before,"backup":backup}),
                )
            }
            RuntimeOperation::DnsUpdate {
                configuration,
                expected_hash,
            } => {
                const PATH: &str = "/etc/resolv.conf";
                let bytes = self
                    .read(PATH)
                    .await
                    .map_err(|_| "resolver_manager_requires_adapter")?;
                if bytes.len() > 65536 {
                    return Err("resolver_configuration_unsupported");
                }
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| "resolver_configuration_unsupported")?;
                if [
                    "NetworkManager",
                    "systemd-resolved",
                    "resolvconf",
                    "cloud-init",
                ]
                .iter()
                .any(|m| text.contains(m))
                {
                    return Err("resolver_manager_requires_adapter");
                }
                let next = configuration.replace(text)?;
                let current = hash(&bytes);
                let Some(expected) = expected_hash else {
                    return Ok(
                        json!({"changed":false,"compatible":true,"hash":current,"configuration":configuration,"target":PATH}),
                    );
                };
                if expected != current {
                    return Err("file_changed");
                }
                let backup = self.configuration_backup(PATH, &bytes).await?;
                self.put(PATH, next.as_bytes(), Some(expected), false, true)
                    .await?;
                Ok(
                    json!({"changed":true,"target":PATH,"hash":hash(next.as_bytes()),"backup":backup,"serviceReloaded":false}),
                )
            }
            _ => Err("invalid_native_operation"),
        }
    }
    async fn setting_state(&self, setting: SystemSetting) -> AgentResult<String> {
        let args = match setting {
            SystemSetting::Hostname => vec!["--static".into()],
            SystemSetting::Timezone => vec![
                "show".into(),
                "--property=Timezone".into(),
                "--value".into(),
            ],
            SystemSetting::Locale => vec!["status".into()],
        };
        let result = self.exec(setting.program(), &args, false).await?;
        if result.code != 0 {
            return Err("native_setting_query_failed");
        }
        Ok(result.text)
    }
}
