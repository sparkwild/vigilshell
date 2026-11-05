use super::*;
use serde_json::json;

fn path(value: &str) -> ScopedPath {
    ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: value.into(),
    }
}
fn policy(level: OperationLevel) -> ExecutionPolicy {
    ExecutionPolicy {
        level,
        ..ExecutionPolicy::default()
    }
}
fn script() -> RuntimeOperation {
    RuntimeOperation::Script {
        source: ScriptSource::Inline {
            text: "echo fixture".into(),
        },
        interpreter: ScriptInterpreter::Sh,
        arguments: vec![],
        directory: path("srv"),
    }
}

#[test]
fn new_policy_decodes_missing_script_review_as_false_and_rejects_old_policy() {
    let mut value = serde_json::to_value(ExecutionPolicy::default()).unwrap();
    assert_eq!(value["scriptReview"], false);
    value.as_object_mut().unwrap().remove("scriptReview");
    let decoded: ExecutionPolicy = serde_json::from_value(value.clone()).unwrap();
    assert!(!decoded.script_review);
    assert_eq!(decoded.level, OperationLevel::Disabled);
    value["commandNetwork"] = json!(false);
    assert!(serde_json::from_value::<ExecutionPolicy>(value.clone()).is_err());
    value.as_object_mut().unwrap().remove("commandNetwork");
    value["version"] = json!(1);
    assert!(serde_json::from_value::<ExecutionPolicy>(value).is_err());
}

#[test]
fn commands_have_explicit_pending_checks_and_never_request_human_review() {
    let command = RuntimeOperation::Command {
        command: "rm -rf /".into(),
        directory: path("srv"),
    };
    assert!(
        policy(OperationLevel::Disabled)
            .authorize(&command)
            .is_err()
    );
    for (level, check) in [
        (
            OperationLevel::ReadOnly,
            CommandCheckRequirement::ReadOnlyAllowlist,
        ),
        (OperationLevel::Operate, CommandCheckRequirement::HighRisk),
    ] {
        let mut current = policy(level);
        current.script_review = true;
        let decision = current.authorize(&command).unwrap();
        assert_eq!(decision.command_check, check);
        assert!(!decision.script_review_required);
    }
    let mut unprotected = policy(OperationLevel::Operate);
    unprotected.protect_system = false;
    assert_eq!(
        unprotected.authorize(&command).unwrap().command_check,
        CommandCheckRequirement::HighRisk
    );
}

#[test]
fn in_memory_operations_cannot_bypass_business_validation() {
    let current = policy(OperationLevel::Operate);
    assert!(matches!(
        current.authorize(&RuntimeOperation::Command {
            command: "\0".into(),
            directory: path("srv"),
        }),
        Err("invalid_command")
    ));
    assert!(matches!(
        current.authorize(&RuntimeOperation::FilesRead {
            target: path("srv/file"),
            offset: crate::MAX_REVISION + 1,
            length: 10,
        }),
        Err("invalid_execution_revision")
    ));
    assert!(
        current
            .authorize(&RuntimeOperation::PackagesInstall {
                packages: vec![PackageTarget::Repository {
                    name: "--arbitrary-option".into(),
                    version: None
                }],
            })
            .is_err()
    );
}

#[test]
fn readonly_denies_scripts_pty_and_writes_but_allows_explicit_reads() {
    let current = policy(OperationLevel::ReadOnly);
    for operation in [
        script(),
        RuntimeOperation::Terminal {
            directory: path("srv"),
            columns: 80,
            rows: 24,
        },
        RuntimeOperation::FilesRemove {
            target: path("srv/app"),
            recursive: false,
        },
        RuntimeOperation::PackagesInstall {
            packages: vec![PackageTarget::Repository {
                name: "curl".into(),
                version: None,
            }],
        },
    ] {
        assert!(matches!(
            current.authorize(&operation),
            Err("read_only_operation_denied")
        ));
    }
    assert!(
        current
            .authorize(&RuntimeOperation::FilesRead {
                target: path("srv/file"),
                offset: 0,
                length: 100
            })
            .is_ok()
    );
}

#[test]
fn review_is_only_for_independent_script_requests() {
    let mut current = policy(OperationLevel::Operate);
    assert!(!current.authorize(&script()).unwrap().script_review_required);
    current.script_review = true;
    assert!(current.authorize(&script()).unwrap().script_review_required);
    for operation in [
        RuntimeOperation::PackagesInstall {
            packages: vec![PackageTarget::Repository {
                name: "curl".into(),
                version: None,
            }],
        },
        RuntimeOperation::PackagesUpdate {
            packages: vec![PackageTarget::Repository {
                name: "curl".into(),
                version: None,
            }],
        },
        RuntimeOperation::PackagesRemove {
            packages: vec![PackageTarget::Repository {
                name: "curl".into(),
                version: None,
            }],
        },
    ] {
        assert!(
            !current
                .authorize(&operation)
                .unwrap()
                .script_review_required
        );
    }
}

