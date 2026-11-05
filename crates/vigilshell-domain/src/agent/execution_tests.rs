use super::*;
use serde_json::{Value, json};

fn request() -> Value {
    json!({
        "identity": { "taskId": "task", "entryId": "entry", "entryGeneration": 1,
            "connectionId": "connection", "connectionRevision": 2, "sessionId": "session", "policyRevision": 3 },
        "requestKey": "request", "timeoutSeconds": null,
        "operation": { "kind": "command", "command": "pwd", "directory": { "scopeId": "__account__", "path": "srv" } }
    })
}

#[test]
fn current_execution_request_decodes_without_runtime_credentials() {
    let value: ExecutionRequest = serde_json::from_value(request()).unwrap();
    assert_eq!(value.validate(), Ok(()));
    assert_eq!(serde_json::to_value(&value).unwrap(), request());
    for (field, injected) in [
        ("protocol", json!(9)),
        ("executionRoute", json!("protected")),
        ("executionReceipt", json!({"executorPid": 1})),
        ("approved", json!(true)),
        ("policy", json!({"model": "simplified", "policy": {}})),
        ("uid", json!(0)),
    ] {
        let mut value = request();
        value[field] = injected;
        assert!(
            serde_json::from_value::<ExecutionRequest>(value).is_err(),
            "{field}"
        );
    }
}

#[test]
fn identities_and_safe_integers_reject_during_decode_and_validate() {
    for field in ["taskId", "entryId", "connectionId", "sessionId"] {
        for bad in ["", "entry/name", "entry\nname", "用户"] {
            let mut value = request();
            value["identity"][field] = json!(bad);
            assert!(
                serde_json::from_value::<ExecutionRequest>(value).is_err(),
                "{field}: {bad}"
            );
        }
    }
    for field in ["entryGeneration", "connectionRevision", "policyRevision"] {
        let mut value = request();
        value["identity"][field] = json!(crate::MAX_REVISION + 1);
        assert!(
            serde_json::from_value::<ExecutionRequest>(value).is_err(),
            "{field}"
        );
    }
    let mut value: ExecutionRequest = serde_json::from_value(request()).unwrap();
    value.identity.entry_generation = crate::MAX_REVISION + 1;
    assert_eq!(value.validate(), Err("invalid_execution_revision"));
    let mut unknown = request();
    unknown["identity"]["executorPid"] = json!(1);
    assert!(serde_json::from_value::<ExecutionRequest>(unknown).is_err());
    let mut zero_generation = request();
    zero_generation["identity"]["entryGeneration"] = json!(0);
    assert!(serde_json::from_value::<ExecutionRequest>(zero_generation).is_err());
    value.identity.entry_generation = 0;
    assert_eq!(value.validate(), Err("invalid_execution_generation"));
    value.identity.entry_generation = 1;
    value.identity.connection_revision = 0;
    value.identity.policy_revision = 0;
    assert_eq!(value.validate(), Ok(()));
}

#[test]
fn request_keys_preserve_utf8_and_punctuation_with_the_existing_byte_budget() {
    for key in [
        "request:key.中文".to_owned(),
        " ".to_owned(),
        "中".repeat(42) + "ab",
        "a".repeat(128),
    ] {
        let mut wire = request();
        wire["requestKey"] = json!(key);
        let decoded: ExecutionRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded.request_key, key);
        assert_eq!(decoded.validate(), Ok(()));
    }
    for key in [
        String::new(),
        "中".repeat(43),
        "a".repeat(129),
        "request\nkey".to_owned(),
        "request\u{85}key".to_owned(),
    ] {
        let mut wire = request();
        wire["requestKey"] = json!(key);
        assert!(serde_json::from_value::<ExecutionRequest>(wire).is_err());
        let mut in_memory: ExecutionRequest = serde_json::from_value(request()).unwrap();
        in_memory.request_key = key;
        assert_eq!(in_memory.validate(), Err("invalid_request_key"));
    }
}

