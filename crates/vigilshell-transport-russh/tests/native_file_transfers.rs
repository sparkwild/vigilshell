//! Explicit disposable loopback SSH target only; no stored connections or credentials.
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64},
    },
};
use vigilshell_application_contracts::ports::*;
use vigilshell_domain::workspace::*;
use vigilshell_transport_russh::RusshTransport;
static NATIVE_TAR_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn io(local: PathBuf, remote: &str, upload: bool, cancel: bool) -> TransferIo {
    TransferIo {
        local,
        remote: remote.into(),
        upload,
        overwrite: false,
        cancel: Arc::new(AtomicBool::new(cancel)),
        progress: Arc::new(AtomicU64::new(0)),
        total: Arc::new(AtomicU64::new(u64::MAX)),
    }
}
#[tokio::test]
#[ignore = "requires explicitly selected disposable loopback SSH target"]
async fn directory_and_archive_transfer_real_ssh() {
    let _lock = NATIVE_TAR_LOCK.lock().await;
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        name: "native transfer fixture".into(),
        auth: AuthKind::Password,
        environment: Environment::Test,
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
    };
    let key = RusshTransport.probe(input.clone()).await.unwrap();
    let tools = RusshTransport
        .tools(
            input,
            key,
            SecretMaterial {
                diagnostic: None,
                kind: AuthKind::Password,
                bytes: b"native-fixture-only".to_vec().into(),
                label: String::new(),
            },
            None,
        )
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().to_path_buf();
    let source = local.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(source.join("empty")).unwrap();
    std::fs::write(source.join("文件 --name\n.txt"), b"fixture bytes\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("文件 --name\n.txt", source.join("link")).unwrap();
    let remote = format!("/tmp/vigil-transfer-{}", new_id());
    let state = Arc::new(Mutex::new(TransferTreeState::default()));
    tools
        .transfer_tree(TransferTreeIo {
            io: io(source.clone(), &remote, true, false),
            state: state.clone(),
            batch_policy: Arc::new(Mutex::new(None)),
        })
        .await
        .unwrap();
    assert_eq!(
        state
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|e| e.phase == "complete")
            .count(),
        3
    );
    #[cfg(unix)]
    assert_eq!(
        state
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|e| e.phase == "skipped")
            .count(),
        1
    );
    let container = std::env::var("VIGILSHELL_TEST_NATIVE_CONTAINER").ok();
    if let Some(container) = &container {
        let label = std::process::Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{index .Config.Labels \"vigilshell.acceptance\"}}",
                container,
            ])
            .output()
            .unwrap();
        assert!(label.status.success());
        assert_eq!(String::from_utf8(label.stdout).unwrap().trim(), "file-menu");
        let linked = std::process::Command::new("docker")
            .args([
                "exec",
                container,
                "ln",
                "-s",
                "/etc/passwd",
                &format!("{remote}/remote-link"),
            ])
            .status()
            .unwrap();
        assert!(linked.success());
    }
    let destination = local.join("download");
    tools
        .transfer_tree(TransferTreeIo {
            io: io(destination.clone(), &remote, false, false),
            state: Arc::new(Mutex::new(TransferTreeState::default())),
            batch_policy: Arc::new(Mutex::new(None)),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(destination.join("文件 --name\n.txt")).unwrap(),
        b"fixture bytes\n"
    );
    assert!(destination.join("empty").is_dir());
    assert!(!destination.join("remote-link").exists());
    let conflict = Arc::new(Mutex::new(TransferTreeState::default()));
    let download = || TransferTreeIo {
        io: io(destination.clone(), &remote, false, false),
        state: conflict.clone(),
        batch_policy: Arc::new(Mutex::new(None)),
    };
    assert_eq!(
        tools.transfer_tree(download()).await,
        Err(WorkspaceError::FilesExists)
    );
    conflict.lock().unwrap().conflict_decision = Some(false);
    tools.transfer_tree(download()).await.unwrap();
    assert_eq!(
        std::fs::read(destination.join("文件 --name\n.txt")).unwrap(),
        b"fixture bytes\n"
    );
    let cancelled = local.join("cancelled");
    assert_eq!(
        tools
            .transfer_tree(TransferTreeIo {
                io: io(cancelled.clone(), &remote, false, true),
                state: Arc::new(Mutex::new(TransferTreeState::default())),
                batch_policy: Arc::new(Mutex::new(None))
            })
            .await,
        Err(WorkspaceError::Cancelled)
    );
    assert!(!cancelled.exists());
    let archive_path = local.join("download.tar.gz");
    tools
        .archive(ArchiveIo {
            io: io(archive_path.clone(), &remote, false, false),
            paths: vec![remote.clone()],
            state: Arc::new(Mutex::new(TransferTreeState::default())),
        })
        .await
        .unwrap();
    let listing = std::process::Command::new("tar")
        .args(["-tzf"])
        .arg(&archive_path)
        .output()
        .unwrap();
    assert!(listing.status.success());
    let text = String::from_utf8(listing.stdout).unwrap();
    assert!(text.contains("empty/"));
    assert!(!text.contains("/link"));
    assert!(!text.contains("remote-link"));
    let member = format!("./{}/文件 --name\n.txt", remote.rsplit('/').next().unwrap());
    let content = std::process::Command::new("tar")
        .args(["-xOzf"])
        .arg(&archive_path)
        .arg(member)
        .output()
        .unwrap();
    assert!(content.status.success());
    assert_eq!(content.stdout, b"fixture bytes\n");
    assert!(
        !std::fs::read_dir(&local).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
    tools
        .remove(format!("{remote}/文件 --name\n.txt"), false)
        .await
        .unwrap();
    tools.remove(format!("{remote}/empty"), true).await.unwrap();
    if container.is_some() {
        tools
            .remove(format!("{remote}/remote-link"), false)
            .await
            .unwrap();
    }
    tools.remove(remote, true).await.unwrap();
    tools.close().await.unwrap();
}

struct TarOverride {
    container: String,
    marker: String,
}
impl TarOverride {
    fn new() -> Self {
        let container = std::env::var("VIGILSHELL_TEST_NATIVE_CONTAINER")
            .expect("disposable labelled container");
        let inspected = std::process::Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.Id}} {{index .Config.Labels \"vigilshell.acceptance\"}}",
                &container,
            ])
            .output()
            .unwrap();
        assert!(inspected.status.success());
        let inspected = String::from_utf8(inspected.stdout).unwrap();
        let fields: Vec<_> = inspected.split_whitespace().collect();
        assert_eq!(fields.get(1), Some(&"file-menu"));
        let value = Self {
            container: fields[0].into(),
            marker: format!("# vigil-archive-fault-{}", new_id()),
        };
        let status = std::process::Command::new("docker").args(["exec", &value.container, "sh", "-c", "test ! -e /usr/local/bin/tar && test ! -L /usr/local/bin/tar && (set -C; printf '%s\\n' '#!/bin/sh' \"$1\" 'exit 127' > /usr/local/bin/tar) && chmod 700 /usr/local/bin/tar", "fixture", &value.marker]).status().unwrap();
        assert!(
            status.success(),
            "refuse to replace a pre-existing tar override"
        );
        value
    }
    fn script(&self, body: &str) {
        use std::io::Write;
        let mut child = std::process::Command::new("docker")
            .args([
                "exec",
                "-i",
                &self.container,
                "sh",
                "-c",
                "grep -F -x -- \"$1\" /usr/local/bin/tar >/dev/null && cat > /usr/local/bin/tar",
                "fixture",
                &self.marker,
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("#!/bin/sh\n{}\n{body}\n", self.marker).as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }
}
impl Drop for TarOverride {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker").args(["exec", &self.container, "sh", "-c", "if grep -F -x -- \"$1\" /usr/local/bin/tar >/dev/null 2>&1; then rm -- /usr/local/bin/tar; fi", "fixture", &self.marker]).status();
    }
}

