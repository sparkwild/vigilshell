use super::*;

/// 固定账号与服务管理器查询；不持有业务操作或暂存目录。
pub(super) struct AccountProbe<'a> {
    pub connection: &'a Connection,
    pub programs: &'a programs::Programs,
    pub control: &'a FileExecutionControl,
}
impl AccountProbe<'_> {
    async fn exec(
        &self,
        name: &str,
        args: &[String],
        environment: &[(&str, String)],
    ) -> AgentResult<process::ResultData> {
        if self.control.cancel.load(Ordering::Acquire) {
            return Err("native_cancelled");
        }
        let body = process::command(self.programs.path(name)?, args, environment);
        process::capture(
            self.connection,
            body,
            Arc::new(process::Silent),
            self.control,
            false,
        )
        .await
    }
    pub(super) async fn account(&self) -> AgentResult<(u32, String)> {
        let result = self.exec("id", &["-u".into()], &[]).await?;
        if result.code != 0 {
            return Err("native_account_unavailable");
        }
        let uid = result
            .text
            .trim()
            .parse::<u32>()
            .map_err(|_| "native_account_unavailable")?;
        let result = self
            .exec("getent", &["passwd".into(), uid.to_string()], &[])
            .await?;
        let fields: Vec<_> = result.text.trim_end().split(':').collect();
        if result.code != 0 || fields.len() != 7 || fields[2].parse::<u32>().ok() != Some(uid) {
            return Err("native_account_unavailable");
        }
        let home = fields[5];
        if !home.starts_with('/') || normalize_path("/", home)? != home || home == "/" {
            return Err("native_account_unavailable");
        }
        Ok((uid, home.into()))
    }
    pub(super) async fn service_env(&self, user: bool) -> AgentResult<Vec<(&'static str, String)>> {
        let (uid, home) = self.account().await?;
        let mut result = vec![("HOME", home)];
        if user {
            let root = format!("/run/user/{uid}");
            let sftp = self
                .connection
                .sftp
                .get_or_try_init(|| crate::sftp::open(&self.connection.handle))
                .await
                .map_err(|_| "user_service_manager_unavailable")?;
            let dir = sftp
                .0
                .lstat(root.clone())
                .await
                .map_err(|_| "user_service_manager_unavailable")?
                .attrs;
            let bus = sftp
                .0
                .lstat(format!("{root}/bus"))
                .await
                .map_err(|_| "user_service_manager_unavailable")?
                .attrs;
            if !dir.is_dir()
                || dir.uid != Some(uid)
                || dir.permissions.is_none_or(|m| m & 0o077 != 0)
                || bus.uid != Some(uid)
                || bus.permissions.is_none_or(|m| m & 0o170000 != 0o140000)
            {
                return Err("user_service_manager_unavailable");
            }
            result.extend([
                ("XDG_RUNTIME_DIR", root.clone()),
                ("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={root}/bus")),
            ]);
        }
        Ok(result)
    }
    pub(super) async fn service_manager_ready(&self, name: &str) -> AgentResult<()> {
        let user = name.starts_with("user:");
        let args = [
            "--no-ask-password",
            if user { "--user" } else { "--system" },
            "show",
            "--property=Version",
            "--value",
        ]
        .map(str::to_owned);
        let result = self
            .exec("systemctl", &args, &self.service_env(user).await?)
            .await?;
        if result.code != 0 || result.text.trim().is_empty() {
            return Err("service_manager_unavailable");
        }
        Ok(())
    }
}
impl Run<'_> {
    pub(super) fn account_probe(&self) -> AccountProbe<'_> {
        AccountProbe {
            connection: self.connection,
            programs: &self.programs,
            control: &self.control,
        }
    }
    pub(super) async fn account(&self) -> AgentResult<(u32, String)> {
        self.account_probe().account().await
    }
    pub(super) async fn service_env(&self, user: bool) -> AgentResult<Vec<(&'static str, String)>> {
        self.account_probe().service_env(user).await
    }
    pub(super) async fn service_exec(
        &self,
        program: &str,
        args: &[String],
        stream: bool,
        user: bool,
    ) -> AgentResult<process::ResultData> {
        self.exec_env(program, args, stream, &self.service_env(user).await?)
            .await
    }
    pub(super) async fn service_path(&self, name: &str, drop: Option<&str>) -> AgentResult<String> {
        let (_, home) = self.account().await?;
        if self.root
            != format!(
                "{home}/.local/share/vigilshell/native-staging/{}",
                self.operation.id()
            )
        {
            return Err("native_account_home_changed");
        }
        native_service_path(name, &home, drop)
    }
}