#[test]
fn requests_validate_business_parameters_without_running_a_command() {
    for operation in [
        json!({"kind":"command","command":"","directory":{"scopeId":"__account__","path":"srv"}}),
        json!({"kind":"terminal","directory":{"scopeId":"__account__","path":"srv"},"columns":0,"rows":24}),
        json!({"kind":"files_read","target":{"scopeId":"__account__","path":"srv"},"offset":0,"length":0}),
        json!({"kind":"command","command":"pwd","directory":{"scopeId":"root","path":"srv"}}),
        json!({"kind":"command","command":"pwd","directory":{"scopeId":"__account__","path":"../etc"}}),
        json!({"kind":"packages_install","packages":[{"kind":"repository","name":"curl","version":null,"uid":0}]}),
        json!({"kind":"services_control","name":"example.service","action":"status"}),
    ] {
        let mut value = request();
        value["operation"] = operation;
        assert!(serde_json::from_value::<ExecutionRequest>(value).is_err());
    }
    let mut omitted_timeout = request();
    omitted_timeout
        .as_object_mut()
        .unwrap()
        .remove("timeoutSeconds");
    assert_eq!(
        serde_json::from_value::<ExecutionRequest>(omitted_timeout)
            .unwrap()
            .timeout_seconds,
        None
    );
    let mut zero_timeout = request();
    zero_timeout["timeoutSeconds"] = json!(0);
    assert!(serde_json::from_value::<ExecutionRequest>(zero_timeout).is_err());
}

#[test]
fn authorization_binds_connection_and_policy_revision() {
    let value: ExecutionRequest = serde_json::from_value(request()).unwrap();
    let mut policy = ConnectionPolicy {
        connection_id: "connection".into(),
        connection_revision: 2,
        revision: 3,
        verify_connect: true,
        execution: ExecutionPolicy {
            level: OperationLevel::ReadOnly,
            ..ExecutionPolicy::default()
        },
    };
    assert_eq!(
        value.authorize(&policy).unwrap().command_check,
        CommandCheckRequirement::ReadOnlyAllowlist
    );
    policy.revision += 1;
    assert!(matches!(
        value.authorize(&policy),
        Err("execution_policy_mismatch")
    ));
}

#[test]
fn result_confirmation_keeps_acceptance_and_unknown_separate() {
    assert!(!ExecutionOutcome::RequestAccepted {}.termination_confirmed());
    assert!(
        !ExecutionOutcome::Unknown {
            reason: ExecutionStopReason::Disconnected
        }
        .termination_confirmed()
    );
    assert!(
        ExecutionOutcome::Stopped {
            reason: ExecutionStopReason::Cancelled
        }
        .termination_confirmed()
    );
    assert!(
        serde_json::from_value::<ExecutionOutcome>(
            json!({"kind":"completed","exitCode":0,"executorPid":12})
        )
        .is_err()
    );
}

#[test]
fn shared_schema_cases_match_strict_rust_decoding() {
    fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<(), String> {
        serde_json::from_value::<T>(value)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/local-execution-contract.json"
    ))
    .unwrap();
    let mut failures = vec![];
    for case in fixture["schemaCases"].as_array().unwrap() {
        let value = case["value"].clone();
        let actual = match case["type"].as_str().unwrap() {
            "ConnectionPolicy" => decode::<ConnectionPolicy>(value),
            "ExecutionRequest" => decode::<ExecutionRequest>(value),
            "ScriptReviewBinding" => decode::<ScriptReviewBinding>(value),
            "ExecutionState" => decode::<ExecutionState>(value),
            "ExecutionOutcome" => decode::<ExecutionOutcome>(value),
            other => panic!("unknown contract fixture type {other}"),
        };
        if actual.is_ok() != case["valid"].as_bool().unwrap() {
            failures.push(format!(
                "{}: expected {}, decoded {actual:?}",
                case["name"], case["valid"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn shared_authorization_cases_keep_pending_checks_explicit() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/local-execution-contract.json"
    ))
    .unwrap();
    let mut failures = vec![];
    for case in fixture["authorizationCases"].as_array().unwrap() {
        let actual = (|| -> Result<Value, String> {
            let request: ExecutionRequest = serde_json::from_value(case["request"].clone())
                .map_err(|error| error.to_string())?;
            let policy: ConnectionPolicy = serde_json::from_value(case["policy"].clone())
                .map_err(|error| error.to_string())?;
            let result = request.authorize(&policy).map_err(str::to_owned)?;
            Ok(
                json!({ "commandCheck": result.command_check, "scriptReviewRequired": result.script_review_required,
                "targetCheckRequired": result.target_check_required }),
            )
        })();
        let expected = if let Some(error) = case.get("error") {
            Err(error.as_str().unwrap().to_owned())
        } else {
            Ok(case["expected"].clone())
        };
        if actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, actual {actual:?}",
                case["name"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