#[tokio::test]
#[ignore = "temporarily overrides tar only inside the explicitly selected disposable container"]
async fn archive_failures_do_not_publish_or_leave_parts() {
    let _lock = NATIVE_TAR_LOCK.lock().await;
    let port = std::env::var("VIGILSHELL_TEST_NATIVE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let input = ConnectionInput {
        host: "127.0.0.1".into(),
        port,
        username: "root".into(),
        name: "archive failure fixture".into(),
        auth: AuthKind::Password,
        environment: Environment::Test,
        notes: String::new(),
        terminal: Box::default(),
        proxy_id: None,
        tunnels: vec![],
    };
    let key = RusshTransport.probe(input.clone()).await.unwrap();
    let tools = RusshTransport
        .tools(
            input,
            key,
            SecretMaterial {
                diagnostic: None,
                kind: AuthKind::Password,
                bytes: b"native-fixture-only".to_vec().into(),
                label: String::new(),
            },
            None,
        )
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"archive fault fixture\n").unwrap();
    let remote = format!("/tmp/vigil-archive-fault-{}", new_id());
    tools.mkdir(remote.clone()).await.unwrap();
    let target = format!("{remote}/source");
    tools
        .transfer(io(source, &target, true, false))
        .await
        .unwrap();
    let wrapper = TarOverride::new();
    let expected = [
        WorkspaceError::FilesUnsupported,
        WorkspaceError::ToolsUnavailable,
        WorkspaceError::Conflict,
    ];
    for (case, error) in expected.into_iter().enumerate() {
        let body = match case {
            0 => "exit 127".to_string(),
            1 => "if [ \"$1\" = --version ]; then printf 'tar (GNU tar) fixture\\n'; exit 0; fi\nprintf 'partial archive output'; exit 9".to_string(),
            _ => format!("if [ \"$1\" = --version ]; then /usr/bin/tar --version; exit $?; fi\n/usr/bin/tar \"$@\"\nstatus=$?\nprintf 'changed after archive' >> {}\nexit \"$status\"", vigilshell_domain::agent::shell_quote(&target)),
        };
        wrapper.script(&body);
        let destination = root.path().join(format!("case-{case}.tar.gz"));
        assert_eq!(
            tools
                .archive(ArchiveIo {
                    io: io(destination.clone(), &remote, false, false),
                    paths: vec![remote.clone()],
                    state: Arc::new(Mutex::new(TransferTreeState::default()))
                })
                .await,
            Err(error)
        );
        assert!(
            !destination.exists(),
            "failed archive must not be published"
        );
        assert!(
            !std::fs::read_dir(root.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".part")),
            "failed archive must remove its staging file"
        );
    }
    let container = wrapper.container.clone();
    drop(wrapper);
    let restored = std::process::Command::new("docker")
        .args([
            "exec",
            &container,
            "sh",
            "-c",
            "test ! -e /usr/local/bin/tar && tar --version",
        ])
        .output()
        .unwrap();
    assert!(restored.status.success());
    assert!(
        String::from_utf8(restored.stdout)
            .unwrap()
            .contains("GNU tar")
    );
    tools.remove(target, false).await.unwrap();
    tools.remove(remote, true).await.unwrap();
    tools.close().await.unwrap();
}
