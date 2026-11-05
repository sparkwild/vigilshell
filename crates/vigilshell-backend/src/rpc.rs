//! JSON-RPC 控制调度与有界 SQLite FIFO；工作线程不拥有连接生命周期。
use crate::support;
use crate::workspace::{WorkspaceOptions, WorkspaceRouter};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use vigilshell_application::SettingsService as PreferencesService;
use vigilshell_application::ports::PreferencesRepository as PreferencesStore;
use vigilshell_application::ports::{DbCommand, DbReply, WorkspaceRepository};
use vigilshell_desktop_protocol::workspace::WorkspaceRequest;
use vigilshell_desktop_protocol::*;
use vigilshell_domain::SettingsError as PreferencesError;
use vigilshell_domain::workspace::WorkspaceError;
const APPLICATION: &str = "vigilshell";
const HISTORY: usize = 256;
const CONTROL_CAPACITY: usize = 8;
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Queued,
    Running,
    Cancelled,
}
#[derive(Default)]
struct Registry {
    active: HashMap<String, (Phase, bool)>,
    completed: VecDeque<String>,
    writes: VecDeque<(String, WriteOutcome)>,
}
impl Registry {
    fn known(&self, id: &str) -> bool {
        self.active.contains_key(id)
            || self.completed.iter().any(|x| x == id)
            || self.writes.iter().any(|x| x.0 == id)
    }
    fn finish(&mut self, id: String, outcome: Option<WriteOutcome>) {
        self.active.remove(&id);
        if let Some(value) = outcome {
            self.writes.push_back((id.clone(), value));
            if self.writes.len() > HISTORY {
                self.writes.pop_front();
            }
        }
        self.completed.push_back(id);
        if self.completed.len() > HISTORY {
            self.completed.pop_front();
        }
    }
    fn outcome(&self, id: &str) -> Option<WriteOutcome> {
        self.writes.iter().find(|x| x.0 == id).map(|x| x.1.clone())
    }
}
fn failure(error: PreferencesError) -> RpcError {
    match error {
        PreferencesError::Conflict(p) => RpcError {
            data: ErrorData {
                code: "conflict".into(),
                preferences: Some(Box::new((*p).into())),
            },
            ..RpcError::app("conflict")
        },
        PreferencesError::RevisionLimit => RpcError::app("revision_limit"),
        PreferencesError::InvalidRequest => RpcError::standard(-32602, "invalid_params"),
        PreferencesError::StorageUnavailable => RpcError::app("storage_unavailable"),
    }
}
fn response(id: String, result: Result<RpcValue, RpcError>) -> Value {
    match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
    }
}
fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::other("truncated_frame"))
            };
        }
        let end = available.iter().position(|b| *b == b'\n');
        let length = end.unwrap_or(available.len());
        if frame.len() + length > MAX_AGENT_FRAME_BYTES {
            return Err(io::Error::other("frame_too_large"));
        }
        frame.extend_from_slice(&available[..length]);
        reader.consume(length + usize::from(end.is_some()));
        if end.is_some() {
            crate::framing::validate_input(&frame)?;
            return Ok(Some(frame));
        }
    }
}
/// validate the envelope before the method enum so malformed input never reaches storage.
fn decode(bytes: &[u8]) -> Result<Option<RuntimeRequest>, (Option<String>, RpcError)> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| (None, RpcError::standard(-32700, "parse_error")))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .map(str::to_owned);
    let object = value
        .as_object()
        .ok_or_else(|| (None, RpcError::standard(-32600, "invalid_request")))?;
    if object.get("jsonrpc") != Some(&json!("2.0"))
        || !object.get("method").is_some_and(Value::is_string)
        || object
            .keys()
            .any(|k| !["jsonrpc", "id", "method", "params"].contains(&k.as_str()))
    {
        return Err((id, RpcError::standard(-32600, "invalid_request")));
    }
    // Notifications never run privileged request methods and never receive responses.
    if !object.contains_key("id") {
        return Ok(None);
    }
    if id.is_none() {
        return Err((None, RpcError::standard(-32600, "invalid_request")));
    }
    let method = object["method"].as_str().unwrap();
    if ![
        "runtime.hello",
        "uiState.get",
        "uiState.update",
        "uiState.reconcile",
        "preferences.get",
        "preferences.update",
        "preferences.reconcile",
        "requests.getStatus",
        "requests.cancel",
        "runtime.shutdown",
    ]
    .contains(&method)
    {
        return Err((id, RpcError::standard(-32601, "method_not_found")));
    }
    let mut command =
        json!({"method":method,"params":object.get("params").cloned().unwrap_or(json!({}))});
    let command: RpcCommand = serde_json::from_value(command.take())
        .map_err(|_| (id.clone(), RpcError::standard(-32602, "invalid_params")))?;
    if let RpcCommand::Update(ref u) = command {
        let request: vigilshell_domain::UpdatePreferencesRequest = (*u.clone()).into();
        request
            .validate()
            .map_err(|_| (id.clone(), RpcError::standard(-32602, "invalid_params")))?;
    }
    if let RpcCommand::UiUpdate(ref u) = command {
        let request: vigilshell_domain::ui_state::UiStateUpdate = u.clone().into();
        request
            .validate()
            .map_err(|_| (id.clone(), RpcError::standard(-32602, "invalid_params")))?;
    }
    Ok(Some(RuntimeRequest {
        jsonrpc: "2.0".into(),
        id: id.unwrap(),
        command,
    }))
}
pub(crate) enum Job {
    Control {
        request: RuntimeRequest,
    },
    Workspace {
        command: Box<DbCommand>,
        reply: oneshot::Sender<Result<DbReply, WorkspaceError>>,
    },
    Stop,
}
type WorkspaceExecutor<S> = fn(&mut S, DbCommand) -> Result<DbReply, WorkspaceError>;

