use super::*;
pub(super) struct Programs {
    pub(super) paths: BTreeMap<String, String>,
    pub managers: Vec<PackageManagerSupport>,
    selected: Option<(PackageManagerKind, String)>,
}
impl Programs {
    pub fn path(&self, name: &str) -> AgentResult<&str> {
        self.paths
            .get(name)
            .map(String::as_str)
            .ok_or("native_program_unavailable")
    }
    pub fn selected(&self) -> AgentResult<(PackageManagerKind, &str)> {
        self.selected
            .as_ref()
            .map(|(k, p)| (*k, p.as_str()))
            .ok_or("package_backend_unavailable")
    }
}
pub(super) async fn probe(
    connection: &Connection,
    control: &FileExecutionControl,
) -> AgentResult<Programs> {
    let mut paths = BTreeMap::new();
    for name in [
        "apt-get",
        "dpkg-query",
        "dpkg-deb",
        "dnf",
        "dnf5",
        "rpm",
        "gpg",
        "gpg2",
        "cp",
        "systemctl",
        "journalctl",
        "systemd-analyze",
        "hostnamectl",
        "timedatectl",
        "localectl",
        "id",
        "getent",
        "sh",
        "bash",
        "python3",
        "node",
        "cat",
        "head",
        "df",
        "uname",
        "hostname",
        "ps",
        "stat",
        "getfattr",
        "setfattr",
        "find",
        "sha256sum",
        "base64",
    ] {
        let mut body = String::new();
        for root in ["/usr/bin", "/usr/sbin", "/bin", "/sbin"] {
            let path = format!("{root}/{name}");
            body.push_str(&format!(
                "if test -f {path} && test -x {path}; then printf '%s' {path}; exit 0; fi; "
            ));
        }
        body.push_str("exit 1");
        let result =
            process::capture(connection, body, Arc::new(process::Silent), control, false).await?;
        if result.code == 0
            && !result.text.is_empty()
            && let Ok(path) = trusted_program(connection, &result.text).await
        {
            paths.insert(name.into(), path);
        }
    }
    let mut managers = Vec::new();
    let mut selected = None;
    for (name, default) in [
        ("apt-get", PackageManagerKind::Apt),
        ("dnf", PackageManagerKind::Dnf4),
        ("dnf5", PackageManagerKind::Dnf5),
    ] {
        let Some(path) = paths.get(name) else {
            continue;
        };
        let result = process::capture(
            connection,
            format!(
                "exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C {} --version",
                shell_quote(path)
            ),
            Arc::new(process::Silent),
            control,
            false,
        )
        .await?;
        let kind = if name == "dnf"
            && (result.text.contains("dnf5")
                || result
                    .text
                    .lines()
                    .next()
                    .is_some_and(|l| l.starts_with("5.")))
        {
            PackageManagerKind::Dnf5
        } else {
            default
        };
        if selected.is_none() && result.code == 0 {
            selected = Some((kind, name.into()));
        }
        if !managers
            .iter()
            .any(|m: &PackageManagerSupport| m.kind == kind)
        {
            managers.push(PackageManagerSupport {
                kind,
                available: result.code == 0,
                version: result.text.lines().next().map(str::to_owned),
                reason: (result.code != 0).then(|| "package_probe_failed".into()),
            });
        }
    }
    Ok(Programs {
        paths,
        managers,
        selected,
    })
}
pub(in crate::agent) async fn trusted_program(
    connection: &Connection,
    path: &str,
) -> AgentResult<String> {
    let session = connection
        .sftp
        .get_or_try_init(|| crate::sftp::open(&connection.handle))
        .await
        .map_err(|_| "native_program_unavailable")?;
    let actual = session
        .0
        .realpath(path)
        .await
        .map_err(|_| "native_program_unavailable")?
        .files
        .into_iter()
        .next()
        .ok_or("native_program_unavailable")?
        .filename;
    if normalize_path("/", &actual)? != actual || !actual.starts_with('/') {
        return Err("native_program_unavailable");
    }
    let mut current = actual.as_str();
    let mut program = true;
    loop {
        let attrs = session
            .0
            .lstat(current)
            .await
            .map_err(|_| "native_program_unavailable")?
            .attrs;
        if attrs.uid != Some(0)
            || attrs
                .permissions
                .is_none_or(|m| m & 0o022 != 0 || program && m & 0o111 == 0)
            || (program && !attrs.is_regular())
            || (!program && !attrs.is_dir())
        {
            return Err("native_program_unavailable");
        }
        if current == "/" {
            break;
        }
        current =
            current.rsplit_once('/').map_or(
                "/",
                |(parent, _)| if parent.is_empty() { "/" } else { parent },
            );
        program = false;
    }
    Ok(actual)
}
