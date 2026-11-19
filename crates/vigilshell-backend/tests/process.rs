use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tempfile::tempdir;
use vigilshell_desktop_protocol::{
    MAX_FRAME_BYTES, PROTOCOL_VERSION, workspace::WORKSPACE_PROTOCOL_VERSION,
};

struct Runtime {
    child: Child,
    lines: Receiver<String>,
}

impl Runtime {
    fn start(directory: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vigilshell-backend"))
            .arg("--data-dir")
            .arg(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    fn send(&mut self, request: Value) -> Value {
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        self.write(&bytes);
        loop {
            let response = self.response();
            if response.get("id").is_some() {
                return response;
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }

    fn response(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(3))
            .expect("runtime must answer within 3 seconds");
        serde_json::from_str(&line).expect("stdout must contain only JSON protocol responses")
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not exit within 3 seconds"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn stderr(&mut self) -> String {
        let mut output = String::new();
        self.child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        output
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn req(id: &str, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn hello(r: &mut Runtime) -> String {
    r.send(req(
        "hello",
        "runtime.hello",
        json!({"application":"vigilshell","contractVersion":PROTOCOL_VERSION}),
    ))["result"]["runtimeId"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn update(theme: Option<&str>, locale: Option<&str>) -> Value {
    let mut patch = json!({});
    let mut base = json!({});
    if let Some(v) = theme {
        patch["themeMode"] = json!(v);
        base["themeMode"] = json!("system");
    }
    if let Some(v) = locale {
        patch["localeMode"] = json!(v);
        base["localeMode"] = json!("system");
    }
    json!({"patch":patch,"base":base,"expectedRevision":0})
}
#[test]
fn writes_merge_conflict_reconcile_and_restart() {
    let dir = tempdir().unwrap();
    let mut r = Runtime::start(dir.path());
    let id = hello(&mut r);
    assert_eq!(
        r.send(req("a", "preferences.update", update(Some("dark"), None)))["result"]["outcome"],
        "applied"
    );
    let b = r.send(req("b", "preferences.update", update(None, Some("en"))));
    assert_eq!(b["result"]["preferences"]["revision"], 2);
    let c = r.send(req(
        "c",
        "preferences.update",
        update(Some("light"), Some("zh-CN")),
    ));
    assert_eq!(c["error"]["data"]["code"], "conflict");
    assert_eq!(c["error"]["data"]["preferences"]["localeMode"], "en");
    assert_eq!(
        r.send(req("d", "preferences.update", update(Some("dark"), None)))["result"]["outcome"],
        "unchanged"
    );
    let result = r.send(req(
        "check",
        "preferences.reconcile",
        json!({"requestId":"a","runtimeId":id}),
    ));
    assert_eq!(result["result"]["outcome"]["status"], "applied");
    assert_eq!(result["result"]["preferences"]["revision"], 2);
    assert_eq!(
        r.send(req("stop", "runtime.shutdown", json!({})))["result"]["kind"],
        "stopped"
    );
    assert!(r.wait().success());
    for line in r.stderr().lines() {
        let log: Value = serde_json::from_str(line).unwrap();
        assert!(log["fields"].get("preferences").is_none());
        assert!(log["fields"].get("patch").is_none());
    }
    let mut second = Runtime::start(dir.path());
    assert_ne!(hello(&mut second), id);
    assert_eq!(
        second.send(req("read", "preferences.get", json!({})))["result"]["preferences"],
        b["result"]["preferences"]
    );
    drop(second.child.stdin.take());
    assert!(second.wait().success());
}
#[test]
fn framing_notifications_and_invalid_input_do_not_execute_writes() {
    let dir = tempdir().unwrap();
    let mut r = Runtime::start(dir.path());
    hello(&mut r);
    r.write(b"invalid\n");
    assert_eq!(r.response()["error"]["code"], -32700);
    r.write(b"{\"jsonrpc\":\"2.0\",\"method\":\"runtime.shutdown\"}\n");
    for (i,params) in [json!({}),json!({"patch":{"themeMode":null},"base":{"themeMode":"system"},"expectedRevision":0}),json!({"patch":{"themeMode":"dark"},"base":{},"expectedRevision":0}),json!({"patch":{"themeMode":"dark"},"base":{"themeMode":"system"},"expectedRevision":9007199254740992u64})].into_iter().enumerate(){
 assert_eq!(r.send(req(&format!("bad-{i}"),"preferences.update",params))["error"]["code"],-32602);}
    assert_eq!(
        r.send(req("x", "unknown", json!({})))["error"]["code"],
        -32601
    );
    assert_eq!(
        r.send(req("read", "preferences.get", json!({})))["result"]["preferences"]["revision"],
        0
    );
    assert_eq!(
        r.send(req("read", "preferences.get", json!({})))["error"]["data"]["code"],
        "duplicate_id"
    );
    drop(r.child.stdin.take());
    assert!(r.wait().success());
}
#[test]
fn framing_limits_eof_and_directory_release() {
    for bytes in [
        vec![],
        vec![b'x'; MAX_FRAME_BYTES + 1],
        b"{\"secret\":\"do-not-log\"}".to_vec(),
    ] {
        let dir = tempdir().unwrap();
        let mut r = Runtime::start(dir.path());
        hello(&mut r);
        r.write(&bytes);
        drop(r.child.stdin.take());
        assert_eq!(r.wait().success(), bytes.is_empty());
        assert!(!r.stderr().contains("do-not-log"));
        assert!(vigilshell_storage_sqlite::SqlitePreferences::open(dir.path()).is_ok());
    }
}
#[test]
fn storage_failure_is_distinct_from_handshake() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("app.sqlite3");
    std::fs::write(&file, b"corrupt").unwrap();
    let mut r = Runtime::start(dir.path());
    hello(&mut r);
    assert_eq!(
        r.send(req("read", "preferences.get", json!({})))["error"]["data"]["code"],
        "storage_unavailable"
    );
    drop(r.child.stdin.take());
    assert!(r.wait().success());
    assert_eq!(std::fs::read(file).unwrap(), b"corrupt");
}
#[test]
fn result_history_is_bounded_and_old_instance_is_unknown() {
    let dir = tempdir().unwrap();
    let mut r = Runtime::start(dir.path());
    let id = hello(&mut r);
    for n in 0..257 {
        r.send(req(
            &format!("save-{n}"),
            "preferences.update",
            update(Some("system"), None),
        ));
    }
    let old = r.send(req(
        "check",
        "preferences.reconcile",
        json!({"requestId":"save-0","runtimeId":id}),
    ));
    assert!(old["result"]["outcome"].is_null());
    let wrong = r.send(req(
        "wrong",
        "preferences.reconcile",
        json!({"requestId":"save-256","runtimeId":"other"}),
    ));
    assert!(wrong["result"]["outcome"].is_null());
    drop(r.child.stdin.take());
    assert!(r.wait().success());
}

#[test]
fn slow_sqlite_has_bounded_admission_and_independent_control() {
    let dir = tempdir().unwrap();
    let mut r = Runtime::start(dir.path());
    let runtime_id = hello(&mut r);
    r.send(req("initial", "preferences.get", json!({})));
    let lock = rusqlite::Connection::open(dir.path().join("app.sqlite3")).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let start = Instant::now();
    for n in 0..65 {
        r.write(
            format!(
                "{}\n",
                req(
                    &format!("write-{n}"),
                    "preferences.update",
                    update(Some("dark"), None)
                )
            )
            .as_bytes(),
        );
    }
    r.write(
        format!(
            "{}\n",
            req("cancel", "requests.cancel", json!({"requestId":"write-63"}))
        )
        .as_bytes(),
    );
    r.write(
        format!(
            "{}\n",
            req(
                "status",
                "requests.getStatus",
                json!({"requestId":"write-62"})
            )
        )
        .as_bytes(),
    );
    let mut busy = false;
    let mut cancelled = false;
    loop {
        let message = r.response();
        if message["id"] == "write-64" {
            busy = message["error"]["data"]["code"] == "busy";
        }
        if message["id"] == "cancel" {
            cancelled = message["result"]["kind"] == "cancelled";
        }
        if message["id"] == "status" {
            assert_eq!(message["result"]["state"], "queued");
            break;
        }
    }
    assert!(start.elapsed() < Duration::from_millis(1800));
    assert!(busy && cancelled);
    lock.execute_batch("ROLLBACK").unwrap();
    r.write(
        format!(
            "{}\n",
            req(
                "barrier",
                "preferences.reconcile",
                json!({"requestId":"write-63","runtimeId":runtime_id})
            )
        )
        .as_bytes(),
    );
    loop {
        let m = r.response();
        if m["id"] == "barrier" {
            assert_eq!(m["result"]["outcome"]["error"]["data"]["code"], "cancelled");
            break;
        }
    }
    drop(r.child.stdin.take());
    assert!(r.wait().success());
}

#[test]
fn oversized_agent_envelope_is_validated_without_killing_the_rpc_process() {
    let dir = tempdir().unwrap();
    let mut r = Runtime::start(dir.path());
    hello(&mut r);
    let response = r.send(json!({"jsonrpc":"2.0","id":"large-agent","owner":"fixture","method":"agent.request","params":{"invalid":"x".repeat(70000)}}));
    assert_eq!(response["error"]["message"], "invalid_params");
    assert_eq!(
        r.send(req("after", "preferences.get", json!({})))["result"]["kind"],
        "preferences"
    );
    drop(r.child.stdin.take());
    assert!(r.wait().success());
}

#[test]
fn workspace_rejects_previous_version_and_negotiates_current_without_restarting_backend() {
    let dir = tempdir().unwrap();
    let mut runtime = Runtime::start(dir.path());
    hello(&mut runtime);
    let owner = "11111111-1111-4111-8111-111111111111";
    let previous = runtime.send(json!({
        "jsonrpc":"2.0", "id":"old-workspace", "owner":owner,
        "method":"workspace.negotiate", "params":{"version":WORKSPACE_PROTOCOL_VERSION - 1}
    }));
    assert_eq!(previous["error"]["data"]["code"], "invalid_input");
    let current = runtime.send(json!({
        "jsonrpc":"2.0", "id":"current-workspace", "owner":owner,
        "method":"workspace.negotiate", "params":{"version":WORKSPACE_PROTOCOL_VERSION}
    }));
    assert_eq!(current["result"]["result"]["kind"], "ready");
    assert_eq!(
        current["result"]["result"]["version"],
        WORKSPACE_PROTOCOL_VERSION
    );
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());
}

#[test]
fn optional_ui_rows_do_not_prevent_restart_and_repair() {
    let dir = tempdir().unwrap();
    let mut runtime = Runtime::start(dir.path());
    hello(&mut runtime);
    runtime.send(req("init", "preferences.get", json!({})));
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());

    let a = "11111111-1111-4111-8111-111111111111";
    let b = "22222222-2222-4222-8222-222222222222";
    let a_scope = format!("connection:{a}");
    let b_scope = format!("connection:{b}");
    let bad_picker = r#"{"kind":"pickers","upload":"relative","download":""}"#;
    let bad_directory =
        r#"{"kind":"directory","connectionRevision":1,"path":3,"scroll":0,"expanded":[]}"#;
    let directory = json!({"kind":"directory","connectionRevision":1,"path":"/remote","scroll":0,"expanded":[]});
    let db = rusqlite::Connection::open(dir.path().join("app.sqlite3")).unwrap();
    db.execute_batch(
        "UPDATE credential_vault SET initialized=1;
         UPDATE connection_configuration SET verifier=zeroblob(48);",
    )
    .unwrap();
    for id in [a, b] {
        db.execute(
            "INSERT INTO connection_profiles(id, group_sort_order, version, revision, config) VALUES(?1,0,1,1,zeroblob(48))",
            [id],
        ).unwrap();
    }
    for (scope, body) in [
        ("pickers", bad_picker),
        (&a_scope, bad_directory),
        (&b_scope, &directory.to_string()),
        ("window", "{ }"),
    ] {
        db.execute("INSERT INTO ui_state VALUES(?1,7,?2)", [scope, body])
            .unwrap();
    }
    let raw = |scope: &str| -> (i64, String) {
        db.query_row(
            "SELECT revision,data FROM ui_state WHERE scope=?1",
            [scope],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    let mut runtime = Runtime::start(dir.path());
    hello(&mut runtime);
    assert_eq!(
        runtime.send(req("prefs", "preferences.get", json!({})))["result"]["kind"],
        "preferences"
    );
    let mut workspace = |id, method, params| {
        let mut message = req(id, method, params);
        message["owner"] = json!(a);
        runtime.send(message)
    };
    assert_eq!(
        workspace(
            "ready",
            "workspace.negotiate",
            json!({"version":WORKSPACE_PROTOCOL_VERSION})
        )["result"]["result"]["kind"],
        "ready"
    );
    assert_eq!(
        workspace(
            "groups",
            "catalogue.list",
            json!({"kind":"groups","query":"","offset":0,"version":null})
        )["result"]["result"]["kind"],
        "catalogue"
    );
    let picker = json!({"kind":"pickers","upload":"/tmp/saved","download":""});
    for (scope, bad, good) in [
        ("pickers", bad_picker, &picker),
        (&a_scope, bad_directory, &directory),
    ] {
        let state = runtime.send(req(
            &format!("get-{scope}"),
            "uiState.get",
            json!({"scope":scope}),
        ));
        assert_eq!(state["result"]["state"]["revision"], 7, "{state}");
        assert_eq!(state["result"]["state"]["data"], Value::Null);
        assert_eq!(raw(scope), (7, bad.into()));
        let write = runtime.send(req(
            &format!("repair-{scope}"),
            "uiState.update",
            json!({"scope":scope,"expectedRevision":7,"data":good}),
        ));
        assert_eq!(write["result"]["state"]["revision"], 8, "{write}");
    }
    let stale = runtime.send(req("stale", "uiState.update", json!({"scope":"pickers","expectedRevision":7,"data":{"kind":"pickers","upload":"/tmp/stale","download":""}})));
    assert!(stale.get("error").is_some());
    for (method, params) in [
        ("uiState.get", json!({"scope":"window"})),
        (
            "uiState.update",
            json!({"scope":"window","expectedRevision":7,"data":picker}),
        ),
    ] {
        assert!(
            runtime
                .send(req(method, method, params))
                .get("error")
                .is_some()
        );
    }
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());
    let mut runtime = Runtime::start(dir.path());
    hello(&mut runtime);
    for (scope, expected) in [
        ("pickers", &picker),
        (&a_scope, &directory),
        (&b_scope, &directory),
    ] {
        assert_eq!(
            &runtime.send(req(
                &format!("get-{scope}"),
                "uiState.get",
                json!({"scope":scope})
            ))["result"]["state"]["data"],
            expected
        );
    }
    assert_eq!(raw("window"), (7, "{ }".into()));
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());
}

#[test]
fn workspace_keeps_group_and_proxy_results_and_rejects_retired_commands() {
    let dir = tempdir().unwrap();
    let mut runtime = Runtime::start(dir.path());
    hello(&mut runtime);
    let owner = "11111111-1111-4111-8111-111111111111";
    let mut call = |id: &str, method: &str, params: Value| {
        runtime.send(json!({
            "jsonrpc":"2.0", "id":id, "owner":owner, "method":method, "params":params
        }))
    };
    assert_eq!(
        call(
            "ready",
            "workspace.negotiate",
            json!({"version":WORKSPACE_PROTOCOL_VERSION})
        )["result"]["result"]["kind"],
        "ready"
    );
    let group = call("create-group", "groups.create", json!({"name":"group"}))["result"]["result"]
        ["group"]
        .clone();
    assert_eq!(group["name"], "group");
    let groups = call(
        "groups",
        "catalogue.list",
        json!({"kind":"groups","query":"group","offset":0,"version":null}),
    );
    assert_eq!(
        groups["result"]["result"]["page"]["groups"][0]["id"],
        group["id"]
    );
    for (index, (method, params)) in [
        ("groups.get", json!({"id":group["id"]})),
        ("proxies.get", json!({"id":owner})),
        (
            "catalogue.search",
            json!({"kind":"connections","query":"","offset":0,"version":null}),
        ),
        (
            "catalogue.list",
            json!({"kind":"search","query":"","offset":0,"version":null}),
        ),
        (
            "catalogue.list",
            json!({"kind":"connections","parent":null,"query":"","offset":0,"version":null}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            call(&format!("retired-{index}"), method, params)
                .get("error")
                .is_some(),
            "{method}"
        );
    }
    let saved = call(
        "save-proxy",
        "proxies.save",
        json!({
            "proxyId":null,"expectedVersion":null,"draftId":null,"clearPassword":false,
            "config":{"name":"proxy","kind":"socks5","host":"127.0.0.1","port":1080,"username":""}
        }),
    );
    let operation = saved["result"]["result"]["operation"]["id"]
        .as_str()
        .expect("proxy save must remain an operation")
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut sequence = 0;
    let proxy = loop {
        let result = call(
            &format!("operation-{sequence}"),
            "operations.get",
            json!({"id":operation}),
        );
        let value = &result["result"]["result"]["operation"];
        if value["phase"] == "success" {
            break value["proxy"].clone();
        }
        assert_eq!(value["phase"], "working", "{result}");
        assert!(Instant::now() < deadline);
        sequence += 1;
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(proxy["credentialSaved"], false);
    let listing = call(
        "list-proxies",
        "proxies.list",
        json!({"offset":0,"version":null}),
    );
    assert_eq!(listing["result"]["result"]["proxies"][0], proxy);
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());
}

#[test]
fn workspace_renaming_emits_only_a_group_notice_and_reconciles_the_original_write() {
    let dir = tempdir().unwrap();
    let mut runtime = Runtime::start(dir.path());
    let runtime_id = hello(&mut runtime);
    let owner = "11111111-1111-4111-8111-111111111111";
    let request = |id: &str, method: &str, params: Value| {
        let mut v = req(id, method, params);
        v["owner"] = json!(owner);
        v
    };
    runtime.send(request(
        "open",
        "workspace.negotiate",
        json!({"version":WORKSPACE_PROTOCOL_VERSION}),
    ));
    let group = runtime.send(request(
        "create-a",
        "groups.create",
        json!({"name":"Before"}),
    ))["result"]["result"]["group"]
        .clone();
    let change = request(
        "lost-response",
        "groups.update",
        json!({"id":group["id"],"version":group["version"],"name":"After"}),
    );
    runtime.write(format!("{change}\n").as_bytes());
    let mut notice = None;
    loop {
        let response = runtime.response();
        if response["method"] == "workspace.changed" {
            let event = &response["params"]["event"];
            assert_ne!(event["kind"], "catalogue");
            if event["kind"] == "groupRenamed" {
                notice = Some(event["group"].clone());
            }
        }
        if response["id"] == "lost-response" {
            break;
        }
    }
    let expected = notice.expect("rename notification precedes its response");
    assert_eq!(expected["name"], "After");
    assert_eq!(expected["version"], 2);
    let reconcile = |id| {
        request(
            id,
            "connections.reconcile",
            json!({"requestId":"lost-response","runtimeId":runtime_id}),
        )
    };
    let recovered = runtime.send(reconcile("query"));
    assert_eq!(recovered["result"]["result"]["result"]["group"], expected);
    for index in 0..256 {
        let record = runtime.send(request(
            &format!("retained-{index}"),
            "groups.update",
            json!({"id":expected["id"],"version":2,"name":"After"}),
        ));
        assert_eq!(record["result"]["result"]["group"], expected);
    }
    let evicted = runtime.send(reconcile("evicted-query"));
    assert_eq!(
        evicted["result"]["result"],
        json!({"kind":"reconciled","result":null,"error":null})
    );
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());

    let mut runtime = Runtime::start(dir.path());
    assert_ne!(hello(&mut runtime), runtime_id);
    runtime.send(request(
        "open",
        "workspace.negotiate",
        json!({"version":WORKSPACE_PROTOCOL_VERSION}),
    ));
    let unknown = runtime.send(reconcile("old-query"));
    assert_eq!(
        unknown["result"]["result"],
        json!({"kind":"reconciled","result":null,"error":null})
    );
    let duplicate = runtime.send(request(
        "new-create",
        "groups.create",
        json!({"name":"After"}),
    ));
    assert_ne!(duplicate["result"]["result"]["group"]["id"], expected["id"]);
    assert_eq!(duplicate["result"]["result"]["group"]["name"], "After (2)");
    drop(runtime.child.stdin.take());
    assert!(runtime.wait().success());
}

#[test]
fn task_recovery_uses_row_evidence_on_two_process_restarts_and_keeps_configuration_readable() {
    use vigilshell_application::ports::{DbCommand, ExecutionDbCommand, WorkspaceRepository};
    use vigilshell_domain::agent::*;
    let dir = tempdir().unwrap();
    let mut store = vigilshell_storage_sqlite::SqlitePreferences::open(dir.path()).unwrap();
    for id in ["queued", "attempted", "damaged", "invalid-phase"] {
        let mut task = ExecutionTask {
            id: id.into(),
            entry_id: "entry".into(),
            connection_id: "connection".into(),
            context_id: "session".into(),
            created_at: 1,
            finished_at: None,
            revision: 0,
            request: ExecutionRequest {
                identity: ExecutionIdentity {
                    task_id: id.into(),
                    entry_id: "entry".into(),
                    connection_id: "connection".into(),
                    session_id: "session".into(),
                    entry_generation: 1,
                    connection_revision: 1,
                    policy_revision: 1,
                },
                request_key: id.into(),
                timeout_seconds: None,
                operation: RuntimeOperation::Command {
                    command: "pwd".into(),
                    directory: ScopedPath {
                        scope_id: ACCOUNT_SCOPE_ID.into(),
                        path: "srv".into(),
                    },
                },
            },
            source_request: None,
            policy: ConnectionPolicy {
                connection_id: "connection".into(),
                connection_revision: 1,
                revision: 1,
                verify_connect: true,
                execution: ExecutionPolicy {
                    level: OperationLevel::ReadOnly,
                    ..Default::default()
                },
            },
            state: ExecutionState {
                phase: ExecutionPhase::Queued,
                dispatch: DispatchState::NotDispatched {},
                review: ScriptReviewState::NotRequired {},
                outcome: None,
                terminal: false,
            },
            phase: None,
            error: None,
            output_incomplete: false,
            file_report: None,
            native_report: None,
            script_snapshot: None,
            script_staging_path: None,
        };
        store
            .execute(DbCommand::Execution(ExecutionDbCommand::Register(
                Box::new(task.clone()),
            )))
            .unwrap();
        if id == "attempted" {
            task.revision = 1;
            task.state.begin_dispatch("attempt".into(), 2).unwrap();
            store
                .execute(DbCommand::Execution(ExecutionDbCommand::Save {
                    task: Box::new(task),
                    expected: 0,
                }))
                .unwrap();
        }
    }
    drop(store);
    let db = rusqlite::Connection::open(dir.path().join("app.sqlite3")).unwrap();
    db.execute("UPDATE agent_tasks SET body='{' WHERE id='damaged'", [])
        .unwrap();
    db.execute(
        "UPDATE agent_tasks SET body=json_set(body,'$.phase',42) WHERE id='invalid-phase'",
        [],
    )
    .unwrap();
    let damaged: Vec<(String, String, String)> = db.prepare("SELECT id,body,fingerprint FROM agent_tasks WHERE id IN ('damaged','invalid-phase') ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    for _ in 0..2 {
        let mut runtime = Runtime::start(dir.path());
        hello(&mut runtime);
        let owner = "11111111-1111-4111-8111-111111111111";
        let request = |id: &str, method: &str, params: Value| {
            let mut v = req(id, method, params);
            v["owner"] = json!(owner);
            v
        };
        runtime.send(request(
            "open",
            "workspace.negotiate",
            json!({"version":WORKSPACE_PROTOCOL_VERSION}),
        ));
        let config = |id: &str| {
            request(
                id,
                "agent.request",
                json!({"request":{"operation":"configuration"}}),
            )
        };
        let first = runtime.send(config("first"));
        assert_eq!(
            first["result"]["result"]["result"]["kind"], "configuration",
            "{first}"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        for attempt in 0.. {
            let configuration = runtime.send(config(&format!("read-{attempt}")));
            if configuration["result"]["result"]["result"]["unrecovered_tasks"]
                == json!(["damaged", "invalid-phase"])
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "recovery did not publish damaged row: {configuration}"
            );
            thread::sleep(Duration::from_millis(5));
        }
        for (id, revision, outcome) in [("queued", 1, "notExecuted"), ("attempted", 2, "unknown")] {
            let (active, body): (bool, String) = db
                .query_row(
                    "SELECT active,body FROM agent_tasks WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let value: Value = serde_json::from_str(&body).unwrap();
            assert!(!active);
            assert_eq!(value["revision"], revision);
            assert_eq!(value["state"]["outcome"]["kind"], outcome, "{value}");
        }
        assert_eq!(
            db.query_row(
                "SELECT body FROM agent_tasks WHERE id='damaged'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "{"
        );
        let mut read_agent = |id: &str, params: Value| {
            runtime.send(request(id, "agent.request", json!({"request":params})))["result"]["result"]["result"].clone()
        };
        let activity = read_agent(
            "activity",
            json!({"operation":"activitySummaries","filter":{}}),
        );
        assert_eq!(activity["kind"], "activitySummaries", "{activity}");
        assert_eq!(activity["items"].as_array().unwrap().len(), 2);
        let tasks = read_agent("history", json!({"operation":"tasks","offset":0}));
        assert_eq!(tasks["kind"], "tasks", "{tasks}");
        assert_eq!(tasks["tasks"].as_array().unwrap().len(), 2);
        for (id, body, fingerprint) in &damaged {
            let detail = read_agent(id, json!({"operation":"task","task_id":id}));
            assert_eq!(
                detail,
                json!({"kind":"error","code":"agent_task_record_unavailable"})
            );
            let (raw, receipt): (String, String) = db.query_row(
                "SELECT t.body,r.fingerprint FROM agent_tasks t JOIN agent_receipts r ON t.id=r.task_id WHERE t.id=?1",
                [id], |r| Ok((r.get(0)?,r.get(1)?)),
            ).unwrap();
            assert_eq!((&raw, &receipt), (body, fingerprint));
        }
        drop(runtime.child.stdin.take());
        assert!(runtime.wait().success());
    }
}