#[test]
fn transfer_requires_separate_permission_and_local_directory_handle() {
    let mut current = policy(OperationLevel::ReadOnly);
    current.download_roots.push(LocalDirectoryGrant {
        id: "download".into(),
        path: "/tmp/download".into(),
    });
    let download = RuntimeOperation::TransfersDownload {
        remote: path("srv/file"),
        local: ScopedPath {
            scope_id: "download".into(),
            path: "file".into(),
        },
        overwrite: false,
    };
    assert!(matches!(
        current.authorize(&download),
        Err("transfer_denied")
    ));
    current.file_transfer = true;
    assert!(current.authorize(&download).is_ok());
    let upload = RuntimeOperation::TransfersUpload {
        remote: path("srv/file"),
        local: ScopedPath {
            scope_id: "download".into(),
            path: "file".into(),
        },
        overwrite: false,
    };
    assert!(current.authorize(&upload).is_err());
    let outside = RuntimeOperation::TransfersDownload {
        remote: path("srv/file"),
        local: ScopedPath {
            scope_id: "download".into(),
            path: "../file".into(),
        },
        overwrite: false,
    };
    assert!(current.authorize(&outside).is_err());
}

#[test]
fn explicit_dangerous_targets_are_checked_without_recursive_flag_overblocking() {
    let mut current = policy(OperationLevel::Operate);
    for target in ["", "etc/ssh", "usr/bin"] {
        assert!(
            current
                .authorize(&RuntimeOperation::FilesRemove {
                    target: path(target),
                    recursive: true
                })
                .is_err()
        );
    }
    assert!(
        current
            .authorize(&RuntimeOperation::FilesRemove {
                target: path("srv/app/cache"),
                recursive: true
            })
            .is_ok()
    );
    current.protected_paths.push(ProtectedPath {
        path: "/srv/app/data".into(),
        kind: ScopeKind::Directory,
    });
    assert!(
        current
            .authorize(&RuntimeOperation::FilesRemove {
                target: path("srv/app"),
                recursive: true
            })
            .is_err()
    );
    current.protect_system = false;
    assert!(
        current
            .authorize(&RuntimeOperation::FilesRemove {
                target: path(""),
                recursive: true
            })
            .is_err()
    );
}

#[test]
fn policy_decode_rejects_invalid_limits_and_grants() {
    let base = serde_json::to_value(ExecutionPolicy::default()).unwrap();
    for roots in [
        json!([{"id":"local","path":"/"}]),
        json!([{"id":"local","path":"/tmp/../outside"}]),
        json!([{"id":"__account__","path":"/tmp/download"}]),
        json!([{"id":"local","path":"/tmp/a"},{"id":"local","path":"/tmp/b"}]),
    ] {
        let mut value = base.clone();
        value["downloadRoots"] = roots;
        assert!(serde_json::from_value::<ExecutionPolicy>(value).is_err());
    }
    let mut invalid_limit = base;
    invalid_limit["limits"]["concurrentTasks"] = json!(0);
    assert!(serde_json::from_value::<ExecutionPolicy>(invalid_limit).is_err());
}

#[test]
fn protected_file_requires_a_filename_without_requiring_an_extension() {
    for (kind, path, valid) in [
        (ScopeKind::Directory, "/", true),
        (ScopeKind::File, "/", false),
        (ScopeKind::File, "/etc/hosts", true),
        (ScopeKind::File, "/etc/", false),
        (ScopeKind::Directory, "/srv/data.v1", true),
    ] {
        let mut p = policy(OperationLevel::ReadOnly);
        p.protected_paths = vec![ProtectedPath {
            kind,
            path: path.into(),
        }];
        assert_eq!(p.validate().is_ok(), valid, "{path}");
    }
}

#[test]
fn local_directory_setting_is_shared_and_unrestricted_mode_keeps_transfer_permissions() {
    let mut original = ConnectionPolicy {
        connection_id: "fixture".into(),
        ..Default::default()
    };
    original.execution.level = OperationLevel::Operate;
    original.execution.file_transfer = true;
    let download = RuntimeOperation::TransfersDownload {
        remote: path("tmp/file"),
        local: ScopedPath {
            scope_id: "local".into(),
            path: "tmp/elsewhere/file".into(),
        },
        overwrite: false,
    };
    assert!(original.execution.authorize(&download).is_ok());
    let limited = PolicySettingsPatch::LocalDirectory(Some("/tmp/shared".into()))
        .apply(&original)
        .unwrap();
    assert_eq!(
        limited.execution.upload_roots,
        limited.execution.download_roots
    );
    assert!(!limited.execution.permits_existing(&original.execution));
    assert!(
        PolicySettingsPatch::LocalDirectory(Some(String::new()))
            .apply(&original)
            .is_err()
    );
    assert!(
        PolicySettingsPatch::LocalDirectory(Some("/".into()))
            .apply(&original)
            .is_err()
    );
    let unlocked = PolicySettingsPatch::LocalDirectory(None)
        .apply(&limited)
        .unwrap();
    assert!(unlocked.execution.permits_existing(&limited.execution));
    let mut denied = unlocked.execution;
    denied.file_transfer = false;
    assert!(denied.authorize(&download).is_err());
    denied.file_transfer = true;
    denied.level = OperationLevel::ReadOnly;
    assert!(denied.authorize(&download).is_ok());
    let upload = RuntimeOperation::TransfersUpload {
        remote: path("tmp/file"),
        local: ScopedPath {
            scope_id: "local".into(),
            path: "tmp/file".into(),
        },
        overwrite: false,
    };
    assert!(denied.authorize(&upload).is_err());
}
