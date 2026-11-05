use super::*;
fn request(command: &str) -> ExecutionRequest {
    ExecutionRequest {
        identity: ExecutionIdentity {
            task_id: "task".into(),
            entry_id: "entry".into(),
            entry_generation: 1,
            connection_id: "connection".into(),
            connection_revision: 1,
            session_id: "session".into(),
            policy_revision: 1,
        },
        request_key: "key".into(),
        timeout_seconds: None,
        operation: RuntimeOperation::Command {
            command: command.into(),
            directory: ScopedPath {
                scope_id: ACCOUNT_SCOPE_ID.into(),
                path: "srv/app".into(),
            },
        },
    }
}
fn policy(level: OperationLevel) -> ConnectionPolicy {
    ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: true,
        execution: ExecutionPolicy {
            level,
            ..Default::default()
        },
    }
}
#[test]
fn readonly_literals_rebuild_and_options_are_bounded() {
    for command in [
        "pwd",
        "ls -lah -- '/tmp/a b'",
        "stat -L /tmp/file",
        "cat -- 'a; rm x'",
        "head -n 20 /tmp/file",
        "tail -c 100 /tmp/file",
        "wc -cl /tmp/file",
        "uname -a",
        "id -un",
    ] {
        let checked = check_command(&request(command), &policy(OperationLevel::ReadOnly))
            .unwrap_or_else(|e| panic!("{command}: {e}"));
        assert!(checked.shell_source().starts_with("if test -x /usr/bin/"));
        assert_eq!(checked.directory(), "/srv/app");
    }
    for command in [
        "sh",
        "rm a",
        "ls >a",
        "ls; id",
        "ls && id",
        "ls | wc",
        "ls $(id)",
        "ls `id`",
        "ls $HOME",
        "ls *",
        "ls ~/",
        "/bin/ls",
        "cat",
        "cat -",
        "head -n nope a",
        "tail -f a",
        "stat --printf=%s a",
        "ls --unknown",
        "id a b",
        "pwd /tmp",
        "ls '",
        "ls\nrm a",
    ] {
        assert!(
            check_command(&request(command), &policy(OperationLevel::ReadOnly)).is_err(),
            "{command}"
        );
    }
}
#[test]
fn destructive_targets_checked_across_entire_request() {
    for command in [
        "cd /; rm -rf etc",
        "export PATH=/tmp; rm file",
        "chmod 999 /srv/app/file",
        "rm --parents /srv/app/file",
        "rmdir -rf /srv/app/file",
        "rm -rf /",
        "rm -rf /etc",
        "rm -rf /srv",
        "rm -r .",
        "rm",
        "rm --froce /srv/app/file",
        "rm -rf '/srv/app/../../'",
        "rm -rf '$TARGET'",
        "chmod -R 777 /etc",
        "chown root /etc/passwd",
        "printf ok; rm -rf /",
        "echo x > /etc/ssh/sshd_config",
        "tee /etc/sudoers",
        "dd of=/dev/sda",
        "find / -delete",
        "sed -i x /etc/passwd",
        "sh -c 'rm -rf /'",
        "python3.12 /srv/app/script.py",
        "sudo rm x",
        "./script.sh",
        "mkfs.ext4 /dev/sda",
        "cp --target-directory=/etc /srv/app/a /srv/app/b",
    ] {
        assert!(
            check_command(&request(command), &policy(OperationLevel::Operate)).is_err(),
            "{command}"
        );
    }
    for command in [
        "rm -rf /srv/app/cache",
        "chmod -R 755 /srv/app/cache",
        "cp /srv/app/a /srv/app/b",
        "cp /etc/resolv.conf /srv/app/resolv.conf",
        "printf '%s' '中文'",
        "printf x > /srv/app/log",
        "ls /srv/app && wc -l /srv/app/log",
        "printf x | wc -c",
        "curl https://example.com",
        "python3 --version",
    ] {
        assert!(
            check_command(&request(command), &policy(OperationLevel::Operate)).is_ok(),
            "{command}"
        );
    }
}
#[test]
fn toggles_do_not_disable_syntax_root_or_extra_protection() {
    let mut p = policy(OperationLevel::Operate);
    p.execution.protect_system = false;
    assert!(check_command(&request("rm /etc/example"), &p).is_ok());
    for command in ["rm /", "rm", "echo $bad", "echo x &&"] {
        assert!(check_command(&request(command), &p).is_err());
    }
    p.execution.protected_paths.push(ProtectedPath {
        path: "/srv/app/private".into(),
        kind: ScopeKind::Directory,
    });
    assert!(check_command(&request("rm -rf /srv/app/private"), &p).is_err());
    assert!(check_command(&request("rm -rf /srv/app"), &p).is_err());
    assert!(check_command(&request("ls"), &policy(OperationLevel::Disabled)).is_err());
}