pub async fn serve_with_workspace<R, W, S, F>(
    reader: R,
    writer: W,
    open: F,
    runtime_id: String,
    options: WorkspaceOptions,
) -> io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
    S: PreferencesStore + WorkspaceRepository + Send + 'static,
    F: FnOnce() -> io::Result<PreferencesService<S>> + Send + 'static,
{
    serve_inner(
        reader,
        writer,
        open,
        runtime_id,
        Some(options),
        Some(|store, command| store.execute(command)),
    )
    .await
}

pub async fn serve<R, W, S, F>(reader: R, writer: W, open: F, runtime_id: String) -> io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
    S: PreferencesStore + Send + 'static,
    F: FnOnce() -> io::Result<PreferencesService<S>> + Send + 'static,
{
    serve_inner(reader, writer, open, runtime_id, None, None).await
}
async fn serve_inner<R, W, S, F>(
    reader: R,
    writer: W,
    open: F,
    runtime_id: String,
    workspace_options: Option<WorkspaceOptions>,
    workspace_execute: Option<WorkspaceExecutor<S>>,
) -> io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
    S: PreferencesStore + Send + 'static,
    F: FnOnce() -> io::Result<PreferencesService<S>> + Send + 'static,
{
    let shutdown = CancellationToken::new();
    let (input_tx, mut input) = mpsc::channel(support::QUEUE_CAPACITY);
    let reader_shutdown = shutdown.clone();
    let _reader = support::thread("stdin", move || {
        let mut reader = reader;
        loop {
            let frame = read_frame(&mut reader);
            let terminal = !matches!(frame, Ok(Some(_)));
            if terminal {
                reader_shutdown.cancel();
            }
            if input_tx.blocking_send(frame).is_err() || terminal {
                break;
            }
        }
        Ok(())
    })?;
    let (output_tx, mut output) = mpsc::channel::<Value>(128);
    let writer_done = support::thread("stdout", move || {
        let mut writer = writer;
        while let Some(value) = output.blocking_recv() {
            let bytes = crate::framing::encode_output(&value)?;
            writer.write_all(&bytes)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        writer.flush()
    })?;
    let tracker = TaskTracker::new();
    let failed = CancellationToken::new();
    let fail_writer = failed.clone();
    let writer_task = tracker.spawn(async move {
        let r = support::completion(writer_done).await;
        if r.is_err() {
            fail_writer.cancel();
        }
        r
    });
    let registry = Arc::new(Mutex::new(Registry::default()));
    let worker_registry = registry.clone();
    let (jobs_tx, mut jobs) = mpsc::channel::<Job>(support::QUEUE_CAPACITY + CONTROL_CAPACITY);
    let worker_output = output_tx.clone();
    let worker_id = runtime_id.clone();
    let worker_done = support::thread("sqlite", move || {
        // Storage failure is a service error; the independent hello/control path stays available.
        let mut service = open();
        while let Some(job) = jobs.blocking_recv() {
            let request = match job {
                Job::Stop => break,
                Job::Workspace { command, reply } => {
                    let result = match (service.as_mut(), workspace_execute) {
                        (Ok(service), Some(execute)) => execute(service.repository_mut(), *command),
                        _ => Err(WorkspaceError::StorageUnavailable),
                    };
                    let _ = reply.send(result);
                    continue;
                }
                Job::Control { request } => request,
            };
            let started = std::time::Instant::now();
            let cancelled = {
                let mut r = worker_registry.lock().unwrap();
                let entry = r.active.get_mut(&request.id).unwrap();
                let cancelled = entry.0 == Phase::Cancelled;
                if !cancelled {
                    entry.0 = Phase::Running;
                }
                cancelled
            };
            let is_update = matches!(
                &request.command,
                RpcCommand::Update(_) | RpcCommand::UiUpdate(_)
            );
            let result = if cancelled {
                Err(RpcError::app("cancelled"))
            } else {
                match service.as_mut() {
                    Err(_) => Err(RpcError::app("storage_unavailable")),
                    Ok(service) => match request.command {
                        RpcCommand::UiGet { scope } => service
                            .repository_mut()
                            .ui_state(&scope)
                            .map(|v| RpcValue::UiState { state: v.into() })
                            .map_err(failure),
                        RpcCommand::UiUpdate(update) => service
                            .repository_mut()
                            .update_ui_state(update.into())
                            .map(|v| RpcValue::UiState { state: v.into() })
                            .map_err(failure),
                        RpcCommand::UiReconcile {
                            scope,
                            request_id,
                            runtime_id,
                        } => service
                            .repository_mut()
                            .ui_state(&scope)
                            .map(|v| RpcValue::UiReconciled {
                                state: v.into(),
                                outcome: if runtime_id == worker_id {
                                    worker_registry.lock().unwrap().outcome(&request_id)
                                } else {
                                    None
                                },
                            })
                            .map_err(failure),

                        RpcCommand::Get {} => service
                            .get()
                            .map(|p| RpcValue::Preferences {
                                preferences: p.into(),
                            })
                            .map_err(failure),
                        RpcCommand::Update(update) => service
                            .update((*update).into())
                            .map(|u| RpcValue::Updated {
                                outcome: if u.applied { "applied" } else { "unchanged" }.into(),
                                preferences: u.preferences.into(),
                            })
                            .map_err(failure),
                        RpcCommand::Reconcile {
                            request_id,
                            runtime_id,
                        } => service
                            .get()
                            .map(|p| RpcValue::Reconciled {
                                preferences: p.into(),
                                outcome: if runtime_id == worker_id {
                                    worker_registry.lock().unwrap().outcome(&request_id)
                                } else {
                                    None
                                },
                            })
                            .map_err(failure),
                        _ => unreachable!(),
                    },
                }
            };
            let write = if is_update {
                Some(match &result {
                    Ok(RpcValue::UiState { state }) => WriteOutcome {
                        status: "applied".into(),
                        ui_state: Some(state.clone()),
                        preferences: None,
                        error: None,
                    },
                    Ok(RpcValue::Updated {
                        outcome,
                        preferences,
                    }) => WriteOutcome {
                        ui_state: None,
                        status: outcome.clone(),
                        preferences: Some(preferences.clone()),
                        error: None,
                    },
                    Err(e) => WriteOutcome {
                        ui_state: None,
                        status: "rejected".into(),
                        preferences: e.data.preferences.as_deref().cloned(),
                        error: Some(e.clone()),
                    },
                    _ => unreachable!(),
                })
            } else {
                None
            };
            // Keep the evidence before either the change event or the response can be lost.
            worker_registry
                .lock()
                .unwrap()
                .finish(request.id.clone(), write);
            if let Ok(RpcValue::Updated {
                outcome,
                preferences,
            }) = &result
                && outcome == "applied"
            {
                worker_output.blocking_send(json!({"jsonrpc":"2.0","method":"preferences.changed","params":{"runtimeId":worker_id,"preferences":preferences}})).map_err(|_|io::Error::other("output_closed"))?;
            }
            tracing::info!(application=APPLICATION,pid=std::process::id(),request_id=%request.id,stage="request_finished",elapsed_ms=started.elapsed().as_millis() as u64,outcome=if result.is_ok(){"ok"}else{"failed"});
            worker_output
                .blocking_send(response(request.id, result))
                .map_err(|_| io::Error::other("output_closed"))?;
        }
        Ok(())
    })?;
    let fail_worker = failed.clone();
    let worker_task = tracker.spawn(async move {
        let r = support::completion(worker_done).await;
        if r.is_err() {
            fail_worker.cancel();
        }
        r
    });
    let workspace = workspace_options
        .map(|options| {
            WorkspaceRouter::new(
                jobs_tx.clone(),
                output_tx.clone(),
                options,
                runtime_id.clone(),
                failed.clone(),
            )
        })
        .transpose()?;
    let workspace_tasks = TaskTracker::new();
    let begin_shutdown = shutdown.clone();
    let work = async move {
        let mut hello = false;
        let mut shutdown_id = None;
        loop {
            let frame = tokio::select! { ()=failed.cancelled()=>return Err(io::Error::other("worker_failed")), f=input.recv()=>f.ok_or_else(||io::Error::other("reader_failed"))? }?;
            let Some(bytes) = frame else { break };
            if let Some(workspace) = &workspace {
                let value: Option<Value> = serde_json::from_slice(&bytes).ok();
                let is_workspace = value.as_ref().is_some_and(|v| {
                    v.get("owner").is_some()
                        || v.get("method").and_then(Value::as_str).is_some_and(|m| {
                            m.starts_with("workspace.")
                                || m.starts_with("catalogue.")
                                || m.starts_with("connections.")
                                || m.starts_with("groups.")
                                || m.starts_with("credential")
                                || m.starts_with("hostTrust.")
                                || m.starts_with("terminals.")
                                || m.starts_with("operations.")
                        })
                });
                if is_workspace {
                    let id = value
                        .as_ref()
                        .and_then(|v| v.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if id.is_none() {
                        continue;
                    }
                    let parsed = serde_json::from_slice::<WorkspaceRequest>(&bytes);
                    let valid = value.as_ref().and_then(Value::as_object).is_some_and(|o| {
                        o.keys().all(|k| {
                            ["jsonrpc", "id", "method", "params", "owner"].contains(&k.as_str())
                        })
                    });
                    let request = match parsed {
                        Ok(r)
                            if valid
                                && r.jsonrpc == "2.0"
                                && !r.id.is_empty()
                                && r.id.len() <= 128 =>
                        {
                            r
                        }
                        _ => {
                            output_tx.send(json!({"jsonrpc":"2.0","id":id,"error":RpcError::standard(-32602,"invalid_params")})).await.map_err(|_|io::Error::other("output_closed"))?;
                            continue;
                        }
                    };
                    if !hello {
                        output_tx
                            .send(response(
                                request.id,
                                Err(RpcError::app("protocol_mismatch")),
                            ))
                            .await
                            .map_err(|_| io::Error::other("output_closed"))?;
                        continue;
                    }
                    let error = {
                        let mut r = registry.lock().unwrap();
                        if r.known(&request.id) {
                            Some("duplicate_id")
                        } else if r.active.len() >= support::QUEUE_CAPACITY {
                            Some("busy")
                        } else {
                            r.active.insert(request.id.clone(), (Phase::Running, false));
                            None
                        }
                    };
                    if let Some(code) = error {
                        output_tx
                            .send(response(request.id, Err(RpcError::app(code))))
                            .await
                            .map_err(|_| io::Error::other("output_closed"))?;
                        continue;
                    }
                    let workspace = workspace.clone();
                    let output = output_tx.clone();
                    let registry = registry.clone();
                    workspace_tasks.spawn(async move {
                        let id = request.id.clone();
                        let result = workspace
                            .request(request)
                            .await
                            .map(|result| RpcValue::Workspace {
                                result: Box::new(result),
                            })
                            .map_err(|e| RpcError::app(e.code()));
                        registry.lock().unwrap().finish(id.clone(), None);
                        let _ = output.send(response(id, result)).await;
                    });
                    continue;
                }
            }
            let request = match decode(&bytes) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err((id, error)) => {
                    output_tx
                        .try_send(json!({"jsonrpc":"2.0","id":id,"error":error}))
                        .map_err(|_| io::Error::other("output_backpressure"))?;
                    continue;
                }
            };
            let id = request.id.clone();
            let mut state = registry.lock().unwrap();
            if state.known(&id) {
                output_tx.try_send(json!({"jsonrpc":"2.0","id":null,"error":RpcError::standard(-32600,"duplicate_id")})).map_err(|_|io::Error::other("output_backpressure"))?;
                continue;
            }
            let immediate = if !hello && !matches!(&request.command, RpcCommand::Hello { .. }) {
                Some(Err(RpcError::app("protocol_mismatch")))
            } else {
                match &request.command {
                    RpcCommand::Hello {
                        application,
                        contract_version,
                    } => Some(
                        if application == APPLICATION && *contract_version == PROTOCOL_VERSION {
                            hello = true;
                            Ok(RpcValue::Ready {
                                application: APPLICATION.into(),
                                contract_version: PROTOCOL_VERSION,
                                runtime_id: runtime_id.clone(),
                            })
                        } else {
                            Err(RpcError::app("protocol_mismatch"))
                        },
                    ),
                    RpcCommand::Status { request_id } => Some(Ok(RpcValue::RequestStatus {
                        state: match state.active.get(request_id) {
                            Some((Phase::Queued, _)) => "queued",
                            Some((Phase::Running, _)) => "running",
                            Some((Phase::Cancelled, _)) => "queued",
                            None if state.known(request_id) => "completed",
                            None => "unknown",
                        }
                        .into(),
                        outcome: state.outcome(request_id),
                    })),
                    RpcCommand::Cancel { request_id } => Some(
                        if let Some((phase, _)) = state.active.get_mut(request_id)
                            && *phase == Phase::Queued
                        {
                            *phase = Phase::Cancelled;
                            Ok(RpcValue::Cancelled {})
                        } else {
                            Err(RpcError::app("too_late"))
                        },
                    ),
                    RpcCommand::Shutdown {} => {
                        shutdown_id = Some(id.clone());
                        begin_shutdown.cancel();
                        break;
                    }
                    _ => None,
                }
            };
            if let Some(result) = immediate {
                state.finish(id.clone(), None);
                output_tx
                    .try_send(response(id, result))
                    .map_err(|_| io::Error::other("output_backpressure"))?;
            } else {
                let control = matches!(
                    &request.command,
                    RpcCommand::Reconcile { .. } | RpcCommand::UiReconcile { .. }
                );
                let count = state.active.values().filter(|v| v.1 == control).count();
                if count
                    >= if control {
                        CONTROL_CAPACITY
                    } else {
                        support::QUEUE_CAPACITY
                    }
                {
                    state.finish(id.clone(), None);
                    output_tx
                        .try_send(response(id, Err(RpcError::app("busy"))))
                        .map_err(|_| io::Error::other("output_backpressure"))?;
                } else {
                    state.active.insert(id, (Phase::Queued, control));
                    jobs_tx
                        .try_send(Job::Control { request })
                        .map_err(|_| io::Error::other("worker_lost"))?;
                }
            }
        }
        begin_shutdown.cancel();
        if let Some(workspace) = &workspace {
            workspace.stop().await?;
        }
        workspace_tasks.close();
        workspace_tasks.wait().await;
        drop(workspace);
        let _ = jobs_tx.send(Job::Stop).await;
        drop(jobs_tx);
        worker_task
            .await
            .map_err(|_| io::Error::other("worker_panicked"))??;
        if let Some(id) = shutdown_id {
            output_tx
                .send(response(id, Ok(RpcValue::Stopped {})))
                .await
                .map_err(|_| io::Error::other("output_closed"))?;
        }
        drop(output_tx);
        writer_task
            .await
            .map_err(|_| io::Error::other("writer_panicked"))??;
        Ok(())
    };
    let result = support::supervise(work, shutdown.clone()).await;
    shutdown.cancel();
    tracker.close();
    if result.is_ok() {
        tracker.wait().await;
    }
    result
}

#[cfg(test)]
mod conformance {
    #[test]
    fn shared_request_examples() {
        let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/desktop-protocol-v12.json"
        ))
        .unwrap();
        for case in cases {
            let bytes = serde_json::to_vec(&case["request"]).unwrap();
            let decoded = super::decode(&bytes);
            if case["valid"] == true {
                assert!(matches!(decoded, Ok(Some(_))), "{}", case["name"]);
            } else {
                assert_eq!(
                    decoded.unwrap_err().1.code,
                    case["error"].as_i64().unwrap() as i32,
                    "{}",
                    case["name"]
                );
            }
        }
    }
}

#[cfg(test)]
mod worker_faults {
    use super::*;
    struct Store;
    impl PreferencesStore for Store {
        fn get(&self) -> Result<vigilshell_domain::Preferences, PreferencesError> {
            Ok(Default::default())
        }
        fn update(
            &mut self,
            _: vigilshell_domain::UpdatePreferencesRequest,
        ) -> Result<vigilshell_domain::PreferencesUpdate, PreferencesError> {
            Err(PreferencesError::InvalidRequest)
        }
    }
    fn input() -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(format!("{}\n",json!({"jsonrpc":"2.0","id":"hello","method":"runtime.hello","params":{"application":APPLICATION,"contractVersion":4}})).into_bytes())
    }
    #[tokio::test]
    async fn observes_storage_worker_panics_without_waiting_for_stdin() {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            serve::<_, _, Store, _>(
                input(),
                Vec::new(),
                || panic!("worker fault"),
                "test".into(),
            ),
        )
        .await
        .unwrap();
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn observes_broken_output_and_stops() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            serve(
                input(),
                Broken,
                || Ok(PreferencesService::new(Store)),
                "test".into(),
            ),
        )
        .await
        .unwrap();
        assert!(result.is_err());
    }
}
