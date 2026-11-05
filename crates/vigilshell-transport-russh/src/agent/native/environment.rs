use super::*;
pub(in crate::agent) async fn inspect_environment(
    connection: &Connection,
) -> Result<ExecutionEnvironment, WorkspaceError> {
    let text=connection.control_bounded("exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /bin/sh -c 'uname -s; uname -m; id -u'".into(),1024).await?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != 3 {
        return Err(WorkspaceError::StreamUnavailable);
    }
    let mut result = ExecutionEnvironment {
        operating_system: lines[0].into(),
        architecture: lines[1].into(),
        account_uid: lines[2].parse().ok(),
        ..Default::default()
    };
    let session = connection
        .sftp
        .get_or_try_init(|| crate::sftp::open(&connection.handle))
        .await;
    result.sftp = session.is_ok();
    result.atomic_rename = session.as_ref().is_ok_and(|s| s.1);
    let control = FileExecutionControl::default();
    let programs = programs::probe(connection, &control)
        .await
        .map_err(|_| WorkspaceError::ToolsUnavailable)?;
    result.programs = programs.paths.keys().cloned().collect();
    result.package_managers = programs.managers.clone();
    let has = |name: &str| programs.path(name).is_ok();
    let mut supported = Vec::<&str>::new();
    if has("sh") {
        supported.extend(["command_run.command", "terminal_manage.open"]);
    }
    if result.sftp {
        supported.extend([
            "files_read.list",
            "files_read.stat",
            "files_read.read",
            "files_manage.mkdir",
            "files_remove",
            "files_manage.link",
            "files_manage.set_attributes",
            "files_transfer.upload",
            "files_transfer.download",
        ]);
        if has("stat") {
            supported.push("files_manage.move");
        }
        if result.atomic_rename && has("getfattr") {
            supported.extend(["files_write.write", "files_write.edit"]);
            if has("cp") && has("find") {
                supported.push("files_manage.copy");
            }
        }
        if has("id")
            && has("getent")
            && has("stat")
            && (has("sh") || has("bash") || has("python3") || has("node"))
        {
            supported.push("command_run.script");
        }
    }
    if result.operating_system == "Linux" {
        supported.push("server_inspect.overview");
        if has("ps") {
            supported.push("server_inspect.processes");
        }
    }
    if has("rpm") || has("dpkg-query") {
        supported.extend(["packages_query", "package_sources_query"]);
    }
    if programs.selected().is_ok() {
        supported.extend([
            "packages_manage.install",
            "packages_manage.upgrade",
            "packages_manage.remove",
        ]);
    }
    if result.sftp && (has("rpm") || has("apt-get")) {
        supported.extend([
            "package_sources_manage.enable",
            "package_sources_manage.disable",
            "package_sources_manage.remove",
        ]);
        if has("gpg") || has("gpg2") {
            supported.extend([
                "package_sources_manage.create",
                "package_sources_manage.update",
            ]);
        }
    }
    let probe = account::AccountProbe {
        connection,
        programs: &programs,
        control: &control,
    };
    result.system_service_manager = probe.service_manager_ready("probe.service").await.is_ok();
    result.user_service_manager = probe
        .service_manager_ready("user:probe.service")
        .await
        .is_ok();
    if programs.path("systemctl").is_ok() {
        supported.push("services_query.list");
    }
    if result.system_service_manager || result.user_service_manager {
        supported.extend([
            "services_query.status",
            "services_query.definition",
            "services_control.start",
            "services_control.stop",
            "services_control.restart",
            "services_control.reload",
            "services_control.enable",
            "services_control.disable",
        ]);
        if programs.path("journalctl").is_ok() {
            supported.push("services_query.logs");
        }
        if result.sftp && programs.path("systemd-analyze").is_ok() {
            supported.extend([
                "services_define.create",
                "services_define.update",
                "services_define.remove",
            ]);
        }
    }
    if result.system_service_manager {
        supported.extend(["power_control.reboot", "power_control.powerOff"]);
        for (name, tool) in [
            ("hostnamectl", "system_settings_manage.hostname"),
            ("timedatectl", "system_settings_manage.timezone"),
            ("localectl", "system_settings_manage.locale"),
        ] {
            if programs.path(name).is_ok() {
                supported.push(tool);
            }
        }
    }
    if result.sftp && result.atomic_rename && programs.path("getfattr").is_ok() {
        supported.push("system_settings_manage.dns");
    }
    supported.sort();
    supported.dedup();
    result.supported_tools = supported.into_iter().map(str::to_owned).collect();
    Ok(result)
}
