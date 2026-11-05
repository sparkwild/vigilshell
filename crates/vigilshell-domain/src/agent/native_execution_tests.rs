use super::*;
fn policy(level: OperationLevel) -> ConnectionPolicy {
    ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 1,
        revision: 1,
        verify_connect: true,
        execution: ExecutionPolicy {
            level,
            script_review: true,
            ..Default::default()
        },
    }
}
fn request(operation: RuntimeOperation) -> ExecutionRequest {
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
        request_key: "request".into(),
        timeout_seconds: None,
        operation,
    }
}
#[test]
fn native_permission_is_not_script_review_or_arbitrary_file_access() {
    let install = request(RuntimeOperation::PackagesInstall {
        packages: vec![PackageTarget::Repository {
            name: "example".into(),
            version: None,
        }],
    });
    assert!(check_native_operation(&install, &policy(OperationLevel::Disabled)).is_err());
    assert!(check_native_operation(&install, &policy(OperationLevel::ReadOnly)).is_err());
    let permit = check_native_operation(&install, &policy(OperationLevel::Operate)).unwrap();
    assert_eq!(
        permit
            .backup_root("/home/native-staging/alice/.local/share/vigilshell/native-staging/task")
            .unwrap(),
        "/home/native-staging/alice/.local/share/vigilshell/source-backups/task"
    );
    let query = request(RuntimeOperation::PackagesList {});
    assert!(check_native_operation(&query, &policy(OperationLevel::ReadOnly)).is_ok());
    let file = RuntimeOperation::FilesRemove {
        target: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "etc/passwd".into(),
        },
        recursive: false,
    };
    assert!(
        native_file_operation(
            &permit,
            file,
            "/root/.local/share/vigilshell/native-staging/task"
        )
        .is_err()
    );
    let source = check_native_operation(
        &request(RuntimeOperation::PackageSourcesList {}),
        &policy(OperationLevel::ReadOnly),
    )
    .unwrap();
    let root = format!(
        "/root/.local/share/vigilshell/native-staging/{}",
        source.id()
    );
    let write = RuntimeOperation::FilesWrite {
        target: ScopedPath {
            scope_id: ACCOUNT_SCOPE_ID.into(),
            path: "etc/apt/sources.list".into(),
        },
        data: "YQ==".into(),
        mode: FileWriteMode::Create,
        expected_hash: None,
    };
    assert!(native_file_operation(&source, write, &root).is_err());
}
#[test]
fn source_formats_keep_identity_and_reject_lossy_updates() {
    use std::path::Path;
    let rpm = "[a]\nname=a\nbaseurl=https://example.invalid/\nenabled=0\ngpgcheck=1\n";
    assert!(package_sources::validate_document(Path::new("/etc/yum.repos.d/a.repo"), rpm).is_ok());
    for text in [
        "garbage",
        "[a]\nenabled=1\nenabled=0",
        "[a]\nbroken",
        "[a]\nenabled=maybe",
    ] {
        assert!(package_sources::validate_rpm(text).is_err(), "{text}");
    }
    let path = Path::new("/etc/apt/sources.list.d/vigilshell-managed-test.sources");
    let text = "Types: deb\nURIs: https://example.invalid\nSuites: stable\nComponents: main\nEnabled: no\n";
    let toggled = package_sources::toggle(path, text, true).unwrap();
    assert_eq!(
        package_sources::identity(path, text, &[]).unwrap(),
        package_sources::identity(path, &toggled, &[]).unwrap()
    );
    assert_ne!(
        package_sources::identity(path, text, &[("key".into(), "old".into())]).unwrap(),
        package_sources::identity(path, text, &[("key".into(), "new".into())]).unwrap()
    );
    assert!(!package_sources::key_path("/etc/apt/trusted.gpg-other"));
}
#[test]
fn service_and_system_actions_obey_levels_and_scoped_definition_access() {
    let queries = vec![
        RuntimeOperation::ServicesList {},
        RuntimeOperation::ServicesStatus {
            name: "example.service".into(),
        },
        RuntimeOperation::ServicesLogs {
            name: "user:example.service".into(),
            lines: 10,
        },
        RuntimeOperation::ServicesDefinition {
            name: "example.service".into(),
        },
    ];
    for query in queries {
        let request = request(query);
        assert!(check_native_operation(&request, &policy(OperationLevel::ReadOnly)).is_ok());
        assert!(check_native_operation(&request, &policy(OperationLevel::Disabled)).is_err());
    }
    let definition:NativeServiceDefinition=serde_json::from_value(serde_json::json!({"target":{"manager":"system","name":"example.service"},"content":{"kind":"dropIn","name":"override.conf","text":"[Service]\nEnvironment=X=one\n"},"expectedHash":null})).unwrap();
    let definition_request = request(RuntimeOperation::ServicesRegister { definition });
    assert!(
        check_native_operation(&definition_request, &policy(OperationLevel::ReadOnly)).is_err()
    );
    let permit =
        check_native_operation(&definition_request, &policy(OperationLevel::Operate)).unwrap();
    let root = "/root/.local/share/vigilshell/native-staging/task";
    let file = |path: &str| ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: path.into(),
    };
    assert!(
        native_file_operation(
            &permit,
            RuntimeOperation::FilesRead {
                target: file("etc/systemd/system/example.service"),
                offset: 0,
                length: 100
            },
            root
        )
        .is_ok()
    );
    for path in [
        "etc/systemd/system/example.service",
        "usr/lib/systemd/system/example.service",
        "etc/systemd/system/another.service.d/override.conf",
        "etc/passwd",
    ] {
        assert!(
            native_file_operation(
                &permit,
                RuntimeOperation::FilesWrite {
                    target: file(path),
                    data: "YQ==".into(),
                    mode: FileWriteMode::Create,
                    expected_hash: None
                },
                root
            )
            .is_err(),
            "{path}"
        );
    }
    for op in [
        RuntimeOperation::PowerControl {
            action: PowerAction::Reboot,
        },
        RuntimeOperation::SystemSettingsUpdate {
            setting: SystemSetting::Hostname,
            value: "example".into(),
        },
    ] {
        let req = request(op);
        assert!(check_native_operation(&req, &policy(OperationLevel::ReadOnly)).is_err());
        assert!(check_native_operation(&req, &policy(OperationLevel::Operate)).is_ok());
    }
}
