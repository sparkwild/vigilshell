use std::{
    fmt, fs,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::Write,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::Condvar,
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, State, Webview, WindowEvent,
    ipc::{Channel, InvokeResponseBody},
};
use ts_rs::TS;
use uuid::Uuid;
use uuid::{Variant, Version};
use vigilshell_core::{
    connection::{
        ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion, CredentialRef,
        ManagedServerId, SshRoute, StoredHostTrust,
    },
    connection_mutation::{
        ConnectionProfileMutationService, DeleteConnectionProfileRequest,
        MutateConnectionProfileError, UpdateConnectionProfileRequest,
    },
    host_trust::{
        ConfirmHostTrustError, ConfirmHostTrustService, HostTrustMutationRepository,
        HostTrustProbeAssessment, HostTrustProbeId, HostTrustProbeStatus, ObservedHostKey,
        PendingHostTrustDecision, PendingHostTrustDecisions, PendingHostTrustInsertError,
        PendingHostTrustLookupError,
    },
    human_terminal::{
        ConnectionProfileRepository, HostTrustRepository, HumanTerminalAdmissionService,
        HumanTerminalSessionId, OpenHumanTerminalError, OpenHumanTerminalRequest,
        PreparedHumanTerminalOpen, TerminalSize,
    },
    server_workspace::{
        AddConnectionToServerRequest, CreateProjectRequest, CreateServerWithConnectionRequest,
        DeleteManagedServerRequest, DeleteProjectRequest, ManagedServer,
        ManagedServerRecordVersion, MutateServerWorkspaceError, Project, ProjectId,
        ProjectRecordVersion, ServerWorkspaceService, UpdateManagedServerRequest,
        UpdateProjectRequest,
    },
};
use vigilshell_credentials::{
    ConfigurePasswordSettlement, CredentialProviderError, PasswordConfigurationStatus,
    RemovePasswordSettlement,
};

#[cfg(all(test, debug_assertions))]
use vigilshell_credentials::HumanPasswordCredentialProvider;
use vigilshell_lifecycle_macos::{
    TerminationDecision, install_native_termination_hook, request_native_termination,
    schedule_native_termination,
};
use vigilshell_storage_sqlite::SqliteStorage;
use vigilshell_transport_russh::{
    HostKeyProbeError, RusshHostKeyProbe, RusshPasswordTerminalTransport,
    RusshTerminalControlError, RusshTerminalControlHandle, RusshTerminalExitSignal,
    RusshTerminalLifecycle, RusshTerminalOpenError, RusshTerminalOutput, RusshTerminalOutputError,
    RusshTerminalOutputSink,
};

mod desktop_data_layout;
mod human_credential_runtime;
mod human_terminal_runtime;
mod resident_lifecycle;

#[cfg(debug_assertions)]
mod debug_human_terminal_fixture;

use desktop_data_layout::{
    DESKTOP_DATA_DIR_ENV, DesktopDataLayout, SINGLE_INSTANCE_LOCK_FILE_NAME,
};
use human_credential_runtime::{CredentialRoute, HumanCredentialRuntime};
use human_terminal_runtime::{
    DisplayContextId, HumanTerminalRuntime, LiveTerminalControl, TERMINAL_CLOSE_SETTLEMENT_TIMEOUT,
    TerminalControlSendError, TerminalRegistryError,
};

#[cfg(debug_assertions)]
use debug_human_terminal_fixture::{
    DebugCredentialSelection, DebugTerminalControlHandle, DebugTerminalLifecycle,
    DebugTerminalOutputError, DebugTerminalOutputSink, DebugTerminalSelection,
    EstablishedDebugTerminal,
};
use resident_lifecycle::{
    BeginQuit, OperationAdmissionError, QuitWaitOutcome, ResidentDrainOutcome,
    ResidentLifecyclePhase, ResidentLifecycleRuntime, ResidentMenuAction, ResidentOperationGuard,
    apply_main_window_close_policy, build_resident_application_menu, handle_resident_menu_event,
    handle_resident_tray_event, install_resident_tray,
};

#[cfg(debug_assertions)]
use std::error::Error;
#[cfg(debug_assertions)]
use vigilshell_core::connection::{
    ConnectionDisplayName, ConnectionSecurityRevision, HostFingerprint,
};

#[cfg(target_os = "macos")]
const SINGLE_INSTANCE_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const SINGLE_INSTANCE_STARTUP_RETRY_INTERVAL: Duration = Duration::from_millis(20);
#[cfg(target_os = "macos")]
const SINGLE_INSTANCE_NOTIFY_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(100);

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct SingleInstanceProcessLock {
    _file: File,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum MacOsSingleInstanceAdmission {
    Primary(SingleInstanceProcessLock),
    SecondaryNotified,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct MacOsSingleInstanceListenerReadiness {
    observed_callbacks: Mutex<u64>,
    changed: Condvar,
}

#[cfg(target_os = "macos")]
impl MacOsSingleInstanceListenerReadiness {
    fn observation(&self) -> std::io::Result<u64> {
        self.observed_callbacks
            .lock()
            .map(|value| *value)
            .map_err(|_| {
                std::io::Error::other("single-instance listener readiness state is unavailable")
            })
    }

    fn mark_callback_observed(&self) {
        if let Ok(mut observed_callbacks) = self.observed_callbacks.lock() {
            *observed_callbacks = observed_callbacks.saturating_add(1);
            self.changed.notify_all();
        }
    }

    fn wait_for_callback_after(&self, observation: u64, timeout: Duration) -> std::io::Result<()> {
        let observed_callbacks = self.observed_callbacks.lock().map_err(|_| {
            std::io::Error::other("single-instance listener readiness state is unavailable")
        })?;
        let (observed_callbacks, wait_result) = self
            .changed
            .wait_timeout_while(observed_callbacks, timeout, |value| *value <= observation)
            .map_err(|_| {
                std::io::Error::other("single-instance listener readiness state is unavailable")
            })?;
        if *observed_callbacks > observation {
            Ok(())
        } else if wait_result.timed_out() {
            Err(single_instance_startup_timeout())
        } else {
            Err(std::io::Error::other(
                "single-instance listener stopped without observing activation",
            ))
        }
    }
}

#[cfg(target_os = "macos")]
impl SingleInstanceProcessLock {
    fn try_acquire(path: &Path) -> Result<Self, SingleInstanceProcessLockError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join(SINGLE_INSTANCE_LOCK_FILE_NAME))
            .map_err(SingleInstanceProcessLockError::Io)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(SingleInstanceProcessLockError::Owned),
            Err(TryLockError::Error(error)) => Err(SingleInstanceProcessLockError::Io(error)),
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum SingleInstanceProcessLockError {
    Owned,
    Io(std::io::Error),
}

#[cfg(target_os = "macos")]
fn admit_macos_primary_instance(
    private_storage_root: &Path,
    identifier: &str,
) -> std::io::Result<MacOsSingleInstanceAdmission> {
    admit_macos_primary_instance_with_timeout(
        private_storage_root,
        identifier,
        SINGLE_INSTANCE_STARTUP_TIMEOUT,
    )
}

#[cfg(target_os = "macos")]
fn admit_macos_primary_instance_with_timeout(
    private_storage_root: &Path,
    identifier: &str,
    timeout: Duration,
) -> std::io::Result<MacOsSingleInstanceAdmission> {
    fs::create_dir_all(private_storage_root)?;
    let socket_path = macos_single_instance_socket_path(identifier);
    let deadline = Instant::now() + timeout;
    loop {
        match SingleInstanceProcessLock::try_acquire(private_storage_root) {
            Ok(lock) => {
                prepare_macos_single_instance_socket(&socket_path)?;
                return Ok(MacOsSingleInstanceAdmission::Primary(lock));
            }
            Err(SingleInstanceProcessLockError::Io(error)) => return Err(error),
            Err(SingleInstanceProcessLockError::Owned) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(single_instance_startup_timeout());
                }
                match notify_macos_primary_instance_with_timeout(
                    &socket_path,
                    remaining.min(SINGLE_INSTANCE_NOTIFY_ATTEMPT_TIMEOUT),
                ) {
                    Ok(()) => return Ok(MacOsSingleInstanceAdmission::SecondaryNotified),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                        ) => {}
                    Err(error) => return Err(error),
                }
                if Instant::now() >= deadline {
                    return Err(single_instance_startup_timeout());
                }
                std::thread::sleep(SINGLE_INSTANCE_STARTUP_RETRY_INTERVAL);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_single_instance_socket_path(identifier: &str) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/{}_si.sock",
        identifier.replace(['.', '-'], "_")
    ))
}

#[cfg(target_os = "macos")]
fn prepare_macos_single_instance_socket(socket_path: &Path) -> std::io::Result<()> {
    match fs::remove_file(socket_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn notify_macos_primary_instance_with_timeout(
    socket_path: &Path,
    timeout: Duration,
) -> std::io::Result<()> {
    let socket_path = socket_path.to_path_buf();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(notify_macos_primary_instance(&socket_path));
    });
    receiver
        .recv_timeout(timeout)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "single-instance activation I/O exceeded its deadline",
            ),
            mpsc::RecvTimeoutError::Disconnected => std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "single-instance activation worker stopped without a result",
            ),
        })?
}

#[cfg(target_os = "macos")]
fn notify_macos_primary_instance(socket_path: &Path) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(b"\0\0")?;
    stream.flush()
}

#[cfg(target_os = "macos")]
fn wait_for_macos_single_instance_listener(
    socket_path: &Path,
    timeout: Duration,
) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(single_instance_startup_timeout());
        }
        match notify_macos_primary_instance_with_timeout(
            socket_path,
            remaining.min(SINGLE_INSTANCE_NOTIFY_ATTEMPT_TIMEOUT),
        ) {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(single_instance_startup_timeout());
        }
        std::thread::sleep(SINGLE_INSTANCE_STARTUP_RETRY_INTERVAL);
    }
}

#[cfg(target_os = "macos")]
fn single_instance_startup_timeout() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "resident Core owner did not become ready",
    )
}

#[cfg(target_os = "macos")]
fn macos_single_instance_readiness_plugin<R: tauri::Runtime>(
    readiness: Arc<MacOsSingleInstanceListenerReadiness>,
) -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("single-instance-readiness")
        .setup(move |app, _api| {
            let deadline = Instant::now() + SINGLE_INSTANCE_STARTUP_TIMEOUT;
            let observation = readiness.observation()?;
            wait_for_macos_single_instance_listener(
                &macos_single_instance_socket_path(&app.config().identifier),
                deadline.saturating_duration_since(Instant::now()),
            )?;
            readiness.wait_for_callback_after(
                observation,
                deadline.saturating_duration_since(Instant::now()),
            )?;
            Ok(())
        })
        .build()
}

struct HostTrustRuntime {
    pending: Mutex<PendingHostTrustDecisions>,
    probe: RusshHostKeyProbe,
}

impl HostTrustRuntime {
    fn production() -> Self {
        Self {
            pending: Mutex::new(PendingHostTrustDecisions::new()),
            probe: RusshHostKeyProbe::production(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "HumanWorkbenchDto.ts")]
struct HumanWorkbenchDto {
    projects: Vec<ProjectDto>,
    servers: Vec<ManagedServerDto>,
    connections: Vec<ConnectionDto>,
    terminal_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "ProjectDto.ts")]
struct ProjectDto {
    project_id: String,
    display_name: String,
    record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "ManagedServerDto.ts")]
struct ManagedServerDto {
    server_id: String,
    project_id: Option<String>,
    display_name: String,
    record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "ConnectionDto.ts")]
struct ConnectionDto {
    profile_id: String,
    server_id: String,
    display_name: String,
    record_version: u64,
    enabled: bool,
    trust_state: TrustStateDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "ConnectionEditorDto.ts")]
struct ConnectionEditorDto {
    profile_id: String,
    server_id: String,
    display_name: String,
    host: String,
    port: u16,
    username: String,
    enabled: bool,
    record_version: u64,
    trust_state: TrustStateDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "TrustStateDto.ts")]
enum TrustStateDto {
    Ready,
    Required,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanTerminalSessionDto {
    session_id: String,
    server_id: String,
    profile_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "IpcErrorCode.ts")]
enum IpcErrorCode {
    StorageUnavailable,
    InvalidRequest,
    ConnectionNotFound,
    ConnectionRecordMismatch,
    ConnectionConflict,
    ConnectionDeleteIncomplete,
    VersionExhausted,
    WorkspaceConflict,
    WorkspaceRecordMismatch,
    ProjectNotFound,
    ManagedServerNotFound,
    ManagedServerNotEmpty,
    CredentialProviderUnavailable,
    CredentialAccessDenied,
    CredentialInvalidSecret,
    ConnectionDisabled,
    ConnectionInUse,
    ConnectionDeleteInProgress,
    HostTrustProbeTimeout,
    HostTrustUnsupportedKey,
    HostTrustProbeUnavailable,
    HostTrustPendingCapacity,
    HostTrustDecisionExpired,
    HostTrustDecisionNotFound,
    HostTrustStateUnavailable,
    HostTrustRequired,
    HostTrustStale,
    CredentialUnavailable,
    TerminalGlobalCapacity,
    TerminalProfileCapacity,
    TerminalHostKeyMismatch,
    TerminalAuthenticationRejected,
    TerminalChannelRejected,
    TerminalPtyRejected,
    TerminalShellRejected,
    TerminalDisconnected,
    TerminalTimeout,
    TerminalUnavailable,
    TerminalOutputUnavailable,
    TerminalControlQueueFull,
    TerminalSessionClosed,
    TerminalCloseTimeout,
    ApplicationShuttingDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcErrorDto {
    code: IpcErrorCode,
}

impl IpcErrorDto {
    const fn storage_unavailable() -> Self {
        Self {
            code: IpcErrorCode::StorageUnavailable,
        }
    }
}

impl From<IpcErrorCode> for IpcErrorDto {
    fn from(code: IpcErrorCode) -> Self {
        Self { code }
    }
}

impl From<CredentialProviderError> for IpcErrorDto {
    fn from(error: CredentialProviderError) -> Self {
        let code = match error {
            CredentialProviderError::AccessDenied => IpcErrorCode::CredentialAccessDenied,
            CredentialProviderError::InvalidSecret => IpcErrorCode::CredentialInvalidSecret,
            CredentialProviderError::Missing
            | CredentialProviderError::Unavailable
            | CredentialProviderError::MainThreadRequired => {
                IpcErrorCode::CredentialProviderUnavailable
            }
        };

        Self { code }
    }
}

impl From<MutateConnectionProfileError> for IpcErrorDto {
    fn from(error: MutateConnectionProfileError) -> Self {
        let code = match error {
            MutateConnectionProfileError::RepositoryUnavailable => IpcErrorCode::StorageUnavailable,
            MutateConnectionProfileError::Conflict => IpcErrorCode::ConnectionConflict,
            MutateConnectionProfileError::NotFound => IpcErrorCode::ConnectionNotFound,
            MutateConnectionProfileError::RecordMismatch => IpcErrorCode::ConnectionRecordMismatch,
            MutateConnectionProfileError::VersionExhausted => IpcErrorCode::VersionExhausted,
        };

        Self { code }
    }
}

impl From<MutateServerWorkspaceError> for IpcErrorDto {
    fn from(error: MutateServerWorkspaceError) -> Self {
        let code = match error {
            MutateServerWorkspaceError::RepositoryUnavailable => IpcErrorCode::StorageUnavailable,
            MutateServerWorkspaceError::Conflict => IpcErrorCode::WorkspaceConflict,
            MutateServerWorkspaceError::ProjectNotFound => IpcErrorCode::ProjectNotFound,
            MutateServerWorkspaceError::ServerNotFound => IpcErrorCode::ManagedServerNotFound,
            MutateServerWorkspaceError::RecordMismatch => IpcErrorCode::WorkspaceRecordMismatch,
            MutateServerWorkspaceError::VersionExhausted => IpcErrorCode::VersionExhausted,
            MutateServerWorkspaceError::ServerNotEmpty => IpcErrorCode::ManagedServerNotEmpty,
        };
        Self { code }
    }
}

fn admit_effectful_operation(
    lifecycle: &ResidentLifecycleRuntime,
) -> Result<ResidentOperationGuard, IpcErrorDto> {
    lifecycle
        .try_admit_operation()
        .map_err(|error| match error {
            OperationAdmissionError::ShuttingDown | OperationAdmissionError::CapacityExhausted => {
                IpcErrorDto::from(IpcErrorCode::ApplicationShuttingDown)
            }
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CredentialActionRequestDto {
    profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetConnectionEditorRequestDto {
    profile_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "CreateConnectionProfileRequestDto.ts")]
struct CreateConnectionProfileRequestDto {
    display_name: String,
    host: String,
    port: i64,
    username: String,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "CreateProjectRequestDto.ts")]
struct CreateProjectRequestDto {
    display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "UpdateProjectRequestDto.ts")]
struct UpdateProjectRequestDto {
    project_id: String,
    expected_record_version: u64,
    display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "DeleteProjectRequestDto.ts")]
struct DeleteProjectRequestDto {
    project_id: String,
    expected_record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "UpdateManagedServerRequestDto.ts")]
struct UpdateManagedServerRequestDto {
    server_id: String,
    expected_record_version: u64,
    project_id: Option<String>,
    display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "DeleteManagedServerRequestDto.ts")]
struct DeleteManagedServerRequestDto {
    server_id: String,
    expected_record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "AddConnectionProfileToServerRequestDto.ts")]
struct AddConnectionProfileToServerRequestDto {
    server_id: String,
    display_name: String,
    host: String,
    port: i64,
    username: String,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateConnectionProfileRequestDto {
    profile_id: String,
    expected_record_version: u64,
    display_name: String,
    host: String,
    port: i64,
    username: String,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteConnectionProfileRequestDto {
    profile_id: String,
    expected_record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeHostKeyRequestDto {
    profile_id: String,
    expected_record_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostTrustDecisionRequestDto {
    probe_id: String,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostTrustProbeResultDto {
    profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    probe_id: Option<String>,
    host_key_type: String,
    observed_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_fingerprint: Option<String>,
    status: HostTrustProbeStatusDto,
}

impl fmt::Debug for HostTrustProbeResultDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostTrustProbeResultDto")
            .field("profile_id", &self.profile_id)
            .field("probe_id", &self.probe_id.as_ref().map(|_| "[opaque]"))
            .field("host_key_type", &self.host_key_type)
            .field("observed_fingerprint", &"[redacted]")
            .field(
                "previous_fingerprint",
                &self.previous_fingerprint.as_ref().map(|_| "[redacted]"),
            )
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HostTrustProbeStatusDto {
    FirstSeen,
    Changed,
    Matches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmHostTrustSettlementDto {
    status: ConfirmHostTrustStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConfirmHostTrustStatusDto {
    Confirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscardHostTrustSettlementDto {
    status: DiscardHostTrustStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DiscardHostTrustStatusDto {
    Discarded,
    AlreadyUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteConnectionProfileSettlementDto {
    status: DeleteConnectionProfileStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DeleteConnectionProfileStatusDto {
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "DeleteProjectSettlementDto.ts")]
struct DeleteProjectSettlementDto {
    status: DeleteProjectStatusDto,
    unassigned_servers: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "DeleteProjectStatusDto.ts")]
enum DeleteProjectStatusDto {
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "DeleteManagedServerSettlementDto.ts")]
struct DeleteManagedServerSettlementDto {
    status: DeleteManagedServerStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "DeleteManagedServerStatusDto.ts")]
enum DeleteManagedServerStatusDto {
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialActionSettlementDto {
    status: CredentialActionStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanPasswordStatusDto {
    status: HumanPasswordStatusValueDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HumanPasswordStatusValueDto {
    Configured,
    NotConfigured,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CredentialActionStatusDto {
    Configured,
    Removed,
    AlreadyMissing,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenHumanTerminalRequestDto {
    profile_id: String,
    columns: u16,
    rows: u16,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HumanTerminalInputRequestDto {
    session_id: String,
    data: Vec<u8>,
}

impl fmt::Debug for HumanTerminalInputRequestDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HumanTerminalInputRequestDto")
            .field("session_id", &self.session_id)
            .field("data_length", &self.data.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HumanTerminalResizeRequestDto {
    session_id: String,
    columns: u16,
    rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HumanTerminalOutputCreditRequestDto {
    session_id: String,
    bytes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HumanTerminalCloseRequestDto {
    session_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HumanTerminalCloseStatusDto {
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanTerminalCloseSettlementDto {
    status: HumanTerminalCloseStatusDto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HumanTerminalExitSignalDto {
    Abort,
    Alarm,
    FloatingPointException,
    Hangup,
    IllegalInstruction,
    Interrupt,
    Kill,
    Pipe,
    Quit,
    SegmentationViolation,
    Terminate,
    UserDefinedOne,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HumanTerminalLifecycleDto {
    Opened {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Eof {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    ExitStatus {
        #[serde(rename = "sessionId")]
        session_id: String,
        status: u32,
    },
    ExitSignal {
        #[serde(rename = "sessionId")]
        session_id: String,
        signal: HumanTerminalExitSignalDto,
        #[serde(rename = "coreDumped")]
        core_dumped: bool,
    },
    Closed {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    Disconnected {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    ProtocolFailure {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
}

#[tauri::command]
fn load_human_workbench(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
) -> Result<HumanWorkbenchDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    load_human_workbench_from(&storage)
}

fn load_human_workbench_from(storage: &SqliteStorage) -> Result<HumanWorkbenchDto, IpcErrorDto> {
    let snapshot = storage
        .load_human_workbench_snapshot()
        .map_err(|_| IpcErrorDto::storage_unavailable())?;
    let (projects, servers, connections) = snapshot.into_parts();
    let projects = projects.iter().map(project_dto).collect();
    let servers = servers.iter().map(managed_server_dto).collect();
    let connections = connections
        .into_iter()
        .map(|snapshot| {
            let (profile, trust) = snapshot.into_parts();
            ConnectionDto {
                profile_id: profile.id().as_str().to_owned(),
                server_id: profile.server_id().as_str().to_owned(),
                display_name: profile.display_name().as_str().to_owned(),
                record_version: profile.record_version().get(),
                enabled: profile.is_enabled(),
                trust_state: trust_state_for(&profile, trust.as_ref()),
            }
        })
        .collect();

    Ok(HumanWorkbenchDto {
        projects,
        servers,
        connections,
        terminal_available: true,
    })
}

fn project_dto(project: &Project) -> ProjectDto {
    ProjectDto {
        project_id: project.id().as_str().to_owned(),
        display_name: project.display_name().as_str().to_owned(),
        record_version: project.record_version().get(),
    }
}

fn managed_server_dto(server: &ManagedServer) -> ManagedServerDto {
    ManagedServerDto {
        server_id: server.id().as_str().to_owned(),
        project_id: server
            .project_id()
            .map(|project_id| project_id.as_str().to_owned()),
        display_name: server.display_name().as_str().to_owned(),
        record_version: server.record_version().get(),
    }
}

fn trust_state_for(profile: &ConnectionProfile, trust: Option<&StoredHostTrust>) -> TrustStateDto {
    match trust {
        Some(trust) if trust.matches(profile) => TrustStateDto::Ready,
        Some(_) => TrustStateDto::Stale,
        None => TrustStateDto::Required,
    }
}

fn connection_editor_dto(
    profile: &ConnectionProfile,
    trust_state: TrustStateDto,
) -> ConnectionEditorDto {
    ConnectionEditorDto {
        profile_id: profile.id().as_str().to_owned(),
        server_id: profile.server_id().as_str().to_owned(),
        display_name: profile.display_name().as_str().to_owned(),
        host: profile.route().host().to_owned(),
        port: profile.route().port(),
        username: profile.route().username().to_owned(),
        enabled: profile.is_enabled(),
        record_version: profile.record_version().get(),
        trust_state,
    }
}

#[tauri::command]
fn get_connection_editor(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: GetConnectionEditorRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    get_connection_editor_from(&storage, request)
}

fn get_connection_editor_from(
    storage: &SqliteStorage,
    request: GetConnectionEditorRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let profile_id = ConnectionProfileId::parse(request.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let snapshot = storage
        .load_connection_editor_snapshot(&profile_id)
        .map_err(|_| IpcErrorDto::storage_unavailable())?
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::ConnectionNotFound))?;
    let (profile, trust) = snapshot.into_parts();
    if profile.id() != &profile_id {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionRecordMismatch));
    }

    let trust_state = trust_state_for(&profile, trust.as_ref());
    Ok(connection_editor_dto(&profile, trust_state))
}

#[tauri::command]
fn create_connection_profile(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: CreateConnectionProfileRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    create_connection_profile_from(
        &storage,
        request,
        ConnectionProfileId::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ManagedServerId::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        CredentialRef::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
    )
}

fn create_connection_profile_from(
    storage: &SqliteStorage,
    request: CreateConnectionProfileRequestDto,
    profile_id: ConnectionProfileId,
    server_id: ManagedServerId,
    credential_ref: CredentialRef,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let server_display_name = request.display_name.clone();
    let request = CreateServerWithConnectionRequest::new(
        server_id,
        None,
        server_display_name,
        profile_id,
        credential_ref,
        request.display_name,
        request.host,
        parse_port(request.port)?,
        request.username,
        request.enabled,
    )
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let created = ServerWorkspaceService::new(storage)
        .create_server_with_connection(request)
        .map_err(IpcErrorDto::from)?;

    Ok(connection_editor_dto(
        created.connection(),
        TrustStateDto::Required,
    ))
}

#[tauri::command]
fn create_project(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: CreateProjectRequestDto,
) -> Result<ProjectDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    create_project_from(
        &storage,
        request,
        ProjectId::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
    )
}

fn create_project_from(
    storage: &SqliteStorage,
    request: CreateProjectRequestDto,
    project_id: ProjectId,
) -> Result<ProjectDto, IpcErrorDto> {
    let request = CreateProjectRequest::new(project_id, request.display_name)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let project = ServerWorkspaceService::new(storage)
        .create_project(request)
        .map_err(IpcErrorDto::from)?;
    Ok(project_dto(&project))
}

#[tauri::command]
fn update_project(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: UpdateProjectRequestDto,
) -> Result<ProjectDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    update_project_from(storage.inner(), request)
}

fn update_project_from(
    storage: &SqliteStorage,
    request: UpdateProjectRequestDto,
) -> Result<ProjectDto, IpcErrorDto> {
    let request = UpdateProjectRequest::new(
        ProjectId::parse(request.project_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ProjectRecordVersion::new(request.expected_record_version)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        request.display_name,
    )
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let project = ServerWorkspaceService::new(storage)
        .update_project(request)
        .map_err(IpcErrorDto::from)?;
    Ok(project_dto(&project))
}

#[tauri::command]
fn delete_project(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: DeleteProjectRequestDto,
) -> Result<DeleteProjectSettlementDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    delete_project_from(storage.inner(), request)
}

fn delete_project_from(
    storage: &SqliteStorage,
    request: DeleteProjectRequestDto,
) -> Result<DeleteProjectSettlementDto, IpcErrorDto> {
    let request = DeleteProjectRequest::new(
        ProjectId::parse(request.project_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ProjectRecordVersion::new(request.expected_record_version)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
    );
    let unassigned_servers = ServerWorkspaceService::new(storage)
        .delete_project(request)
        .map_err(IpcErrorDto::from)?;
    Ok(DeleteProjectSettlementDto {
        status: DeleteProjectStatusDto::Deleted,
        unassigned_servers,
    })
}

#[tauri::command]
fn update_managed_server(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: UpdateManagedServerRequestDto,
) -> Result<ManagedServerDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    update_managed_server_from(storage.inner(), request)
}

fn update_managed_server_from(
    storage: &SqliteStorage,
    request: UpdateManagedServerRequestDto,
) -> Result<ManagedServerDto, IpcErrorDto> {
    let request = UpdateManagedServerRequest::new(
        ManagedServerId::parse(request.server_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ManagedServerRecordVersion::new(request.expected_record_version)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        parse_optional_project_id(request.project_id)?,
        request.display_name,
    )
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let server = ServerWorkspaceService::new(storage)
        .update_server(request)
        .map_err(IpcErrorDto::from)?;
    Ok(managed_server_dto(&server))
}

#[tauri::command]
fn delete_managed_server(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: DeleteManagedServerRequestDto,
) -> Result<DeleteManagedServerSettlementDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    delete_managed_server_from(storage.inner(), request)
}

fn delete_managed_server_from(
    storage: &SqliteStorage,
    request: DeleteManagedServerRequestDto,
) -> Result<DeleteManagedServerSettlementDto, IpcErrorDto> {
    let request = DeleteManagedServerRequest::new(
        ManagedServerId::parse(request.server_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ManagedServerRecordVersion::new(request.expected_record_version)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
    );
    ServerWorkspaceService::new(storage)
        .delete_server(request)
        .map_err(IpcErrorDto::from)?;
    Ok(DeleteManagedServerSettlementDto {
        status: DeleteManagedServerStatusDto::Deleted,
    })
}

#[tauri::command]
fn add_connection_profile_to_server(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: AddConnectionProfileToServerRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    add_connection_profile_to_server_from(
        storage.inner(),
        request,
        ConnectionProfileId::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        CredentialRef::parse(Uuid::new_v4().to_string())
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
    )
}

fn add_connection_profile_to_server_from(
    storage: &SqliteStorage,
    request: AddConnectionProfileToServerRequestDto,
    profile_id: ConnectionProfileId,
    credential_ref: CredentialRef,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let request = AddConnectionToServerRequest::new(
        ManagedServerId::parse(request.server_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        profile_id,
        credential_ref,
        request.display_name,
        request.host,
        parse_port(request.port)?,
        request.username,
        request.enabled,
    )
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = ServerWorkspaceService::new(storage)
        .add_connection_to_server(request)
        .map_err(IpcErrorDto::from)?;
    Ok(connection_editor_dto(&profile, TrustStateDto::Required))
}

fn parse_optional_project_id(value: Option<String>) -> Result<Option<ProjectId>, IpcErrorDto> {
    value
        .map(ProjectId::parse)
        .transpose()
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))
}

#[tauri::command]
fn update_connection_profile(
    storage: State<'_, SqliteStorage>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: UpdateConnectionProfileRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    update_connection_profile_from(&storage, request)
}

fn update_connection_profile_from(
    storage: &SqliteStorage,
    request: UpdateConnectionProfileRequestDto,
) -> Result<ConnectionEditorDto, IpcErrorDto> {
    let request = UpdateConnectionProfileRequest::new(
        ConnectionProfileId::parse(request.profile_id)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        ConnectionRecordVersion::new(request.expected_record_version)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?,
        request.display_name,
        request.host,
        parse_port(request.port)?,
        request.username,
        request.enabled,
    )
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = ConnectionProfileMutationService::new(storage)
        .update(request)
        .map_err(IpcErrorDto::from)?;
    let trust = HostTrustRepository::find(storage, profile.id())
        .map_err(|_| IpcErrorDto::storage_unavailable())?;
    let trust_state = trust_state_for(&profile, trust.as_ref());
    Ok(connection_editor_dto(&profile, trust_state))
}

fn parse_port(value: i64) -> Result<u16, IpcErrorDto> {
    u16::try_from(value)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::InvalidRequest))
}

struct PreparedHostKeyProbe {
    profile_id: ConnectionProfileId,
    expected_record_version: ConnectionRecordVersion,
    profile: ConnectionProfile,
    route: SshRoute,
}

#[tauri::command]
async fn probe_host_key(
    storage: State<'_, SqliteStorage>,
    runtime: State<'_, HostTrustRuntime>,
    credentials: State<'_, HumanCredentialRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: ProbeHostKeyRequestDto,
) -> Result<HostTrustProbeResultDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    let prepared = prepare_host_key_probe(storage.inner(), request)?;
    if !matches!(
        credential_route_for_profile(&prepared.profile, &credentials),
        Ok(CredentialRoute::Production)
    ) {
        return Err(IpcErrorDto::from(IpcErrorCode::HostTrustProbeUnavailable));
    }
    let observed = runtime
        .probe
        .probe(&prepared.route)
        .await
        .map_err(map_host_key_probe_error)?;

    finalize_host_key_probe(
        storage.inner(),
        storage.inner(),
        &runtime.pending,
        prepared,
        observed,
        Instant::now(),
        || {
            HostTrustProbeId::parse(Uuid::new_v4().to_string())
                .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))
        },
    )
}

fn prepare_host_key_probe<Profiles>(
    profiles: &Profiles,
    request: ProbeHostKeyRequestDto,
) -> Result<PreparedHostKeyProbe, IpcErrorDto>
where
    Profiles: ConnectionProfileRepository,
{
    let profile_id = ConnectionProfileId::parse(request.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let expected_record_version = ConnectionRecordVersion::new(request.expected_record_version)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = load_probe_profile(profiles, &profile_id, expected_record_version)?;

    Ok(PreparedHostKeyProbe {
        profile_id,
        expected_record_version,
        profile: profile.clone(),
        route: profile.route().clone(),
    })
}

fn load_probe_profile<Profiles>(
    profiles: &Profiles,
    profile_id: &ConnectionProfileId,
    expected_record_version: ConnectionRecordVersion,
) -> Result<ConnectionProfile, IpcErrorDto>
where
    Profiles: ConnectionProfileRepository,
{
    let profile = profiles
        .find(profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::ConnectionNotFound))?;
    if profile.id() != profile_id {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionRecordMismatch));
    }
    if !profile.is_enabled() {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionDisabled));
    }
    if profile.record_version() != expected_record_version {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionConflict));
    }

    Ok(profile)
}

fn finalize_host_key_probe<Profiles, Trust, NewProbeId>(
    profiles: &Profiles,
    trust: &Trust,
    pending: &Mutex<PendingHostTrustDecisions>,
    prepared: PreparedHostKeyProbe,
    observed: ObservedHostKey,
    now: Instant,
    new_probe_id: NewProbeId,
) -> Result<HostTrustProbeResultDto, IpcErrorDto>
where
    Profiles: ConnectionProfileRepository,
    Trust: HostTrustRepository,
    NewProbeId: FnOnce() -> Result<HostTrustProbeId, IpcErrorDto>,
{
    let profile = load_probe_profile(
        profiles,
        &prepared.profile_id,
        prepared.expected_record_version,
    )?;
    let stored = trust
        .find(&prepared.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?;
    let assessment = HostTrustProbeAssessment::assess(&profile, stored.as_ref(), observed)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?;
    let profile_id = assessment.profile_id().as_str().to_owned();
    let host_key_type = assessment.observed().algorithm().as_str().to_owned();
    let observed_fingerprint = assessment.observed().fingerprint().as_str().to_owned();
    let previous_fingerprint = assessment
        .previous_fingerprint()
        .map(|fingerprint| fingerprint.as_str().to_owned());
    let status = map_probe_status(assessment.status());

    let probe_id = if assessment.status() == HostTrustProbeStatus::Matches {
        None
    } else {
        let probe_id = new_probe_id()?;
        let serialized_probe_id = probe_id.as_str().to_owned();
        let decision = PendingHostTrustDecision::from_assessment(probe_id, assessment)
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?;
        pending
            .lock()
            .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?
            .insert(decision, now)
            .map_err(map_pending_insert_error)?;
        Some(serialized_probe_id)
    };

    Ok(HostTrustProbeResultDto {
        profile_id,
        probe_id,
        host_key_type,
        observed_fingerprint,
        previous_fingerprint,
        status,
    })
}

const fn map_probe_status(status: HostTrustProbeStatus) -> HostTrustProbeStatusDto {
    match status {
        HostTrustProbeStatus::FirstSeen => HostTrustProbeStatusDto::FirstSeen,
        HostTrustProbeStatus::Changed => HostTrustProbeStatusDto::Changed,
        HostTrustProbeStatus::Matches => HostTrustProbeStatusDto::Matches,
    }
}

const fn map_host_key_probe_error(error: HostKeyProbeError) -> IpcErrorDto {
    let code = match error {
        HostKeyProbeError::Timeout => IpcErrorCode::HostTrustProbeTimeout,
        HostKeyProbeError::UnsupportedKeyExchange | HostKeyProbeError::UnsupportedHostKey => {
            IpcErrorCode::HostTrustUnsupportedKey
        }
        HostKeyProbeError::ConnectionUnavailable
        | HostKeyProbeError::ProtocolFailure
        | HostKeyProbeError::ObservationUnavailable
        | HostKeyProbeError::UnexpectedCompletion => IpcErrorCode::HostTrustProbeUnavailable,
    };
    IpcErrorDto { code }
}

const fn map_pending_insert_error(error: PendingHostTrustInsertError) -> IpcErrorDto {
    let code = match error {
        PendingHostTrustInsertError::Capacity => IpcErrorCode::HostTrustPendingCapacity,
        PendingHostTrustInsertError::DuplicateProbeId
        | PendingHostTrustInsertError::ClockOutOfRange => IpcErrorCode::HostTrustStateUnavailable,
    };
    IpcErrorDto { code }
}

#[tauri::command]
fn confirm_host_trust(
    storage: State<'_, SqliteStorage>,
    runtime: State<'_, HostTrustRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: HostTrustDecisionRequestDto,
) -> Result<ConfirmHostTrustSettlementDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    confirm_host_trust_from(
        storage.inner(),
        storage.inner(),
        &runtime.pending,
        request,
        Instant::now(),
    )
}

fn confirm_host_trust_from<Profiles, Trust>(
    profiles: &Profiles,
    trust: &Trust,
    pending: &Mutex<PendingHostTrustDecisions>,
    request: HostTrustDecisionRequestDto,
    now: Instant,
) -> Result<ConfirmHostTrustSettlementDto, IpcErrorDto>
where
    Profiles: ConnectionProfileRepository,
    Trust: HostTrustMutationRepository,
{
    let probe_id = HostTrustProbeId::parse(request.probe_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let consumed = pending
        .lock()
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?
        .consume_for_confirmation(&probe_id, now)
        .map_err(map_pending_lookup_error)?;
    ConfirmHostTrustService::new(profiles, trust)
        .confirm(consumed)
        .map_err(map_confirm_host_trust_error)?;

    Ok(ConfirmHostTrustSettlementDto {
        status: ConfirmHostTrustStatusDto::Confirmed,
    })
}

const fn map_pending_lookup_error(error: PendingHostTrustLookupError) -> IpcErrorDto {
    let code = match error {
        PendingHostTrustLookupError::NotFound => IpcErrorCode::HostTrustDecisionNotFound,
        PendingHostTrustLookupError::Expired => IpcErrorCode::HostTrustDecisionExpired,
    };
    IpcErrorDto { code }
}

const fn map_confirm_host_trust_error(error: ConfirmHostTrustError) -> IpcErrorDto {
    let code = match error {
        ConfirmHostTrustError::DecisionNotFound => IpcErrorCode::HostTrustDecisionNotFound,
        ConfirmHostTrustError::DecisionExpired => IpcErrorCode::HostTrustDecisionExpired,
        ConfirmHostTrustError::ConnectionRepositoryUnavailable
        | ConfirmHostTrustError::ConnectionRecordMismatch
        | ConfirmHostTrustError::StorageUnavailable => IpcErrorCode::HostTrustStateUnavailable,
        ConfirmHostTrustError::ConnectionNotFound => IpcErrorCode::ConnectionNotFound,
        ConfirmHostTrustError::ConnectionDisabled => IpcErrorCode::ConnectionDisabled,
        ConfirmHostTrustError::Stale => IpcErrorCode::HostTrustStale,
    };
    IpcErrorDto { code }
}

#[tauri::command]
fn discard_host_trust(
    runtime: State<'_, HostTrustRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: HostTrustDecisionRequestDto,
) -> Result<DiscardHostTrustSettlementDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    discard_host_trust_from(&runtime.pending, request, Instant::now())
}

fn discard_host_trust_from(
    pending: &Mutex<PendingHostTrustDecisions>,
    request: HostTrustDecisionRequestDto,
    now: Instant,
) -> Result<DiscardHostTrustSettlementDto, IpcErrorDto> {
    let probe_id = HostTrustProbeId::parse(request.probe_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let status = match pending
        .lock()
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::HostTrustStateUnavailable))?
        .discard(&probe_id, now)
    {
        Ok(()) => DiscardHostTrustStatusDto::Discarded,
        Err(PendingHostTrustLookupError::NotFound | PendingHostTrustLookupError::Expired) => {
            DiscardHostTrustStatusDto::AlreadyUnavailable
        }
    };

    Ok(DiscardHostTrustSettlementDto { status })
}

struct PreparedConnectionDelete {
    profile_id: ConnectionProfileId,
    expected_record_version: ConnectionRecordVersion,
    credential_ref: CredentialRef,
}

#[tauri::command]
async fn delete_connection_profile(
    storage: State<'_, SqliteStorage>,
    credentials: State<'_, HumanCredentialRuntime>,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: DeleteConnectionProfileRequestDto,
) -> Result<DeleteConnectionProfileSettlementDto, IpcErrorDto> {
    let operation = admit_effectful_operation(&lifecycle)?;
    let profile_id = ConnectionProfileId::parse(request.profile_id.clone())
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let delete_reservation = terminal_runtime
        .reserve_delete(profile_id.clone())
        .map_err(map_terminal_registry_error)?;
    let (prepared, profile) = prepare_connection_delete_with_id(&storage, request, profile_id)?;
    let credential_route = credential_route_for_profile(&profile, &credentials)?;
    let storage = storage.inner().clone();
    let credentials = credentials.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        let _delete_reservation = delete_reservation;
        settle_connection_delete_with_runtime(&storage, &credentials, credential_route, prepared)
    })
    .await
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::ConnectionDeleteIncomplete))?
}

#[cfg(all(test, debug_assertions))]
fn prepare_connection_delete(
    storage: &SqliteStorage,
    request: DeleteConnectionProfileRequestDto,
) -> Result<PreparedConnectionDelete, IpcErrorDto> {
    let profile_id = ConnectionProfileId::parse(request.profile_id.clone())
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    prepare_connection_delete_with_id(storage, request, profile_id).map(|(prepared, _)| prepared)
}

fn prepare_connection_delete_with_id(
    storage: &SqliteStorage,
    request: DeleteConnectionProfileRequestDto,
    profile_id: ConnectionProfileId,
) -> Result<(PreparedConnectionDelete, ConnectionProfile), IpcErrorDto> {
    let expected_record_version = ConnectionRecordVersion::new(request.expected_record_version)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = ConnectionProfileRepository::find(storage, &profile_id)
        .map_err(|_| IpcErrorDto::storage_unavailable())?
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::ConnectionNotFound))?;
    if profile.id() != &profile_id {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionRecordMismatch));
    }
    if profile.record_version() != expected_record_version {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionConflict));
    }

    Ok((
        PreparedConnectionDelete {
            profile_id,
            expected_record_version,
            credential_ref: profile.credential_ref().clone(),
        },
        profile,
    ))
}

fn settle_connection_delete_with_runtime(
    storage: &SqliteStorage,
    credentials: &HumanCredentialRuntime,
    route: CredentialRoute,
    prepared: PreparedConnectionDelete,
) -> Result<DeleteConnectionProfileSettlementDto, IpcErrorDto> {
    let removal = credentials
        .remove_password_after_confirmation(route, &prepared.credential_ref)
        .map_err(IpcErrorDto::from)?;
    require_confirmed_credential_removal(removal)?;
    finalize_connection_delete(storage, prepared)
}

#[cfg(all(test, debug_assertions))]
fn settle_connection_delete(
    storage: &SqliteStorage,
    credentials: &dyn HumanPasswordCredentialProvider,
    prepared: PreparedConnectionDelete,
) -> Result<DeleteConnectionProfileSettlementDto, IpcErrorDto> {
    let removal = credentials
        .remove_password_after_confirmation(&prepared.credential_ref)
        .map_err(IpcErrorDto::from)?;
    require_confirmed_credential_removal(removal)?;
    finalize_connection_delete(storage, prepared)
}

fn require_confirmed_credential_removal(
    settlement: RemovePasswordSettlement,
) -> Result<(), IpcErrorDto> {
    match settlement {
        RemovePasswordSettlement::Removed | RemovePasswordSettlement::AlreadyMissing => Ok(()),
        RemovePasswordSettlement::Cancelled => Err(IpcErrorDto::from(
            IpcErrorCode::CredentialProviderUnavailable,
        )),
    }
}

fn finalize_connection_delete(
    storage: &SqliteStorage,
    prepared: PreparedConnectionDelete,
) -> Result<DeleteConnectionProfileSettlementDto, IpcErrorDto> {
    let request =
        DeleteConnectionProfileRequest::new(prepared.profile_id, prepared.expected_record_version);
    match ConnectionProfileMutationService::new(storage).delete(request) {
        Ok(()) | Err(MutateConnectionProfileError::NotFound) => {
            Ok(DeleteConnectionProfileSettlementDto {
                status: DeleteConnectionProfileStatusDto::Deleted,
            })
        }
        Err(_) => Err(IpcErrorDto::from(IpcErrorCode::ConnectionDeleteIncomplete)),
    }
}

#[cfg(all(test, debug_assertions))]
fn delete_connection_profile_from(
    storage: &SqliteStorage,
    credentials: &dyn HumanPasswordCredentialProvider,
    request: DeleteConnectionProfileRequestDto,
) -> Result<DeleteConnectionProfileSettlementDto, IpcErrorDto> {
    let prepared = prepare_connection_delete(storage, request)?;
    settle_connection_delete(storage, credentials, prepared)
}

#[tauri::command]
async fn configure_human_password(
    app: AppHandle,
    storage: State<'_, SqliteStorage>,
    credentials: State<'_, HumanCredentialRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: CredentialActionRequestDto,
) -> Result<CredentialActionSettlementDto, IpcErrorDto> {
    let operation = admit_effectful_operation(&lifecycle)?;
    let prepared = prepare_credential_action(&storage, &credentials, request)?;
    let credentials = credentials.inner().clone();
    let settlement = run_credential_action_on_main_thread(&app, move || {
        let _operation = operation;
        credentials.configure_password_on_main_thread(prepared.route, &prepared.credential_ref)
    })
    .await?;

    Ok(configure_settlement_dto(settlement))
}

#[tauri::command]
async fn remove_human_password(
    app: AppHandle,
    storage: State<'_, SqliteStorage>,
    credentials: State<'_, HumanCredentialRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: CredentialActionRequestDto,
) -> Result<CredentialActionSettlementDto, IpcErrorDto> {
    let operation = admit_effectful_operation(&lifecycle)?;
    let prepared = prepare_credential_action(&storage, &credentials, request)?;
    let credentials = credentials.inner().clone();
    let settlement = run_credential_action_on_main_thread(&app, move || {
        let _operation = operation;
        credentials.remove_password_on_main_thread(prepared.route, &prepared.credential_ref)
    })
    .await?;

    Ok(remove_settlement_dto(settlement))
}

#[cfg(all(test, debug_assertions))]
fn credential_ref_for_profile(
    storage: &SqliteStorage,
    request: CredentialActionRequestDto,
) -> Result<CredentialRef, IpcErrorDto> {
    let requested_id = ConnectionProfileId::parse(request.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = ConnectionProfileRepository::find(storage, &requested_id)
        .map_err(|_| IpcErrorDto::storage_unavailable())?
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::ConnectionNotFound))?;
    if profile.id() != &requested_id {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionRecordMismatch));
    }

    Ok(profile.credential_ref().clone())
}

struct PreparedCredentialAction {
    credential_ref: CredentialRef,
    route: CredentialRoute,
}

fn prepare_credential_action(
    storage: &SqliteStorage,
    credentials: &HumanCredentialRuntime,
    request: CredentialActionRequestDto,
) -> Result<PreparedCredentialAction, IpcErrorDto> {
    let requested_id = ConnectionProfileId::parse(request.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let profile = ConnectionProfileRepository::find(storage, &requested_id)
        .map_err(|_| IpcErrorDto::storage_unavailable())?
        .ok_or_else(|| IpcErrorDto::from(IpcErrorCode::ConnectionNotFound))?;
    if profile.id() != &requested_id {
        return Err(IpcErrorDto::from(IpcErrorCode::ConnectionRecordMismatch));
    }
    let route = credential_route_for_profile(&profile, credentials)?;
    Ok(PreparedCredentialAction {
        credential_ref: profile.credential_ref().clone(),
        route,
    })
}

fn credential_route_for_profile(
    profile: &ConnectionProfile,
    credentials: &HumanCredentialRuntime,
) -> Result<CredentialRoute, IpcErrorDto> {
    #[cfg(debug_assertions)]
    {
        match debug_human_terminal_fixture::classify_credential(
            profile,
            credentials.fixture_enabled(),
        ) {
            DebugCredentialSelection::Fixture => Ok(CredentialRoute::Fixture),
            DebugCredentialSelection::Production => Ok(CredentialRoute::Production),
            DebugCredentialSelection::ReservedMismatch => Err(IpcErrorDto::from(
                IpcErrorCode::CredentialProviderUnavailable,
            )),
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = (profile, credentials);
        Ok(CredentialRoute::Production)
    }
}

#[tauri::command]
async fn get_human_password_status(
    storage: State<'_, SqliteStorage>,
    credentials: State<'_, HumanCredentialRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: CredentialActionRequestDto,
) -> Result<HumanPasswordStatusDto, IpcErrorDto> {
    let operation = admit_effectful_operation(&lifecycle)?;
    let prepared = prepare_credential_action(&storage, &credentials, request)?;
    let credentials = credentials.inner().clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        credentials.password_configuration_status(prepared.route, &prepared.credential_ref)
    })
    .await
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::CredentialProviderUnavailable))?
    .map_err(IpcErrorDto::from)?;

    Ok(human_password_status_dto(status))
}

async fn run_credential_action_on_main_thread<T>(
    app: &AppHandle,
    action: impl FnOnce() -> Result<T, CredentialProviderError> + Send + 'static,
) -> Result<T, IpcErrorDto>
where
    T: Send + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = sender.send(action());
    })
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::CredentialProviderUnavailable))?;

    tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::CredentialProviderUnavailable))?
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::CredentialProviderUnavailable))?
        .map_err(IpcErrorDto::from)
}

#[cfg(all(test, debug_assertions))]
fn configure_human_password_from(
    storage: &SqliteStorage,
    credentials: &dyn HumanPasswordCredentialProvider,
    request: CredentialActionRequestDto,
) -> Result<CredentialActionSettlementDto, IpcErrorDto> {
    let credential_ref = credential_ref_for_profile(storage, request)?;
    credentials
        .configure_password_on_main_thread(&credential_ref)
        .map(configure_settlement_dto)
        .map_err(IpcErrorDto::from)
}

#[cfg(all(test, debug_assertions))]
fn remove_human_password_from(
    storage: &SqliteStorage,
    credentials: &dyn HumanPasswordCredentialProvider,
    request: CredentialActionRequestDto,
) -> Result<CredentialActionSettlementDto, IpcErrorDto> {
    let credential_ref = credential_ref_for_profile(storage, request)?;
    credentials
        .remove_password_on_main_thread(&credential_ref)
        .map(remove_settlement_dto)
        .map_err(IpcErrorDto::from)
}

#[cfg(all(test, debug_assertions))]
fn get_human_password_status_from(
    storage: &SqliteStorage,
    credentials: &dyn HumanPasswordCredentialProvider,
    request: CredentialActionRequestDto,
) -> Result<HumanPasswordStatusDto, IpcErrorDto> {
    let credential_ref = credential_ref_for_profile(storage, request)?;
    credentials
        .password_configuration_status(&credential_ref)
        .map(human_password_status_dto)
        .map_err(IpcErrorDto::from)
}

const fn human_password_status_dto(status: PasswordConfigurationStatus) -> HumanPasswordStatusDto {
    let status = match status {
        PasswordConfigurationStatus::Configured => HumanPasswordStatusValueDto::Configured,
        PasswordConfigurationStatus::NotConfigured => HumanPasswordStatusValueDto::NotConfigured,
    };
    HumanPasswordStatusDto { status }
}

const fn configure_settlement_dto(
    settlement: ConfigurePasswordSettlement,
) -> CredentialActionSettlementDto {
    let status = match settlement {
        ConfigurePasswordSettlement::Configured => CredentialActionStatusDto::Configured,
        ConfigurePasswordSettlement::Cancelled => CredentialActionStatusDto::Cancelled,
    };
    CredentialActionSettlementDto { status }
}

const fn remove_settlement_dto(
    settlement: RemovePasswordSettlement,
) -> CredentialActionSettlementDto {
    let status = match settlement {
        RemovePasswordSettlement::Removed => CredentialActionStatusDto::Removed,
        RemovePasswordSettlement::AlreadyMissing => CredentialActionStatusDto::AlreadyMissing,
        RemovePasswordSettlement::Cancelled => CredentialActionStatusDto::Cancelled,
    };
    CredentialActionSettlementDto { status }
}

#[tauri::command]
async fn open_human_terminal(
    webview: Webview,
    storage: State<'_, SqliteStorage>,
    credentials: State<'_, HumanCredentialRuntime>,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: OpenHumanTerminalRequestDto,
    output: Channel<InvokeResponseBody>,
) -> Result<HumanTerminalSessionDto, IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    let profile_id = ConnectionProfileId::parse(request.profile_id)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let terminal_size = TerminalSize::new(request.columns, request.rows)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    let open_reservation = terminal_runtime
        .reserve_open(profile_id.clone())
        .map_err(map_terminal_registry_error)?;
    let prepared = HumanTerminalAdmissionService::new(&*storage, &*storage)
        .prepare(OpenHumanTerminalRequest::new(profile_id, terminal_size))
        .map_err(IpcErrorDto::from)?;
    let terminal_route = classify_human_terminal_route(&prepared, &credentials)?;
    let server_id = prepared.profile().server_id().as_str().to_owned();
    let profile_id = prepared.profile().id().as_str().to_owned();
    let credentials = credentials.inner().clone();
    let (authorized, open_reservation) = tauri::async_runtime::spawn_blocking(move || {
        let broker = credentials.for_human_terminal(terminal_route.credential_route());
        let authorized = prepared.acquire_credential(&broker)?;
        Ok::<_, OpenHumanTerminalError>((authorized, open_reservation))
    })
    .await
    .map_err(|_| IpcErrorDto::from(IpcErrorCode::CredentialUnavailable))?
    .map_err(IpcErrorDto::from)?;

    let session_id = new_human_terminal_session_id()?;
    let session_id_text = session_id.as_str().to_owned();
    let display_context = display_context_for(&webview);
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let sink = TauriTerminalOutputSink {
        channel: output,
        session_id: session_id_text.clone(),
        ready_sender: Some(ready_sender),
    };
    let (control, failed_open_waiter) = match terminal_route {
        HumanTerminalRoute::Production => {
            let established = RusshPasswordTerminalTransport::production()
                .establish(authorized)
                .await
                .map_err(map_terminal_open_error)?;
            let (control, owner) = established.into_owner(sink);
            let control = Arc::new(TauriTerminalControl::Production(control));
            let registration = open_reservation
                .promote(session_id, display_context, control.clone())
                .map_err(map_terminal_registry_error)?;
            let failed_open_waiter = registration.settlement_waiter();
            tauri::async_runtime::spawn(async move {
                let _registration = registration;
                owner.run().await;
            });
            (control, failed_open_waiter)
        }
        #[cfg(debug_assertions)]
        HumanTerminalRoute::Fixture => {
            drop(authorized);
            let (control, owner) = EstablishedDebugTerminal::new().into_owner(sink);
            let control = Arc::new(TauriTerminalControl::Fixture(control));
            let registration = open_reservation
                .promote(session_id, display_context, control.clone())
                .map_err(map_terminal_registry_error)?;
            let failed_open_waiter = registration.settlement_waiter();
            tauri::async_runtime::spawn_blocking(move || {
                let _registration = registration;
                owner.run();
            });
            (control, failed_open_waiter)
        }
    };
    if control.opened_delivery_failed(ready_receiver).await {
        control.close();
        let settled = tauri::async_runtime::spawn_blocking(move || {
            failed_open_waiter.wait(TERMINAL_CLOSE_SETTLEMENT_TIMEOUT)
        })
        .await;
        if !matches!(settled, Ok(Ok(()))) {
            return Err(IpcErrorDto::from(IpcErrorCode::TerminalCloseTimeout));
        }
        return Err(IpcErrorDto::from(IpcErrorCode::TerminalOutputUnavailable));
    }

    Ok(HumanTerminalSessionDto {
        session_id: session_id_text,
        server_id,
        profile_id,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HumanTerminalRoute {
    Production,
    #[cfg(debug_assertions)]
    Fixture,
}

impl HumanTerminalRoute {
    const fn credential_route(self) -> CredentialRoute {
        match self {
            Self::Production => CredentialRoute::Production,
            #[cfg(debug_assertions)]
            Self::Fixture => CredentialRoute::Fixture,
        }
    }
}

fn classify_human_terminal_route(
    prepared: &PreparedHumanTerminalOpen,
    credentials: &HumanCredentialRuntime,
) -> Result<HumanTerminalRoute, IpcErrorDto> {
    #[cfg(debug_assertions)]
    {
        match debug_human_terminal_fixture::classify_terminal(
            prepared,
            credentials.fixture_enabled(),
        ) {
            DebugTerminalSelection::Fixture => Ok(HumanTerminalRoute::Fixture),
            DebugTerminalSelection::Production => Ok(HumanTerminalRoute::Production),
            DebugTerminalSelection::ReservedMismatch => {
                Err(IpcErrorDto::from(IpcErrorCode::TerminalUnavailable))
            }
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = (prepared, credentials);
        Ok(HumanTerminalRoute::Production)
    }
}

#[tauri::command]
fn send_human_terminal_input(
    webview: Webview,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: HumanTerminalInputRequestDto,
) -> Result<(), IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    let session_id = parse_human_terminal_session_id(request.session_id)?;
    terminal_runtime
        .try_input(&session_id, &display_context_for(&webview), request.data)
        .map_err(map_terminal_registry_error)
}

#[tauri::command]
fn resize_human_terminal(
    webview: Webview,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: HumanTerminalResizeRequestDto,
) -> Result<(), IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    let session_id = parse_human_terminal_session_id(request.session_id)?;
    let size = TerminalSize::new(request.columns, request.rows)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    terminal_runtime
        .try_resize(&session_id, &display_context_for(&webview), size)
        .map_err(map_terminal_registry_error)
}

#[tauri::command]
fn grant_human_terminal_output_credit(
    webview: Webview,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    lifecycle: State<'_, ResidentLifecycleRuntime>,
    request: HumanTerminalOutputCreditRequestDto,
) -> Result<(), IpcErrorDto> {
    let _operation = admit_effectful_operation(&lifecycle)?;
    let session_id = parse_human_terminal_session_id(request.session_id)?;
    terminal_runtime
        .try_grant_output_credit(&session_id, &display_context_for(&webview), request.bytes)
        .map_err(map_terminal_registry_error)
}

#[tauri::command]
async fn close_human_terminal(
    webview: Webview,
    terminal_runtime: State<'_, HumanTerminalRuntime>,
    request: HumanTerminalCloseRequestDto,
) -> Result<HumanTerminalCloseSettlementDto, IpcErrorDto> {
    let session_id = parse_human_terminal_session_id(request.session_id)?;
    let waiter = terminal_runtime
        .close(&session_id, &display_context_for(&webview))
        .map_err(map_terminal_registry_error)?;
    tauri::async_runtime::spawn_blocking(move || waiter.wait(TERMINAL_CLOSE_SETTLEMENT_TIMEOUT))
        .await
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::TerminalCloseTimeout))?
        .map_err(map_terminal_registry_error)?;
    Ok(HumanTerminalCloseSettlementDto {
        status: HumanTerminalCloseStatusDto::Closed,
    })
}

impl From<OpenHumanTerminalError> for IpcErrorDto {
    fn from(error: OpenHumanTerminalError) -> Self {
        let code = match error {
            OpenHumanTerminalError::ConnectionRepositoryUnavailable
            | OpenHumanTerminalError::HostTrustRepositoryUnavailable => {
                IpcErrorCode::StorageUnavailable
            }
            OpenHumanTerminalError::ConnectionNotFound => IpcErrorCode::ConnectionNotFound,
            OpenHumanTerminalError::ConnectionRecordMismatch => {
                IpcErrorCode::ConnectionRecordMismatch
            }
            OpenHumanTerminalError::ConnectionDisabled => IpcErrorCode::ConnectionDisabled,
            OpenHumanTerminalError::HostTrustRequired => IpcErrorCode::HostTrustRequired,
            OpenHumanTerminalError::HostTrustStale => IpcErrorCode::HostTrustStale,
            OpenHumanTerminalError::CredentialUnavailable => IpcErrorCode::CredentialUnavailable,
        };

        Self { code }
    }
}

fn display_context_for(webview: &Webview) -> DisplayContextId {
    DisplayContextId::new(webview.window().label(), webview.label())
}

fn new_human_terminal_session_id() -> Result<HumanTerminalSessionId, IpcErrorDto> {
    HumanTerminalSessionId::parse(Uuid::new_v4().hyphenated().to_string())
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::TerminalUnavailable))
}

fn parse_human_terminal_session_id(value: String) -> Result<HumanTerminalSessionId, IpcErrorDto> {
    let uuid =
        Uuid::parse_str(&value).map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))?;
    if uuid.get_version() != Some(Version::Random)
        || uuid.get_variant() != Variant::RFC4122
        || uuid.hyphenated().to_string() != value
    {
        return Err(IpcErrorDto::from(IpcErrorCode::InvalidRequest));
    }
    HumanTerminalSessionId::parse(value)
        .map_err(|_| IpcErrorDto::from(IpcErrorCode::InvalidRequest))
}

fn map_terminal_registry_error(error: TerminalRegistryError) -> IpcErrorDto {
    let code = match error {
        TerminalRegistryError::GlobalCapacityReached => IpcErrorCode::TerminalGlobalCapacity,
        TerminalRegistryError::ProfileCapacityReached => IpcErrorCode::TerminalProfileCapacity,
        TerminalRegistryError::ProfileDeleting => IpcErrorCode::ConnectionDeleteInProgress,
        TerminalRegistryError::ProfileInUse => IpcErrorCode::ConnectionInUse,
        TerminalRegistryError::DeleteAlreadyReserved => IpcErrorCode::ConnectionDeleteInProgress,
        TerminalRegistryError::InputTooLarge | TerminalRegistryError::CreditGrantOutOfRange => {
            IpcErrorCode::InvalidRequest
        }
        TerminalRegistryError::ControlQueueFull => IpcErrorCode::TerminalControlQueueFull,
        TerminalRegistryError::SessionClosed | TerminalRegistryError::SessionNotFound => {
            IpcErrorCode::TerminalSessionClosed
        }
        TerminalRegistryError::CloseSettlementTimeout => IpcErrorCode::TerminalCloseTimeout,
        TerminalRegistryError::ShuttingDown => IpcErrorCode::ApplicationShuttingDown,
        TerminalRegistryError::StateUnavailable
        | TerminalRegistryError::ReservationUnavailable
        | TerminalRegistryError::SessionAlreadyRegistered
        | TerminalRegistryError::DisplayContextMismatch => IpcErrorCode::TerminalUnavailable,
    };
    IpcErrorDto::from(code)
}

const fn map_terminal_open_error(error: RusshTerminalOpenError) -> IpcErrorDto {
    let code = match error {
        RusshTerminalOpenError::HostKeyMismatch => IpcErrorCode::TerminalHostKeyMismatch,
        RusshTerminalOpenError::AuthRejected => IpcErrorCode::TerminalAuthenticationRejected,
        RusshTerminalOpenError::ChannelRejected => IpcErrorCode::TerminalChannelRejected,
        RusshTerminalOpenError::PtyRejected => IpcErrorCode::TerminalPtyRejected,
        RusshTerminalOpenError::ShellRejected => IpcErrorCode::TerminalShellRejected,
        RusshTerminalOpenError::Disconnected => IpcErrorCode::TerminalDisconnected,
        RusshTerminalOpenError::Timeout => IpcErrorCode::TerminalTimeout,
        RusshTerminalOpenError::InvalidCredentialEncoding | RusshTerminalOpenError::Unavailable => {
            IpcErrorCode::TerminalUnavailable
        }
    };
    IpcErrorDto { code }
}

enum TauriTerminalControl {
    Production(RusshTerminalControlHandle),
    #[cfg(debug_assertions)]
    Fixture(DebugTerminalControlHandle),
}

impl TauriTerminalControl {
    async fn opened_delivery_failed(&self, ready_receiver: mpsc::Receiver<Result<(), ()>>) -> bool {
        match tauri::async_runtime::spawn_blocking(move || {
            ready_receiver.recv_timeout(TERMINAL_CLOSE_SETTLEMENT_TIMEOUT)
        })
        .await
        {
            Ok(Ok(Ok(()))) => false,
            Ok(Ok(Err(())) | Err(_)) | Err(_) => true,
        }
    }
}

impl LiveTerminalControl for TauriTerminalControl {
    fn try_input(&self, input: Vec<u8>) -> Result<(), TerminalControlSendError> {
        match self {
            Self::Production(control) => {
                control.try_input(input).map_err(map_terminal_control_error)
            }
            #[cfg(debug_assertions)]
            Self::Fixture(control) => control.try_input(input),
        }
    }

    fn try_resize(&self, size: TerminalSize) -> Result<(), TerminalControlSendError> {
        match self {
            Self::Production(control) => {
                control.try_resize(size).map_err(map_terminal_control_error)
            }
            #[cfg(debug_assertions)]
            Self::Fixture(control) => control.try_resize(size),
        }
    }

    fn try_grant_output_credit(&self, bytes: u32) -> Result<(), TerminalControlSendError> {
        match self {
            Self::Production(control) => control
                .try_grant_credit(bytes)
                .map_err(map_terminal_control_error),
            #[cfg(debug_assertions)]
            Self::Fixture(control) => control.try_grant_credit(bytes),
        }
    }

    fn close(&self) {
        match self {
            Self::Production(control) => control.close(),
            #[cfg(debug_assertions)]
            Self::Fixture(control) => control.close(),
        }
    }
}

const fn map_terminal_control_error(error: RusshTerminalControlError) -> TerminalControlSendError {
    match error {
        RusshTerminalControlError::QueueFull => TerminalControlSendError::QueueFull,
        RusshTerminalControlError::Closed
        | RusshTerminalControlError::InputTooLarge
        | RusshTerminalControlError::CreditOutOfRange => TerminalControlSendError::Closed,
    }
}

struct TauriTerminalOutputSink {
    channel: Channel<InvokeResponseBody>,
    session_id: String,
    ready_sender: Option<mpsc::SyncSender<Result<(), ()>>>,
}

impl TauriTerminalOutputSink {
    fn send_body(&mut self, body: InvokeResponseBody, is_opened: bool) -> Result<(), ()> {
        let result = self.channel.send(body).map_err(|_| ());
        if is_opened && let Some(sender) = self.ready_sender.take() {
            let _ = sender.send(if result.is_ok() { Ok(()) } else { Err(()) });
        }
        result
    }

    fn lifecycle_body(
        &self,
        lifecycle: HumanTerminalLifecycleDto,
    ) -> Result<InvokeResponseBody, ()> {
        serde_json::to_string(&lifecycle)
            .map(InvokeResponseBody::Json)
            .map_err(|_| ())
    }
}

impl RusshTerminalOutputSink for TauriTerminalOutputSink {
    fn send(&mut self, output: RusshTerminalOutput) -> Result<(), RusshTerminalOutputError> {
        let is_ready = matches!(
            output,
            RusshTerminalOutput::Lifecycle(RusshTerminalLifecycle::Ready)
        );
        let body = match output {
            RusshTerminalOutput::Data(data) => InvokeResponseBody::Raw(data.into_bytes()),
            RusshTerminalOutput::Lifecycle(lifecycle) => {
                let lifecycle = lifecycle_dto(&self.session_id, lifecycle);
                self.lifecycle_body(lifecycle)
                    .map_err(|_| RusshTerminalOutputError)?
            }
        };
        self.send_body(body, is_ready)
            .map_err(|_| RusshTerminalOutputError)
    }
}

#[cfg(debug_assertions)]
impl DebugTerminalOutputSink for TauriTerminalOutputSink {
    fn send_data(&mut self, data: Vec<u8>) -> Result<(), DebugTerminalOutputError> {
        self.send_body(InvokeResponseBody::Raw(data), false)
            .map_err(|_| DebugTerminalOutputError)
    }

    fn send_lifecycle(
        &mut self,
        lifecycle: DebugTerminalLifecycle,
    ) -> Result<(), DebugTerminalOutputError> {
        let is_opened = lifecycle == DebugTerminalLifecycle::Opened;
        let lifecycle = match lifecycle {
            DebugTerminalLifecycle::Opened => HumanTerminalLifecycleDto::Opened {
                session_id: self.session_id.clone(),
            },
            DebugTerminalLifecycle::Closed => HumanTerminalLifecycleDto::Closed {
                session_id: self.session_id.clone(),
            },
            DebugTerminalLifecycle::Disconnected => HumanTerminalLifecycleDto::Disconnected {
                session_id: self.session_id.clone(),
            },
            DebugTerminalLifecycle::ProtocolFailure => HumanTerminalLifecycleDto::ProtocolFailure {
                session_id: self.session_id.clone(),
            },
        };
        let body = self
            .lifecycle_body(lifecycle)
            .map_err(|_| DebugTerminalOutputError)?;
        self.send_body(body, is_opened)
            .map_err(|_| DebugTerminalOutputError)
    }
}

impl Drop for TauriTerminalOutputSink {
    fn drop(&mut self) {
        if let Some(sender) = self.ready_sender.take() {
            let _ = sender.send(Err(()));
        }
    }
}

fn lifecycle_dto(session_id: &str, lifecycle: RusshTerminalLifecycle) -> HumanTerminalLifecycleDto {
    let session_id = session_id.to_owned();
    match lifecycle {
        RusshTerminalLifecycle::Ready => HumanTerminalLifecycleDto::Opened { session_id },
        RusshTerminalLifecycle::Eof => HumanTerminalLifecycleDto::Eof { session_id },
        RusshTerminalLifecycle::ExitStatus(status) => {
            HumanTerminalLifecycleDto::ExitStatus { session_id, status }
        }
        RusshTerminalLifecycle::ExitSignal {
            signal,
            core_dumped,
        } => HumanTerminalLifecycleDto::ExitSignal {
            session_id,
            signal: map_terminal_exit_signal(signal),
            core_dumped,
        },
        RusshTerminalLifecycle::Closed => HumanTerminalLifecycleDto::Closed { session_id },
        RusshTerminalLifecycle::Disconnected => {
            HumanTerminalLifecycleDto::Disconnected { session_id }
        }
        RusshTerminalLifecycle::ProtocolFailure => {
            HumanTerminalLifecycleDto::ProtocolFailure { session_id }
        }
    }
}

const fn map_terminal_exit_signal(signal: RusshTerminalExitSignal) -> HumanTerminalExitSignalDto {
    match signal {
        RusshTerminalExitSignal::Abort => HumanTerminalExitSignalDto::Abort,
        RusshTerminalExitSignal::Alarm => HumanTerminalExitSignalDto::Alarm,
        RusshTerminalExitSignal::FloatingPointException => {
            HumanTerminalExitSignalDto::FloatingPointException
        }
        RusshTerminalExitSignal::Hangup => HumanTerminalExitSignalDto::Hangup,
        RusshTerminalExitSignal::IllegalInstruction => {
            HumanTerminalExitSignalDto::IllegalInstruction
        }
        RusshTerminalExitSignal::Interrupt => HumanTerminalExitSignalDto::Interrupt,
        RusshTerminalExitSignal::Kill => HumanTerminalExitSignalDto::Kill,
        RusshTerminalExitSignal::Pipe => HumanTerminalExitSignalDto::Pipe,
        RusshTerminalExitSignal::Quit => HumanTerminalExitSignalDto::Quit,
        RusshTerminalExitSignal::SegmentationViolation => {
            HumanTerminalExitSignalDto::SegmentationViolation
        }
        RusshTerminalExitSignal::Terminate => HumanTerminalExitSignalDto::Terminate,
        RusshTerminalExitSignal::UserDefinedOne => HumanTerminalExitSignalDto::UserDefinedOne,
        RusshTerminalExitSignal::Unknown => HumanTerminalExitSignalDto::Unknown,
    }
}

#[cfg(debug_assertions)]
fn seed_debug_storage(storage: &SqliteStorage) -> Result<(), Box<dyn Error>> {
    let ready = debug_profile(
        "fixture-ready",
        "server-ready",
        1,
        "ready.vigilshell.invalid",
        "operator-ready",
        "credential-ref-ready",
        true,
    )?;
    let stale = debug_profile(
        "fixture-stale",
        "server-stale",
        2,
        "stale.vigilshell.invalid",
        "operator-stale",
        "credential-ref-stale",
        true,
    )?;
    let disabled = debug_profile(
        "fixture-disabled",
        "server-disabled",
        1,
        "disabled.vigilshell.invalid",
        "operator-disabled",
        "credential-ref-disabled",
        false,
    )?;

    for profile in [&ready, &stale, &disabled] {
        storage.upsert_connection_profile_fixture(profile)?;
    }

    storage.upsert_host_trust_fixture(&debug_trust(
        &ready,
        1,
        "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    )?)?;
    storage.upsert_host_trust_fixture(&debug_trust(
        &stale,
        1,
        "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ",
    )?)?;
    storage.upsert_host_trust_fixture(&debug_trust(
        &disabled,
        1,
        "SHA256:CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCg",
    )?)?;

    Ok(())
}

#[cfg(debug_assertions)]
fn debug_profile(
    profile_id: &str,
    server_id: &str,
    security_revision: u64,
    host: &str,
    username: &str,
    credential_ref: &str,
    enabled: bool,
) -> Result<ConnectionProfile, Box<dyn Error>> {
    Ok(ConnectionProfile::new(
        ConnectionProfileId::parse(profile_id)?,
        ManagedServerId::parse(server_id)?,
        ConnectionDisplayName::parse(server_id)?,
        ConnectionRecordVersion::INITIAL,
        ConnectionSecurityRevision::new(security_revision)?,
        SshRoute::new(host, 22, username)?,
        CredentialRef::parse(credential_ref)?,
        enabled,
    ))
}

#[cfg(debug_assertions)]
fn debug_trust(
    profile: &ConnectionProfile,
    security_revision: u64,
    fingerprint: &str,
) -> Result<StoredHostTrust, Box<dyn Error>> {
    Ok(StoredHostTrust::new(
        profile.id().clone(),
        ConnectionSecurityRevision::new(security_revision)?,
        HostFingerprint::parse(fingerprint)?,
    ))
}

fn begin_native_application_quit(
    app: &AppHandle,
    lifecycle: &ResidentLifecycleRuntime,
    terminal_runtime: &HumanTerminalRuntime,
    storage: &SqliteStorage,
) -> TerminationDecision {
    let leader = match lifecycle.begin_quit() {
        BeginQuit::Leader(leader) => leader,
        BeginQuit::Follower(waiter) => {
            let _ = waiter.remaining();
            return TerminationDecision::Cancel;
        }
        BeginQuit::ExitReady(outcome) => {
            let _ = outcome;
            return TerminationDecision::TerminateNow;
        }
    };

    let terminal_waiter = terminal_runtime.begin_quit().ok();
    let app = app.clone();
    let storage = storage.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operations_drained =
            matches!(leader.wait_for_operations(), ResidentDrainOutcome::Drained);
        let terminals_drained = operations_drained
            && terminal_waiter
                .map(|waiter| waiter.wait(leader.remaining()).is_ok())
                .unwrap_or(false);
        let storage_flushed = terminals_drained
            && matches!(
                run_shutdown_step_within(leader.remaining(), move || {
                    storage.flush_for_shutdown()
                }),
                Some(Ok(()))
            );
        let shutdown_succeeded = storage_flushed && !leader.remaining().is_zero();

        let quit_outcome = leader.mark_exit_ready();
        let shutdown_succeeded =
            shutdown_succeeded && matches!(quit_outcome, QuitWaitOutcome::Settled);
        if !shutdown_succeeded {
            app.exit(1);
            return;
        }
        if schedule_native_termination().is_err() {
            app.exit(1);
        }
    });

    TerminationDecision::Cancel
}

fn run_shutdown_step_within<T, F>(timeout: Duration, action: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    if timeout.is_zero() {
        return None;
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("vigilshell-shutdown-step".to_owned())
        .spawn(move || {
            let _ = sender.send(action());
        })
        .ok()?;
    receiver.recv_timeout(timeout).ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let default_data_root = dirs::data_local_dir()
        .map(|path| path.join(&context.config().identifier))
        .expect("failed to resolve the private desktop data root");
    let data_layout = DesktopDataLayout::resolve(default_data_root)
        .expect("failed to establish the private desktop data layout");

    #[cfg(target_os = "macos")]
    let single_instance_admission = {
        admit_macos_primary_instance(data_layout.authority_root(), &context.config().identifier)
            .expect("failed to establish single-instance admission")
    };
    #[cfg(target_os = "macos")]
    let single_instance_lock = match single_instance_admission {
        MacOsSingleInstanceAdmission::Primary(lock) => lock,
        MacOsSingleInstanceAdmission::SecondaryNotified => std::process::exit(0),
    };

    #[cfg(target_os = "macos")]
    data_layout
        .prepare_primary()
        .expect("failed to prepare the primary desktop data layout");

    let builder = tauri::Builder::default()
        .enable_macos_default_menu(false)
        .manage(data_layout);

    #[cfg(target_os = "macos")]
    let builder = builder.manage(single_instance_lock);

    #[cfg(target_os = "macos")]
    let single_instance_listener_readiness =
        Arc::new(MacOsSingleInstanceListenerReadiness::default());

    #[cfg(target_os = "macos")]
    let builder = {
        let readiness = Arc::clone(&single_instance_listener_readiness);
        builder.plugin(tauri_plugin_single_instance::init(
            move |app, _args, _cwd| {
                readiness.mark_callback_observed();
                let _ = resident_lifecycle::handle_single_instance_activation(app);
            },
        ))
    };

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(macos_single_instance_readiness_plugin(
        single_instance_listener_readiness,
    ));

    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let _ = resident_lifecycle::handle_single_instance_activation(app);
    }));

    let builder = builder
        .on_menu_event(|app, event| {
            if matches!(
                handle_resident_menu_event(app, &event),
                Ok(ResidentMenuAction::QuitVigilShell)
            ) {
                let _ = request_native_termination();
            }
        })
        .on_tray_icon_event(|app, event| {
            let _ = handle_resident_tray_event(app, &event);
        })
        .on_web_content_process_terminate(|webview| {
            if let Some(runtime) = webview.try_state::<HumanTerminalRuntime>() {
                runtime.close_all(&display_context_for(webview));
            }
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let _ = apply_main_window_close_policy(window, api);
            }
            WindowEvent::Destroyed => {
                if let Some(runtime) = window.try_state::<HumanTerminalRuntime>() {
                    runtime.close_all_for_window(window.label());
                }
            }
            _ => {}
        })
        .setup(|app| {
            let data_layout = app.state::<DesktopDataLayout>();
            #[cfg(not(target_os = "macos"))]
            data_layout.prepare_primary()?;

            #[cfg(debug_assertions)]
            let data_root_override = std::env::var_os(DESKTOP_DATA_DIR_ENV);
            #[cfg(debug_assertions)]
            let debug_terminal_fixture_enabled = debug_human_terminal_fixture::fixture_enabled(
                std::env::var_os(debug_human_terminal_fixture::DEBUG_TERMINAL_FIXTURE_ENV),
                data_root_override.as_ref(),
            )?;
            let storage = SqliteStorage::new(data_layout.database_path())?;
            data_layout.harden_sqlite_sidecars()?;

            #[cfg(debug_assertions)]
            if data_root_override.is_none() || debug_terminal_fixture_enabled {
                seed_debug_storage(&storage)?;
            }

            let secure_credentials_root = data_layout
                .secure_credentials_root()
                .map(std::path::Path::to_path_buf);
            #[cfg(debug_assertions)]
            let credentials = if debug_terminal_fixture_enabled {
                HumanCredentialRuntime::development(
                    secure_credentials_root,
                    debug_human_terminal_fixture::reserved_credential_refs()?,
                )?
            } else {
                HumanCredentialRuntime::production(secure_credentials_root)?
            };
            #[cfg(not(debug_assertions))]
            let credentials = HumanCredentialRuntime::production(secure_credentials_root)?;

            let terminal_runtime = HumanTerminalRuntime::new();
            let lifecycle = ResidentLifecycleRuntime::new();
            let app_handle = app.handle().clone();
            let quit_lifecycle = lifecycle.clone();
            let quit_terminal_runtime = terminal_runtime.clone();
            let quit_storage = storage.clone();

            app.manage(storage);
            app.manage(credentials);
            app.manage(HostTrustRuntime::production());
            app.manage(terminal_runtime);
            app.manage(lifecycle);
            app.set_menu(build_resident_application_menu(app.handle())?)?;
            install_resident_tray(app.handle())?;
            install_native_termination_hook(move || {
                begin_native_application_quit(
                    &app_handle,
                    &quit_lifecycle,
                    &quit_terminal_runtime,
                    &quit_storage,
                )
            })?;
            Ok(())
        });

    let builder = builder.invoke_handler(tauri::generate_handler![
        load_human_workbench,
        get_connection_editor,
        create_connection_profile,
        create_project,
        update_project,
        delete_project,
        update_managed_server,
        delete_managed_server,
        add_connection_profile_to_server,
        update_connection_profile,
        delete_connection_profile,
        get_human_password_status,
        configure_human_password,
        remove_human_password,
        probe_host_key,
        confirm_host_trust,
        discard_host_trust,
        open_human_terminal,
        send_human_terminal_input,
        resize_human_terminal,
        grant_human_terminal_output_credit,
        close_human_terminal
    ]);

    let app = builder
        .build(context)
        .expect("failed to build the VigilShell desktop application");
    app.run(|app, event| match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            let lifecycle = app.state::<ResidentLifecycleRuntime>();
            if lifecycle.phase() != ResidentLifecyclePhase::ExitReady {
                api.prevent_exit();
                let _ = request_native_termination();
            }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. }
            if app.state::<ResidentLifecycleRuntime>().phase()
                == ResidentLifecyclePhase::Running =>
        {
            let _ = resident_lifecycle::show_and_focus_main_window(app);
        }
        _ => {}
    });
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use std::cell::Cell;

    #[cfg(target_os = "macos")]
    use std::{io::Read, os::unix::net::UnixListener, sync::mpsc, thread};

    use super::*;
    use vigilshell_core::{
        host_trust::{
            HostKeyAlgorithm, HostTrustMutationRepositoryError,
            MAXIMUM_PENDING_HOST_TRUST_DECISIONS, PENDING_HOST_TRUST_TTL, StoreHostTrustResult,
        },
        human_terminal::ConnectionRepositoryError,
    };

    const TEST_FINGERPRINT_A: &str = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const TEST_FINGERPRINT_B: &str = "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ";

    #[cfg(target_os = "macos")]
    #[test]
    fn process_lock_allows_exactly_one_owner() {
        let root = std::env::temp_dir().join(format!(
            "vigilshell-single-instance-lock-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("temporary lock root");

        let first = SingleInstanceProcessLock::try_acquire(&root).expect("first owner");
        assert!(matches!(
            SingleInstanceProcessLock::try_acquire(&root),
            Err(SingleInstanceProcessLockError::Owned)
        ));
        drop(first);
        let second = SingleInstanceProcessLock::try_acquire(&root).expect("released lock");
        drop(second);

        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn secondary_notifies_with_an_empty_protocol_payload() {
        let root = std::env::temp_dir().join(format!(
            "vigilshell-single-instance-secondary-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("temporary lock root");
        let owner = SingleInstanceProcessLock::try_acquire(&root).expect("primary owner");
        let identifier = format!("com.sparkwild.vigilshell.test.{}", Uuid::new_v4());
        let socket_path = macos_single_instance_socket_path(&identifier);
        let listener = UnixListener::bind(&socket_path).expect("test activation listener");
        let (sender, receiver) = mpsc::sync_channel(1);
        let listener_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("secondary activation");
            let mut payload = Vec::new();
            stream
                .read_to_end(&mut payload)
                .expect("activation payload");
            sender.send(payload).expect("return activation payload");
        });

        assert!(matches!(
            admit_macos_primary_instance_with_timeout(
                &root,
                &identifier,
                Duration::from_millis(100)
            ),
            Ok(MacOsSingleInstanceAdmission::SecondaryNotified)
        ));
        assert_eq!(receiver.recv().expect("activation payload"), b"\0\0");

        listener_thread.join().expect("activation listener thread");
        drop(owner);
        fs::remove_file(socket_path).expect("remove test activation socket");
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn admission_recovers_after_the_owner_releases_its_lock() {
        let root = std::env::temp_dir().join(format!(
            "vigilshell-single-instance-recovery-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("temporary lock root");
        let owner = SingleInstanceProcessLock::try_acquire(&root).expect("primary owner");
        let release_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            drop(owner);
        });

        let admission = admit_macos_primary_instance_with_timeout(
            &root,
            &format!("com.sparkwild.vigilshell.test.{}", Uuid::new_v4()),
            Duration::from_millis(250),
        )
        .expect("replacement primary admission");
        assert!(matches!(
            admission,
            MacOsSingleInstanceAdmission::Primary(_)
        ));

        release_thread.join().expect("owner release thread");
        drop(admission);
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn admission_timeout_fails_closed_before_builder_setup() {
        let root = std::env::temp_dir().join(format!(
            "vigilshell-single-instance-timeout-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("temporary lock root");
        let owner = SingleInstanceProcessLock::try_acquire(&root).expect("primary owner");

        let error = admit_macos_primary_instance_with_timeout(
            &root,
            &format!("com.sparkwild.vigilshell.test.{}", Uuid::new_v4()),
            Duration::from_millis(30),
        )
        .expect_err("timeout must fail closed");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);

        drop(owner);
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn admission_io_failure_fails_closed_before_builder_setup() {
        let root =
            std::env::temp_dir().join(format!("vigilshell-single-instance-io-{}", Uuid::new_v4()));
        File::create(&root).expect("blocking file");

        assert!(
            admit_macos_primary_instance(
                &root,
                &format!("com.sparkwild.vigilshell.test.{}", Uuid::new_v4())
            )
            .is_err()
        );

        fs::remove_file(root).expect("remove blocking file");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_single_instance_socket_matches_the_official_plugin_identity() {
        assert_eq!(
            macos_single_instance_socket_path("com.sparkwild.vigilshell"),
            PathBuf::from("/tmp/com_sparkwild_vigilshell_si.sock")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn primary_admission_removes_a_stale_socket_before_builder_setup() {
        let root = std::env::temp_dir().join(format!(
            "vigilshell-single-instance-stale-socket-{}",
            Uuid::new_v4()
        ));
        let identifier = format!("com.sparkwild.vigilshell.test.{}", Uuid::new_v4());
        let socket_path = macos_single_instance_socket_path(&identifier);
        File::create(&socket_path).expect("stale socket placeholder");

        let admission =
            admit_macos_primary_instance(&root, &identifier).expect("primary admission");
        assert!(matches!(
            admission,
            MacOsSingleInstanceAdmission::Primary(_)
        ));
        assert!(!socket_path.exists());

        drop(admission);
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn listener_readiness_timeout_fails_closed_before_window_setup() {
        let socket_path = macos_single_instance_socket_path(&format!(
            "com.sparkwild.vigilshell.test.{}",
            Uuid::new_v4()
        ));

        let error =
            wait_for_macos_single_instance_listener(&socket_path, Duration::from_millis(30))
                .expect_err("missing listener must fail closed");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn listener_readiness_requires_the_official_callback_to_be_observed() {
        let readiness = Arc::new(MacOsSingleInstanceListenerReadiness::default());
        let observation = readiness.observation().expect("initial observation");
        let callback_readiness = Arc::clone(&readiness);
        let callback_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            callback_readiness.mark_callback_observed();
        });

        readiness
            .wait_for_callback_after(observation, Duration::from_millis(200))
            .expect("callback observation");
        callback_thread.join().expect("callback thread");
        assert!(
            readiness
                .wait_for_callback_after(
                    readiness.observation().expect("latest observation"),
                    Duration::from_millis(20),
                )
                .is_err()
        );
    }

    fn test_probe_id(index: usize) -> HostTrustProbeId {
        HostTrustProbeId::parse(format!("00000000-0000-4000-8000-{index:012x}"))
            .expect("valid probe id")
    }

    fn test_observed(fingerprint: &str) -> ObservedHostKey {
        ObservedHostKey::new(
            HostKeyAlgorithm::Ed25519,
            HostFingerprint::parse(fingerprint).expect("valid fingerprint"),
        )
    }

    fn test_pending(profile: &ConnectionProfile, index: usize) -> PendingHostTrustDecision {
        let assessment =
            HostTrustProbeAssessment::assess(profile, None, test_observed(TEST_FINGERPRINT_A))
                .expect("valid assessment");
        PendingHostTrustDecision::from_assessment(test_probe_id(index), assessment)
            .expect("confirmation required")
    }

    fn create_test_connection(storage: &SqliteStorage, profile_id: &str, enabled: bool) {
        create_connection_profile_from(
            storage,
            CreateConnectionProfileRequestDto {
                display_name: format!("Connection {profile_id}"),
                host: format!("{profile_id}.example.invalid"),
                port: 22,
                username: "operator".to_owned(),
                enabled,
            },
            ConnectionProfileId::parse(profile_id).expect("profile id"),
            ManagedServerId::parse(format!("server-{profile_id}")).expect("server id"),
            CredentialRef::parse(format!("credential-{profile_id}")).expect("credential ref"),
        )
        .expect("create connection");
    }

    #[derive(Clone, Copy)]
    enum FakeCredentialOutcome {
        Status(Result<PasswordConfigurationStatus, CredentialProviderError>),
        Configure(Result<ConfigurePasswordSettlement, CredentialProviderError>),
        Remove(Result<RemovePasswordSettlement, CredentialProviderError>),
    }

    struct FakeCredentialProvider {
        expected_ref: String,
        outcome: FakeCredentialOutcome,
    }

    struct FakeProfiles {
        profile: Option<ConnectionProfile>,
        calls: Cell<usize>,
    }

    impl ConnectionProfileRepository for FakeProfiles {
        fn find(
            &self,
            _profile_id: &ConnectionProfileId,
        ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.profile.clone())
        }
    }

    struct FakeTrustMutation {
        outcome: Result<StoreHostTrustResult, HostTrustMutationRepositoryError>,
        calls: Cell<usize>,
    }

    impl HostTrustMutationRepository for FakeTrustMutation {
        fn replace_if_security_revision(
            &self,
            _trust: &StoredHostTrust,
            _expected_security_revision: ConnectionSecurityRevision,
        ) -> Result<StoreHostTrustResult, HostTrustMutationRepositoryError> {
            self.calls.set(self.calls.get() + 1);
            self.outcome
        }
    }

    impl HumanPasswordCredentialProvider for FakeCredentialProvider {
        fn password_configuration_status(
            &self,
            credential_ref: &CredentialRef,
        ) -> Result<PasswordConfigurationStatus, CredentialProviderError> {
            assert_eq!(credential_ref.as_str(), self.expected_ref);
            match self.outcome {
                FakeCredentialOutcome::Status(result) => result,
                FakeCredentialOutcome::Configure(_) | FakeCredentialOutcome::Remove(_) => {
                    panic!("unexpected status query")
                }
            }
        }

        fn configure_password_on_main_thread(
            &self,
            credential_ref: &CredentialRef,
        ) -> Result<ConfigurePasswordSettlement, CredentialProviderError> {
            assert_eq!(credential_ref.as_str(), self.expected_ref);
            match self.outcome {
                FakeCredentialOutcome::Configure(result) => result,
                FakeCredentialOutcome::Status(_) | FakeCredentialOutcome::Remove(_) => {
                    panic!("unexpected configure action")
                }
            }
        }

        fn remove_password_on_main_thread(
            &self,
            credential_ref: &CredentialRef,
        ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
            assert_eq!(credential_ref.as_str(), self.expected_ref);
            match self.outcome {
                FakeCredentialOutcome::Remove(result) => result,
                FakeCredentialOutcome::Status(_) | FakeCredentialOutcome::Configure(_) => {
                    panic!("unexpected remove action")
                }
            }
        }

        fn remove_password_after_confirmation(
            &self,
            credential_ref: &CredentialRef,
        ) -> Result<RemovePasswordSettlement, CredentialProviderError> {
            self.remove_password_on_main_thread(credential_ref)
        }
    }

    #[test]
    fn workbench_lists_stable_safe_connection_and_trust_states() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");

        let workbench = load_human_workbench_from(&storage).expect("load workbench");

        assert!(workbench.terminal_available);
        assert_eq!(workbench.connections.len(), 3);
        assert!(
            workbench
                .connections
                .iter()
                .all(|connection| connection.record_version == 1)
        );
        assert_eq!(
            workbench
                .connections
                .iter()
                .map(|connection| connection.profile_id.as_str())
                .collect::<Vec<_>>(),
            vec!["fixture-disabled", "fixture-ready", "fixture-stale"]
        );
        assert_eq!(
            workbench
                .connections
                .iter()
                .map(|connection| (connection.profile_id.as_str(), connection.trust_state))
                .collect::<Vec<_>>(),
            vec![
                ("fixture-disabled", TrustStateDto::Ready),
                ("fixture-ready", TrustStateDto::Ready),
                ("fixture-stale", TrustStateDto::Stale),
            ]
        );
        assert!(
            !workbench
                .connections
                .iter()
                .find(|connection| connection.profile_id == "fixture-disabled")
                .expect("disabled fixture")
                .enabled
        );
    }

    #[test]
    fn host_key_probe_preflight_rejects_invalid_missing_stale_and_disabled_requests() {
        struct UnavailableProfiles;

        impl ConnectionProfileRepository for UnavailableProfiles {
            fn find(
                &self,
                _profile_id: &ConnectionProfileId,
            ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
                Err(ConnectionRepositoryError::Unavailable)
            }
        }

        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&storage, "probe-ready", true);
        create_test_connection(&storage, "probe-disabled", false);

        for (request, expected) in [
            (
                ProbeHostKeyRequestDto {
                    profile_id: " ".to_owned(),
                    expected_record_version: 1,
                },
                IpcErrorCode::InvalidRequest,
            ),
            (
                ProbeHostKeyRequestDto {
                    profile_id: "probe-ready".to_owned(),
                    expected_record_version: 0,
                },
                IpcErrorCode::InvalidRequest,
            ),
            (
                ProbeHostKeyRequestDto {
                    profile_id: "probe-missing".to_owned(),
                    expected_record_version: 1,
                },
                IpcErrorCode::ConnectionNotFound,
            ),
            (
                ProbeHostKeyRequestDto {
                    profile_id: "probe-ready".to_owned(),
                    expected_record_version: 2,
                },
                IpcErrorCode::ConnectionConflict,
            ),
            (
                ProbeHostKeyRequestDto {
                    profile_id: "probe-disabled".to_owned(),
                    expected_record_version: 1,
                },
                IpcErrorCode::ConnectionDisabled,
            ),
        ] {
            let error = match prepare_host_key_probe(&storage, request) {
                Ok(_) => panic!("invalid probe preflight must fail"),
                Err(error) => error,
            };
            assert_eq!(error.code, expected);
        }

        let unavailable = match prepare_host_key_probe(
            &UnavailableProfiles,
            ProbeHostKeyRequestDto {
                profile_id: "probe-ready".to_owned(),
                expected_record_version: 1,
            },
        ) {
            Ok(_) => panic!("unavailable profile state must fail"),
            Err(error) => error,
        };
        assert_eq!(unavailable.code, IpcErrorCode::HostTrustStateUnavailable);
    }

    #[test]
    fn host_key_probe_postflight_reloads_and_rejects_record_changes() {
        for (profile_id, postflight_change, expected) in [
            ("probe-renamed", "renamed", IpcErrorCode::ConnectionConflict),
            (
                "probe-disabled-after",
                "disabled",
                IpcErrorCode::ConnectionDisabled,
            ),
            (
                "probe-deleted-after",
                "deleted",
                IpcErrorCode::ConnectionNotFound,
            ),
        ] {
            let storage = SqliteStorage::open_in_memory().expect("open storage");
            create_test_connection(&storage, profile_id, true);
            let prepared = prepare_host_key_probe(
                &storage,
                ProbeHostKeyRequestDto {
                    profile_id: profile_id.to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare probe");

            match postflight_change {
                "renamed" | "disabled" => {
                    update_connection_profile_from(
                        &storage,
                        UpdateConnectionProfileRequestDto {
                            profile_id: profile_id.to_owned(),
                            expected_record_version: 1,
                            display_name: if postflight_change == "renamed" {
                                "Renamed during probe".to_owned()
                            } else {
                                format!("Connection {profile_id}")
                            },
                            host: format!("{profile_id}.example.invalid"),
                            port: 22,
                            username: "operator".to_owned(),
                            enabled: postflight_change != "disabled",
                        },
                    )
                    .expect("update connection during probe");
                }
                "deleted" => ConnectionProfileMutationService::new(&storage)
                    .delete(DeleteConnectionProfileRequest::new(
                        ConnectionProfileId::parse(profile_id).expect("profile id"),
                        ConnectionRecordVersion::INITIAL,
                    ))
                    .expect("delete connection during probe"),
                _ => unreachable!("known postflight change"),
            }

            let error = finalize_host_key_probe(
                &storage,
                &storage,
                &Mutex::new(PendingHostTrustDecisions::new()),
                prepared,
                test_observed(TEST_FINGERPRINT_A),
                Instant::now(),
                || Ok(test_probe_id(1)),
            )
            .expect_err("postflight mutation must invalidate probe");
            assert_eq!(error.code, expected);
        }
    }

    #[test]
    fn host_key_probe_matches_without_pending_and_serializes_exact_fields() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&storage, "probe-matches", true);
        let profile_id = ConnectionProfileId::parse("probe-matches").expect("profile id");
        storage
            .upsert_host_trust_fixture(&StoredHostTrust::new(
                profile_id,
                ConnectionSecurityRevision::INITIAL,
                HostFingerprint::parse(TEST_FINGERPRINT_A).expect("fingerprint"),
            ))
            .expect("store trust");
        let prepared = prepare_host_key_probe(
            &storage,
            ProbeHostKeyRequestDto {
                profile_id: "probe-matches".to_owned(),
                expected_record_version: 1,
            },
        )
        .expect("prepare probe");
        let pending = Mutex::new(PendingHostTrustDecisions::new());

        let result = finalize_host_key_probe(
            &storage,
            &storage,
            &pending,
            prepared,
            test_observed(TEST_FINGERPRINT_A),
            Instant::now(),
            || panic!("matching trust must not generate a probe id"),
        )
        .expect("matching probe");

        assert_eq!(result.status, HostTrustProbeStatusDto::Matches);
        assert_eq!(result.probe_id, None);
        assert!(pending.lock().expect("pending registry").is_empty());
        assert_eq!(
            serde_json::to_string(&result).expect("serialize result"),
            format!(
                r#"{{"profileId":"probe-matches","hostKeyType":"ssh-ed25519","observedFingerprint":"{TEST_FINGERPRINT_A}","previousFingerprint":"{TEST_FINGERPRINT_A}","status":"matches"}}"#
            )
        );
    }

    #[test]
    fn host_key_probe_first_seen_and_changed_return_exact_confirmation_dtos() {
        let first_storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&first_storage, "probe-first", true);
        let first = finalize_host_key_probe(
            &first_storage,
            &first_storage,
            &Mutex::new(PendingHostTrustDecisions::new()),
            prepare_host_key_probe(
                &first_storage,
                ProbeHostKeyRequestDto {
                    profile_id: "probe-first".to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare first probe"),
            test_observed(TEST_FINGERPRINT_A),
            Instant::now(),
            || Ok(test_probe_id(1)),
        )
        .expect("first-seen probe");
        assert_eq!(
            serde_json::to_string(&first).expect("serialize first result"),
            format!(
                r#"{{"profileId":"probe-first","probeId":"{}","hostKeyType":"ssh-ed25519","observedFingerprint":"{TEST_FINGERPRINT_A}","status":"first_seen"}}"#,
                test_probe_id(1).as_str()
            )
        );

        let changed_storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&changed_storage, "probe-changed", true);
        let changed_profile_id = ConnectionProfileId::parse("probe-changed").expect("profile id");
        changed_storage
            .upsert_host_trust_fixture(&StoredHostTrust::new(
                changed_profile_id,
                ConnectionSecurityRevision::INITIAL,
                HostFingerprint::parse(TEST_FINGERPRINT_A).expect("fingerprint"),
            ))
            .expect("store trust");
        let changed = finalize_host_key_probe(
            &changed_storage,
            &changed_storage,
            &Mutex::new(PendingHostTrustDecisions::new()),
            prepare_host_key_probe(
                &changed_storage,
                ProbeHostKeyRequestDto {
                    profile_id: "probe-changed".to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare changed probe"),
            test_observed(TEST_FINGERPRINT_B),
            Instant::now(),
            || Ok(test_probe_id(2)),
        )
        .expect("changed probe");
        assert_eq!(
            serde_json::to_string(&changed).expect("serialize changed result"),
            format!(
                r#"{{"profileId":"probe-changed","probeId":"{}","hostKeyType":"ssh-ed25519","observedFingerprint":"{TEST_FINGERPRINT_B}","previousFingerprint":"{TEST_FINGERPRINT_A}","status":"changed"}}"#,
                test_probe_id(2).as_str()
            )
        );

        let debug = format!("{changed:?}");
        assert!(!debug.contains(test_probe_id(2).as_str()));
        assert!(!debug.contains(TEST_FINGERPRINT_A));
        assert!(!debug.contains(TEST_FINGERPRINT_B));
    }

    #[test]
    fn host_key_probe_replaces_same_profile_pending_and_maps_global_capacity() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&storage, "probe-replacement", true);
        let profile = ConnectionProfileRepository::find(
            &storage,
            &ConnectionProfileId::parse("probe-replacement").expect("profile id"),
        )
        .expect("read profile")
        .expect("stored profile");
        let now = Instant::now();
        let pending = Mutex::new(PendingHostTrustDecisions::new());
        pending
            .lock()
            .expect("pending registry")
            .insert(test_pending(&profile, 20), now)
            .expect("insert first pending");

        let replacement = finalize_host_key_probe(
            &storage,
            &storage,
            &pending,
            prepare_host_key_probe(
                &storage,
                ProbeHostKeyRequestDto {
                    profile_id: "probe-replacement".to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare replacement probe"),
            test_observed(TEST_FINGERPRINT_A),
            now,
            || Ok(test_probe_id(21)),
        )
        .expect("replace same-profile pending");
        assert_eq!(
            replacement.probe_id.as_deref(),
            Some(test_probe_id(21).as_str())
        );
        assert_eq!(pending.lock().expect("pending registry").len(), 1);
        assert_eq!(
            discard_host_trust_from(
                &pending,
                HostTrustDecisionRequestDto {
                    probe_id: test_probe_id(20).as_str().to_owned(),
                },
                now,
            )
            .expect("old pending is unavailable")
            .status,
            DiscardHostTrustStatusDto::AlreadyUnavailable
        );
        assert_eq!(
            discard_host_trust_from(
                &pending,
                HostTrustDecisionRequestDto {
                    probe_id: test_probe_id(21).as_str().to_owned(),
                },
                now,
            )
            .expect("discard replacement")
            .status,
            DiscardHostTrustStatusDto::Discarded
        );

        let full_pending = Mutex::new(PendingHostTrustDecisions::new());
        {
            let mut registry = full_pending.lock().expect("pending registry");
            for index in 0..MAXIMUM_PENDING_HOST_TRUST_DECISIONS {
                let profile_id = format!("capacity-{index}");
                let server_id = format!("server-capacity-{index}");
                let host = format!("capacity-{index}.example.invalid");
                let credential_ref = format!("credential-capacity-{index}");
                let profile = debug_profile(
                    &profile_id,
                    &server_id,
                    1,
                    &host,
                    "operator",
                    &credential_ref,
                    true,
                )
                .expect("capacity profile");
                registry
                    .insert(test_pending(&profile, 100 + index), now)
                    .expect("fill pending capacity");
            }
        }
        create_test_connection(&storage, "probe-capacity", true);
        let capacity = finalize_host_key_probe(
            &storage,
            &storage,
            &full_pending,
            prepare_host_key_probe(
                &storage,
                ProbeHostKeyRequestDto {
                    profile_id: "probe-capacity".to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare capacity probe"),
            test_observed(TEST_FINGERPRINT_A),
            now,
            || Ok(test_probe_id(999)),
        )
        .expect_err("new profile must respect pending capacity");
        assert_eq!(capacity.code, IpcErrorCode::HostTrustPendingCapacity);
    }

    #[test]
    fn host_trust_confirmation_persists_and_is_single_use() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_test_connection(&storage, "confirm-success", true);
        let now = Instant::now();
        let pending = Mutex::new(PendingHostTrustDecisions::new());
        let probe_id = test_probe_id(3);
        let result = finalize_host_key_probe(
            &storage,
            &storage,
            &pending,
            prepare_host_key_probe(
                &storage,
                ProbeHostKeyRequestDto {
                    profile_id: "confirm-success".to_owned(),
                    expected_record_version: 1,
                },
            )
            .expect("prepare confirmation probe"),
            test_observed(TEST_FINGERPRINT_A),
            now,
            || Ok(probe_id.clone()),
        )
        .expect("probe requires confirmation");
        assert_eq!(result.probe_id.as_deref(), Some(probe_id.as_str()));

        let settlement = confirm_host_trust_from(
            &storage,
            &storage,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: probe_id.as_str().to_owned(),
            },
            now,
        )
        .expect("confirm trust");
        assert_eq!(
            serde_json::to_string(&settlement).expect("serialize confirmation"),
            r#"{"status":"confirmed"}"#
        );
        let stored = HostTrustRepository::find(
            &storage,
            &ConnectionProfileId::parse("confirm-success").expect("profile id"),
        )
        .expect("read trust")
        .expect("stored trust");
        assert_eq!(stored.fingerprint().as_str(), TEST_FINGERPRINT_A);

        let replay = confirm_host_trust_from(
            &storage,
            &storage,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: probe_id.as_str().to_owned(),
            },
            now,
        )
        .expect_err("confirmation replay must fail");
        assert_eq!(replay.code, IpcErrorCode::HostTrustDecisionNotFound);
    }

    #[test]
    fn host_trust_confirmation_consumes_before_storage_failure() {
        let profile = debug_profile(
            "confirm-storage-failure",
            "server-confirm-storage-failure",
            1,
            "confirm-storage-failure.example.invalid",
            "operator",
            "credential-confirm-storage-failure",
            true,
        )
        .expect("profile");
        let profiles = FakeProfiles {
            profile: Some(profile.clone()),
            calls: Cell::new(0),
        };
        let mutation = FakeTrustMutation {
            outcome: Err(HostTrustMutationRepositoryError::Unavailable),
            calls: Cell::new(0),
        };
        let now = Instant::now();
        let pending = Mutex::new(PendingHostTrustDecisions::new());
        pending
            .lock()
            .expect("pending registry")
            .insert(test_pending(&profile, 4), now)
            .expect("insert pending");

        let failure = confirm_host_trust_from(
            &profiles,
            &mutation,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(4).as_str().to_owned(),
            },
            now,
        )
        .expect_err("storage failure must fail closed");
        assert_eq!(failure.code, IpcErrorCode::HostTrustStateUnavailable);
        assert_eq!(profiles.calls.get(), 1);
        assert_eq!(mutation.calls.get(), 1);

        let replay = confirm_host_trust_from(
            &profiles,
            &mutation,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(4).as_str().to_owned(),
            },
            now,
        )
        .expect_err("failed confirmation must remain consumed");
        assert_eq!(replay.code, IpcErrorCode::HostTrustDecisionNotFound);
        assert_eq!(profiles.calls.get(), 1);
        assert_eq!(mutation.calls.get(), 1);
    }

    #[test]
    fn host_trust_confirmation_releases_registry_lock_before_repository_access() {
        struct LockCheckingProfiles<'a> {
            profile: ConnectionProfile,
            pending: &'a Mutex<PendingHostTrustDecisions>,
            observed_unlocked: Cell<bool>,
        }

        impl ConnectionProfileRepository for LockCheckingProfiles<'_> {
            fn find(
                &self,
                _profile_id: &ConnectionProfileId,
            ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
                let unlocked = self.pending.try_lock().is_ok();
                self.observed_unlocked.set(unlocked);
                Ok(Some(self.profile.clone()))
            }
        }

        let profile = debug_profile(
            "confirm-unlocked",
            "server-confirm-unlocked",
            1,
            "confirm-unlocked.example.invalid",
            "operator",
            "credential-confirm-unlocked",
            true,
        )
        .expect("profile");
        let now = Instant::now();
        let pending = Mutex::new(PendingHostTrustDecisions::new());
        pending
            .lock()
            .expect("pending registry")
            .insert(test_pending(&profile, 7), now)
            .expect("insert pending");
        let profiles = LockCheckingProfiles {
            profile,
            pending: &pending,
            observed_unlocked: Cell::new(false),
        };
        let mutation = FakeTrustMutation {
            outcome: Ok(StoreHostTrustResult::Stored),
            calls: Cell::new(0),
        };

        confirm_host_trust_from(
            &profiles,
            &mutation,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(7).as_str().to_owned(),
            },
            now,
        )
        .expect("confirm trust");

        assert!(profiles.observed_unlocked.get());
        assert_eq!(mutation.calls.get(), 1);
    }

    #[test]
    fn host_trust_expiry_and_discard_have_stable_settlements() {
        let profile = debug_profile(
            "decision-lifecycle",
            "server-decision-lifecycle",
            1,
            "decision-lifecycle.example.invalid",
            "operator",
            "credential-decision-lifecycle",
            true,
        )
        .expect("profile");
        let profiles = FakeProfiles {
            profile: Some(profile.clone()),
            calls: Cell::new(0),
        };
        let mutation = FakeTrustMutation {
            outcome: Ok(StoreHostTrustResult::Stored),
            calls: Cell::new(0),
        };
        let now = Instant::now();
        let pending = Mutex::new(PendingHostTrustDecisions::new());
        pending
            .lock()
            .expect("pending registry")
            .insert(test_pending(&profile, 5), now)
            .expect("insert expiring pending");
        let expired = confirm_host_trust_from(
            &profiles,
            &mutation,
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(5).as_str().to_owned(),
            },
            now.checked_add(PENDING_HOST_TRUST_TTL)
                .expect("expiry time"),
        )
        .expect_err("expired decision must fail");
        assert_eq!(expired.code, IpcErrorCode::HostTrustDecisionExpired);
        assert_eq!(profiles.calls.get(), 0);
        assert_eq!(mutation.calls.get(), 0);

        pending
            .lock()
            .expect("pending registry")
            .insert(test_pending(&profile, 6), now)
            .expect("insert discardable pending");
        let discarded = discard_host_trust_from(
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(6).as_str().to_owned(),
            },
            now,
        )
        .expect("discard pending");
        assert_eq!(
            serde_json::to_string(&discarded).expect("serialize discard"),
            r#"{"status":"discarded"}"#
        );
        let replay = discard_host_trust_from(
            &pending,
            HostTrustDecisionRequestDto {
                probe_id: test_probe_id(6).as_str().to_owned(),
            },
            now,
        )
        .expect("discard replay is idempotent");
        assert_eq!(replay.status, DiscardHostTrustStatusDto::AlreadyUnavailable);
    }

    #[test]
    fn host_trust_error_codes_are_exact_and_renderer_safe() {
        for (error, expected) in [
            (
                map_host_key_probe_error(HostKeyProbeError::Timeout),
                r#"{"code":"host_trust_probe_timeout"}"#,
            ),
            (
                map_host_key_probe_error(HostKeyProbeError::UnsupportedHostKey),
                r#"{"code":"host_trust_unsupported_key"}"#,
            ),
            (
                map_host_key_probe_error(HostKeyProbeError::UnsupportedKeyExchange),
                r#"{"code":"host_trust_unsupported_key"}"#,
            ),
            (
                map_host_key_probe_error(HostKeyProbeError::ConnectionUnavailable),
                r#"{"code":"host_trust_probe_unavailable"}"#,
            ),
            (
                map_pending_insert_error(PendingHostTrustInsertError::Capacity),
                r#"{"code":"host_trust_pending_capacity"}"#,
            ),
            (
                map_confirm_host_trust_error(ConfirmHostTrustError::DecisionExpired),
                r#"{"code":"host_trust_decision_expired"}"#,
            ),
            (
                map_confirm_host_trust_error(ConfirmHostTrustError::DecisionNotFound),
                r#"{"code":"host_trust_decision_not_found"}"#,
            ),
            (
                map_confirm_host_trust_error(ConfirmHostTrustError::StorageUnavailable),
                r#"{"code":"host_trust_state_unavailable"}"#,
            ),
        ] {
            let serialized = serde_json::to_string(&error).expect("serialize IPC error");
            assert_eq!(serialized, expected);
            for forbidden in [
                ".invalid",
                "operator",
                "credential-",
                "securityRevision",
                "SHA256:",
                "russh",
            ] {
                assert!(!serialized.contains(forbidden));
            }
        }
    }

    #[test]
    fn connection_create_and_editor_use_native_identities_and_safe_projections() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let editor = create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "  Production edge  ".to_owned(),
                host: "edge.example.invalid".to_owned(),
                port: 2222,
                username: "operator".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-native").expect("profile id"),
            ManagedServerId::parse("server-native").expect("server id"),
            CredentialRef::parse("credential-native").expect("credential ref"),
        )
        .expect("create connection");

        assert_eq!(editor.profile_id, "profile-native");
        assert_eq!(editor.server_id, "server-native");
        assert_eq!(editor.display_name, "Production edge");
        assert_eq!(editor.host, "edge.example.invalid");
        assert_eq!(editor.port, 2222);
        assert_eq!(editor.username, "operator");
        assert_eq!(editor.record_version, 1);
        assert_eq!(editor.trust_state, TrustStateDto::Required);

        let loaded = get_connection_editor_from(
            &storage,
            GetConnectionEditorRequestDto {
                profile_id: editor.profile_id.clone(),
            },
        )
        .expect("load editor");
        assert_eq!(loaded, editor);

        let catalogue = load_human_workbench_from(&storage).expect("load catalogue");
        let catalogue_json = serde_json::to_string(&catalogue).expect("serialize catalogue");
        assert!(catalogue_json.contains("Production edge"));
        assert!(!catalogue_json.contains("edge.example.invalid"));
        assert!(!catalogue_json.contains("operator"));
        assert!(!catalogue_json.contains("credential-native"));
        assert!(!catalogue_json.contains("securityRevision"));

        let editor_json = serde_json::to_string(&editor).expect("serialize editor");
        assert!(editor_json.contains("edge.example.invalid"));
        assert!(editor_json.contains("operator"));
        assert!(!editor_json.contains("credential-native"));
        assert!(!editor_json.contains("securityRevision"));
    }

    #[test]
    fn new_connection_settlements_do_not_depend_on_a_post_commit_trust_read() {
        let create_storage = SqliteStorage::open_in_memory().expect("open create storage");
        create_storage
            .make_host_trust_reads_unavailable_fixture()
            .expect("make trust reads unavailable");
        let created = create_connection_profile_from(
            &create_storage,
            CreateConnectionProfileRequestDto {
                display_name: "Created route".to_owned(),
                host: "created.example.invalid".to_owned(),
                port: 22,
                username: "created-user".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-created-settlement").expect("profile id"),
            ManagedServerId::parse("server-created-settlement").expect("server id"),
            CredentialRef::parse("credential-created-settlement").expect("credential ref"),
        )
        .expect("settle committed create");
        assert_eq!(created.trust_state, TrustStateDto::Required);
        assert!(
            ConnectionProfileRepository::find(
                &create_storage,
                &ConnectionProfileId::parse(&created.profile_id).expect("profile id"),
            )
            .expect("read committed create")
            .is_some()
        );
        assert_eq!(
            get_connection_editor_from(
                &create_storage,
                GetConnectionEditorRequestDto {
                    profile_id: created.profile_id,
                },
            )
            .expect_err("editor trust read must remain unavailable")
            .code,
            IpcErrorCode::StorageUnavailable
        );

        let add_storage = SqliteStorage::open_in_memory().expect("open add storage");
        let first = create_connection_profile_from(
            &add_storage,
            CreateConnectionProfileRequestDto {
                display_name: "First route".to_owned(),
                host: "first.example.invalid".to_owned(),
                port: 22,
                username: "first-user".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-first-settlement").expect("profile id"),
            ManagedServerId::parse("server-add-settlement").expect("server id"),
            CredentialRef::parse("credential-first-settlement").expect("credential ref"),
        )
        .expect("create first profile");
        add_storage
            .make_host_trust_reads_unavailable_fixture()
            .expect("make trust reads unavailable");
        let added = add_connection_profile_to_server_from(
            &add_storage,
            AddConnectionProfileToServerRequestDto {
                server_id: first.server_id,
                display_name: "Added route".to_owned(),
                host: "added.example.invalid".to_owned(),
                port: 2222,
                username: "added-user".to_owned(),
                enabled: false,
            },
            ConnectionProfileId::parse("profile-added-settlement").expect("profile id"),
            CredentialRef::parse("credential-added-settlement").expect("credential ref"),
        )
        .expect("settle committed add");
        assert_eq!(added.trust_state, TrustStateDto::Required);
        assert!(
            ConnectionProfileRepository::find(
                &add_storage,
                &ConnectionProfileId::parse(&added.profile_id).expect("profile id"),
            )
            .expect("read committed add")
            .is_some()
        );
        assert_eq!(
            get_connection_editor_from(
                &add_storage,
                GetConnectionEditorRequestDto {
                    profile_id: added.profile_id,
                },
            )
            .expect_err("editor trust read must remain unavailable")
            .code,
            IpcErrorCode::StorageUnavailable
        );
    }

    #[test]
    fn project_server_and_second_connection_commands_preserve_profile_scoped_authority() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let project = create_project_from(
            &storage,
            CreateProjectRequestDto {
                display_name: "  Production  ".to_owned(),
            },
            ProjectId::parse("project-production").expect("project id"),
        )
        .expect("create project");
        let first = create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "Primary route".to_owned(),
                host: "primary.example.invalid".to_owned(),
                port: 22,
                username: "primary-user".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-primary").expect("profile id"),
            ManagedServerId::parse("server-primary").expect("server id"),
            CredentialRef::parse("credential-primary").expect("credential ref"),
        )
        .expect("create server and first connection");
        let server = update_managed_server_from(
            &storage,
            UpdateManagedServerRequestDto {
                server_id: first.server_id.clone(),
                expected_record_version: 1,
                project_id: Some(project.project_id.clone()),
                display_name: "Production server".to_owned(),
            },
        )
        .expect("assign and rename server");
        assert_eq!(server.record_version, 2);
        assert_eq!(server.project_id.as_deref(), Some("project-production"));

        let second = add_connection_profile_to_server_from(
            &storage,
            AddConnectionProfileToServerRequestDto {
                server_id: server.server_id.clone(),
                display_name: "Secondary route".to_owned(),
                host: "secondary.example.invalid".to_owned(),
                port: 2222,
                username: "secondary-user".to_owned(),
                enabled: false,
            },
            ConnectionProfileId::parse("profile-secondary").expect("profile id"),
            CredentialRef::parse("credential-secondary").expect("credential ref"),
        )
        .expect("add second connection");
        assert_eq!(second.server_id, server.server_id);

        let snapshot = load_human_workbench_from(&storage).expect("load workspace");
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.servers.len(), 1);
        assert_eq!(snapshot.connections.len(), 2);
        assert_eq!(snapshot.servers[0].display_name, "Production server");
        assert_eq!(
            snapshot.servers[0].project_id,
            Some(project.project_id.clone())
        );

        let non_empty = delete_managed_server_from(
            &storage,
            DeleteManagedServerRequestDto {
                server_id: server.server_id.clone(),
                expected_record_version: 2,
            },
        )
        .expect_err("non-empty server delete must fail");
        assert_eq!(non_empty.code, IpcErrorCode::ManagedServerNotEmpty);

        let project_delete = delete_project_from(
            &storage,
            DeleteProjectRequestDto {
                project_id: project.project_id,
                expected_record_version: 1,
            },
        )
        .expect("delete project and unassign server");
        assert_eq!(project_delete.unassigned_servers, 1);
        let reloaded = load_human_workbench_from(&storage).expect("reload workspace");
        assert!(reloaded.projects.is_empty());
        assert_eq!(reloaded.servers[0].project_id, None);
        assert_eq!(reloaded.servers[0].record_version, 3);

        for profile_id in [first.profile_id, second.profile_id] {
            assert_eq!(
                vigilshell_core::connection_mutation::ConnectionProfileMutationRepository::delete_if_version(
                    &storage,
                    &ConnectionProfileId::parse(profile_id).expect("profile id"),
                    ConnectionRecordVersion::INITIAL,
                )
                .expect("delete profile"),
                vigilshell_core::connection_mutation::DeleteConnectionProfileResult::Deleted
            );
        }
        assert_eq!(
            delete_managed_server_from(
                &storage,
                DeleteManagedServerRequestDto {
                    server_id: server.server_id,
                    expected_record_version: 3,
                },
            )
            .expect("delete empty server")
            .status,
            DeleteManagedServerStatusDto::Deleted
        );
    }

    #[test]
    fn workspace_requests_reject_unknown_fields_and_map_safe_errors() {
        assert!(
            serde_json::from_str::<CreateProjectRequestDto>(
                r#"{"displayName":"Project","unexpected":true}"#
            )
            .is_err()
        );
        assert_eq!(
            IpcErrorDto::from(MutateServerWorkspaceError::Conflict).code,
            IpcErrorCode::WorkspaceConflict
        );
        assert_eq!(
            IpcErrorDto::from(MutateServerWorkspaceError::RecordMismatch).code,
            IpcErrorCode::WorkspaceRecordMismatch
        );
        assert_eq!(
            IpcErrorDto::from(MutateServerWorkspaceError::ServerNotEmpty).code,
            IpcErrorCode::ManagedServerNotEmpty
        );
    }

    #[test]
    fn generated_workspace_catalogue_contract_excludes_sensitive_profile_fields() {
        let generated = concat!(
            include_str!("../../src/lib/desktop/generated/HumanWorkbenchDto.ts"),
            include_str!("../../src/lib/desktop/generated/ProjectDto.ts"),
            include_str!("../../src/lib/desktop/generated/ManagedServerDto.ts"),
            include_str!("../../src/lib/desktop/generated/ConnectionDto.ts"),
        );
        for forbidden in [
            "host:",
            "username:",
            "credential",
            "securityRevision",
            "fingerprint",
            "password",
        ] {
            assert!(!generated.contains(forbidden));
        }
    }

    #[test]
    fn generated_workspace_bindings_are_current() {
        assert_generated_binding::<HumanWorkbenchDto>("HumanWorkbenchDto.ts");
        assert_generated_binding::<ProjectDto>("ProjectDto.ts");
        assert_generated_binding::<ManagedServerDto>("ManagedServerDto.ts");
        assert_generated_binding::<ConnectionDto>("ConnectionDto.ts");
        assert_generated_binding::<ConnectionEditorDto>("ConnectionEditorDto.ts");
        assert_generated_binding::<TrustStateDto>("TrustStateDto.ts");
        assert_generated_binding::<IpcErrorCode>("IpcErrorCode.ts");
        assert_generated_binding::<CreateConnectionProfileRequestDto>(
            "CreateConnectionProfileRequestDto.ts",
        );
        assert_generated_binding::<CreateProjectRequestDto>("CreateProjectRequestDto.ts");
        assert_generated_binding::<UpdateProjectRequestDto>("UpdateProjectRequestDto.ts");
        assert_generated_binding::<DeleteProjectRequestDto>("DeleteProjectRequestDto.ts");
        assert_generated_binding::<UpdateManagedServerRequestDto>(
            "UpdateManagedServerRequestDto.ts",
        );
        assert_generated_binding::<DeleteManagedServerRequestDto>(
            "DeleteManagedServerRequestDto.ts",
        );
        assert_generated_binding::<AddConnectionProfileToServerRequestDto>(
            "AddConnectionProfileToServerRequestDto.ts",
        );
        assert_generated_binding::<DeleteProjectSettlementDto>("DeleteProjectSettlementDto.ts");
        assert_generated_binding::<DeleteProjectStatusDto>("DeleteProjectStatusDto.ts");
        assert_generated_binding::<DeleteManagedServerSettlementDto>(
            "DeleteManagedServerSettlementDto.ts",
        );
        assert_generated_binding::<DeleteManagedServerStatusDto>("DeleteManagedServerStatusDto.ts");
    }

    fn assert_generated_binding<T: TS + 'static>(filename: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/lib/desktop/generated")
            .join(filename);
        let committed = fs::read_to_string(&path).expect("read committed binding");
        let generated =
            T::export_to_string(&ts_rs::Config::from_env()).expect("generate renderer binding");
        assert_eq!(committed, generated, "binding drift: {}", path.display());
    }

    #[test]
    fn connection_update_maps_version_conflicts_and_refreshes_editor_state() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "Initial".to_owned(),
                host: "initial.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-update").expect("profile id"),
            ManagedServerId::parse("server-update").expect("server id"),
            CredentialRef::parse("credential-update").expect("credential ref"),
        )
        .expect("create connection");

        let updated = update_connection_profile_from(
            &storage,
            UpdateConnectionProfileRequestDto {
                profile_id: "profile-update".to_owned(),
                expected_record_version: 1,
                display_name: "Renamed".to_owned(),
                host: "initial.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: false,
            },
        )
        .expect("update connection");
        assert_eq!(updated.display_name, "Renamed");
        assert_eq!(updated.record_version, 2);
        assert!(!updated.enabled);

        let conflict = update_connection_profile_from(
            &storage,
            UpdateConnectionProfileRequestDto {
                profile_id: "profile-update".to_owned(),
                expected_record_version: 1,
                display_name: "Stale writer".to_owned(),
                host: "initial.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
        )
        .expect_err("stale writer must fail");
        assert_eq!(conflict.code, IpcErrorCode::ConnectionConflict);
    }

    #[test]
    fn confirmed_connection_delete_removes_profile_trust_and_missing_password() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "Delete me".to_owned(),
                host: "delete.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-delete").expect("profile id"),
            ManagedServerId::parse("server-delete").expect("server id"),
            CredentialRef::parse("credential-delete").expect("credential ref"),
        )
        .expect("create connection");
        let profile_id = ConnectionProfileId::parse("profile-delete").expect("profile id");
        storage
            .upsert_host_trust_fixture(&StoredHostTrust::new(
                profile_id.clone(),
                ConnectionSecurityRevision::INITIAL,
                HostFingerprint::parse("SHA256:DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDw")
                    .expect("fingerprint"),
            ))
            .expect("store trust");
        let provider = FakeCredentialProvider {
            expected_ref: "credential-delete".to_owned(),
            outcome: FakeCredentialOutcome::Remove(Ok(RemovePasswordSettlement::AlreadyMissing)),
        };

        let settlement = delete_connection_profile_from(
            &storage,
            &provider,
            DeleteConnectionProfileRequestDto {
                profile_id: "profile-delete".to_owned(),
                expected_record_version: 1,
            },
        )
        .expect("delete connection");

        assert_eq!(settlement.status, DeleteConnectionProfileStatusDto::Deleted);
        assert_eq!(
            ConnectionProfileRepository::find(&storage, &profile_id).expect("read profile"),
            None
        );
        assert_eq!(
            HostTrustRepository::find(&storage, &profile_id).expect("read trust"),
            None
        );
    }

    #[test]
    fn post_credential_delete_conflict_reports_incomplete_without_losing_profile() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "Concurrent".to_owned(),
                host: "concurrent.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("profile-concurrent").expect("profile id"),
            ManagedServerId::parse("server-concurrent").expect("server id"),
            CredentialRef::parse("credential-concurrent").expect("credential ref"),
        )
        .expect("create connection");
        let prepared = prepare_connection_delete(
            &storage,
            DeleteConnectionProfileRequestDto {
                profile_id: "profile-concurrent".to_owned(),
                expected_record_version: 1,
            },
        )
        .expect("prepare delete");
        update_connection_profile_from(
            &storage,
            UpdateConnectionProfileRequestDto {
                profile_id: "profile-concurrent".to_owned(),
                expected_record_version: 1,
                display_name: "Concurrent update".to_owned(),
                host: "concurrent.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
        )
        .expect("concurrent update");

        let error = finalize_connection_delete(&storage, prepared)
            .expect_err("stale delete after credential removal must report partial failure");
        assert_eq!(error.code, IpcErrorCode::ConnectionDeleteIncomplete);
        let editor = get_connection_editor_from(
            &storage,
            GetConnectionEditorRequestDto {
                profile_id: "profile-concurrent".to_owned(),
            },
        )
        .expect("profile remains");
        assert_eq!(editor.record_version, 2);
    }

    #[test]
    fn generated_terminal_session_ids_are_canonical_uuid_v4() {
        let first = new_human_terminal_session_id().expect("generate first session id");
        let second = new_human_terminal_session_id().expect("generate second session id");
        assert_ne!(first, second);
        assert_eq!(
            parse_human_terminal_session_id(first.as_str().to_owned()).expect("parse first"),
            first
        );
        for invalid in [
            "session-1",
            "00000000-0000-3000-8000-000000000001",
            "00000000-0000-4000-0000-000000000001",
            "00000000000040008000000000000001",
            "00000000-0000-4000-8000-00000000000A",
        ] {
            assert_eq!(
                parse_human_terminal_session_id(invalid.to_owned())
                    .expect_err("noncanonical id must fail")
                    .code,
                IpcErrorCode::InvalidRequest
            );
        }
    }

    #[test]
    fn serialized_ipc_payloads_exclude_sensitive_fixture_values() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");
        let workbench = load_human_workbench_from(&storage).expect("load workbench");
        let session = HumanTerminalSessionDto {
            session_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            server_id: "server-ready".to_owned(),
            profile_id: "fixture-ready".to_owned(),
        };
        let serialized = format!(
            "{}{}",
            serde_json::to_string(&workbench).expect("serialize workbench"),
            serde_json::to_string(&session).expect("serialize session")
        );

        for forbidden in [
            ".invalid",
            "operator-ready",
            "credential-ref-ready",
            "SHA256:fixture-ready",
            "password",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn credential_configuration_resolves_the_reference_inside_rust() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");
        let provider = FakeCredentialProvider {
            expected_ref: "credential-ref-ready".to_owned(),
            outcome: FakeCredentialOutcome::Configure(Ok(ConfigurePasswordSettlement::Configured)),
        };

        let settlement = configure_human_password_from(
            &storage,
            &provider,
            CredentialActionRequestDto {
                profile_id: "fixture-ready".to_owned(),
            },
        )
        .expect("configure credential");

        assert_eq!(settlement.status, CredentialActionStatusDto::Configured);
        let serialized = serde_json::to_string(&settlement).expect("serialize settlement");
        assert_eq!(serialized, r#"{"status":"configured"}"#);
        assert!(!serialized.contains("credential-ref-ready"));
    }

    #[test]
    fn debug_fixture_routing_uses_the_full_loaded_profile_and_never_falls_back() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");
        let credentials = HumanCredentialRuntime::development(
            None,
            debug_human_terminal_fixture::reserved_credential_refs()
                .expect("reserved credential references"),
        )
        .expect("development credentials");

        let action = prepare_credential_action(
            &storage,
            &credentials,
            CredentialActionRequestDto {
                profile_id: "fixture-ready".to_owned(),
            },
        )
        .expect("classify exact fixture action");
        assert_eq!(action.route, CredentialRoute::Fixture);

        let prepared = HumanTerminalAdmissionService::new(&storage, &storage)
            .prepare(OpenHumanTerminalRequest::new(
                ConnectionProfileId::parse("fixture-ready").expect("profile id"),
                TerminalSize::new(80, 24).expect("terminal size"),
            ))
            .expect("prepare exact fixture terminal");
        assert_eq!(
            classify_human_terminal_route(&prepared, &credentials),
            Ok(HumanTerminalRoute::Fixture)
        );

        update_connection_profile_from(
            &storage,
            UpdateConnectionProfileRequestDto {
                profile_id: "fixture-ready".to_owned(),
                expected_record_version: 1,
                display_name: "Changed reserved fixture".to_owned(),
                host: "changed.vigilshell.invalid".to_owned(),
                port: 22,
                username: "operator-ready".to_owned(),
                enabled: true,
            },
        )
        .expect("mutate reserved fixture");
        let error = match prepare_credential_action(
            &storage,
            &credentials,
            CredentialActionRequestDto {
                profile_id: "fixture-ready".to_owned(),
            },
        ) {
            Ok(_) => panic!("reserved mismatch must fail closed"),
            Err(error) => error,
        };
        assert_eq!(error.code, IpcErrorCode::CredentialProviderUnavailable);
    }

    #[test]
    fn debug_fixture_requires_explicit_runtime_opt_in_and_ordinary_profiles_stay_production() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");
        let production = HumanCredentialRuntime::production(None).expect("production credentials");
        let reserved = match prepare_credential_action(
            &storage,
            &production,
            CredentialActionRequestDto {
                profile_id: "fixture-ready".to_owned(),
            },
        ) {
            Ok(_) => panic!("reserved fixture without opt-in must fail closed"),
            Err(error) => error,
        };
        assert_eq!(reserved.code, IpcErrorCode::CredentialProviderUnavailable);

        create_connection_profile_from(
            &storage,
            CreateConnectionProfileRequestDto {
                display_name: "Ordinary".to_owned(),
                host: "ordinary.example.invalid".to_owned(),
                port: 22,
                username: "operator".to_owned(),
                enabled: true,
            },
            ConnectionProfileId::parse("ordinary-profile").expect("profile id"),
            ManagedServerId::parse("ordinary-server").expect("server id"),
            CredentialRef::parse("credential-ref-ready").expect("credential ref"),
        )
        .expect("create ordinary profile");
        let development = HumanCredentialRuntime::development(
            None,
            debug_human_terminal_fixture::reserved_credential_refs()
                .expect("reserved credential references"),
        )
        .expect("development credentials");
        let ordinary = prepare_credential_action(
            &storage,
            &development,
            CredentialActionRequestDto {
                profile_id: "ordinary-profile".to_owned(),
            },
        )
        .expect("classify ordinary profile");
        assert_eq!(ordinary.route, CredentialRoute::Production);
    }

    #[test]
    fn password_status_resolves_the_reference_and_returns_only_safe_state() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");

        for (status, expected) in [
            (
                PasswordConfigurationStatus::Configured,
                (
                    HumanPasswordStatusValueDto::Configured,
                    r#"{"status":"configured"}"#,
                ),
            ),
            (
                PasswordConfigurationStatus::NotConfigured,
                (
                    HumanPasswordStatusValueDto::NotConfigured,
                    r#"{"status":"not_configured"}"#,
                ),
            ),
        ] {
            let provider = FakeCredentialProvider {
                expected_ref: "credential-ref-ready".to_owned(),
                outcome: FakeCredentialOutcome::Status(Ok(status)),
            };
            let result = get_human_password_status_from(
                &storage,
                &provider,
                CredentialActionRequestDto {
                    profile_id: "fixture-ready".to_owned(),
                },
            )
            .expect("query password status");
            let serialized = serde_json::to_string(&result).expect("serialize password status");

            assert_eq!(result.status, expected.0);
            assert_eq!(serialized, expected.1);
            for forbidden in [
                "credential-ref-ready",
                "password",
                ".invalid",
                "operator-ready",
            ] {
                assert!(!serialized.contains(forbidden));
            }
        }
    }

    #[test]
    fn password_status_fails_closed_with_safe_error_codes() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");

        for (error, expected) in [
            (
                CredentialProviderError::AccessDenied,
                IpcErrorCode::CredentialAccessDenied,
            ),
            (
                CredentialProviderError::Unavailable,
                IpcErrorCode::CredentialProviderUnavailable,
            ),
        ] {
            let provider = FakeCredentialProvider {
                expected_ref: "credential-ref-ready".to_owned(),
                outcome: FakeCredentialOutcome::Status(Err(error)),
            };
            let failure = get_human_password_status_from(
                &storage,
                &provider,
                CredentialActionRequestDto {
                    profile_id: "fixture-ready".to_owned(),
                },
            )
            .expect_err("status query must fail closed");
            let serialized = serde_json::to_string(&failure).expect("serialize status error");

            assert_eq!(failure.code, expected);
            assert!(!serialized.contains("credential-ref-ready"));
            assert!(!serialized.contains("Keychain"));
        }
    }

    #[test]
    fn credential_removal_is_idempotent_and_cancel_is_safe() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        seed_debug_storage(&storage).expect("seed debug records");
        let missing_provider = FakeCredentialProvider {
            expected_ref: "credential-ref-ready".to_owned(),
            outcome: FakeCredentialOutcome::Remove(Ok(RemovePasswordSettlement::AlreadyMissing)),
        };
        let cancelled_provider = FakeCredentialProvider {
            expected_ref: "credential-ref-ready".to_owned(),
            outcome: FakeCredentialOutcome::Remove(Ok(RemovePasswordSettlement::Cancelled)),
        };

        let request = || CredentialActionRequestDto {
            profile_id: "fixture-ready".to_owned(),
        };
        assert_eq!(
            remove_human_password_from(&storage, &missing_provider, request())
                .expect("remove missing credential")
                .status,
            CredentialActionStatusDto::AlreadyMissing
        );
        assert_eq!(
            remove_human_password_from(&storage, &cancelled_provider, request())
                .expect("cancel credential removal")
                .status,
            CredentialActionStatusDto::Cancelled
        );
    }

    #[test]
    fn credential_actions_fail_closed_before_invoking_the_provider() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let provider = FakeCredentialProvider {
            expected_ref: "must-not-be-used".to_owned(),
            outcome: FakeCredentialOutcome::Configure(Ok(ConfigurePasswordSettlement::Configured)),
        };

        let error = configure_human_password_from(
            &storage,
            &provider,
            CredentialActionRequestDto {
                profile_id: "missing-profile".to_owned(),
            },
        )
        .expect_err("missing profile must fail closed");
        assert_eq!(error.code, IpcErrorCode::ConnectionNotFound);

        let invalid = configure_human_password_from(
            &storage,
            &provider,
            CredentialActionRequestDto {
                profile_id: "   ".to_owned(),
            },
        )
        .expect_err("invalid profile id must fail closed");
        assert_eq!(invalid.code, IpcErrorCode::InvalidRequest);
    }

    #[test]
    fn credential_provider_errors_map_to_stable_safe_codes() {
        for (error, expected) in [
            (
                CredentialProviderError::AccessDenied,
                IpcErrorCode::CredentialAccessDenied,
            ),
            (
                CredentialProviderError::InvalidSecret,
                IpcErrorCode::CredentialInvalidSecret,
            ),
            (
                CredentialProviderError::Missing,
                IpcErrorCode::CredentialProviderUnavailable,
            ),
            (
                CredentialProviderError::MainThreadRequired,
                IpcErrorCode::CredentialProviderUnavailable,
            ),
            (
                CredentialProviderError::Unavailable,
                IpcErrorCode::CredentialProviderUnavailable,
            ),
        ] {
            let serialized = serde_json::to_string(&IpcErrorDto::from(error))
                .expect("serialize credential provider error");
            assert_eq!(IpcErrorDto::from(error).code, expected);
            assert!(!serialized.contains("credential-ref"));
            assert!(!serialized.contains("Keychain"));
        }
    }

    #[test]
    fn ipc_errors_serialize_only_a_stable_code() {
        let serialized =
            serde_json::to_string(&IpcErrorDto::from(IpcErrorCode::CredentialUnavailable))
                .expect("serialize IPC error");

        assert_eq!(serialized, r#"{"code":"credential_unavailable"}"#);
    }

    #[test]
    fn application_quit_atomically_closes_command_admission() {
        let lifecycle = ResidentLifecycleRuntime::new();
        let operation = admit_effectful_operation(&lifecycle).expect("admit operation");
        let BeginQuit::Leader(leader) = lifecycle.begin_quit() else {
            panic!("first quit should own settlement");
        };

        let rejected = match admit_effectful_operation(&lifecycle) {
            Ok(_) => panic!("quit must reject new desktop commands"),
            Err(error) => error,
        };
        assert_eq!(rejected.code, IpcErrorCode::ApplicationShuttingDown);
        assert_eq!(
            serde_json::to_string(&rejected).expect("serialize shutdown error"),
            r#"{"code":"application_shutting_down"}"#
        );

        drop(operation);
        assert_eq!(leader.wait_for_operations(), ResidentDrainOutcome::Drained);
        leader.mark_exit_ready();
    }

    #[test]
    fn shutdown_step_returns_at_its_budget_even_when_the_worker_is_blocked() {
        assert_eq!(
            run_shutdown_step_within(Duration::from_secs(1), || 42),
            Some(42)
        );

        let (release_sender, release_receiver) = mpsc::sync_channel(1);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        let result = run_shutdown_step_within(Duration::from_millis(5), move || {
            release_receiver.recv().expect("release worker");
            finished_sender.send(()).expect("report worker completion");
        });

        assert_eq!(result, None);
        release_sender.send(()).expect("release timed-out worker");
        finished_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("timed-out worker should finish after release");
    }
}

#[cfg(test)]
mod terminal_ipc_tests {
    use std::sync::{Arc, Mutex, mpsc};

    use super::*;

    #[test]
    fn terminal_sink_acknowledges_only_a_delivered_opened_lifecycle() {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&delivered);
        let channel = Channel::new(move |body| {
            observed.lock().expect("delivery lock").push(body);
            Ok(())
        });
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let mut sink = TauriTerminalOutputSink {
            channel,
            session_id: test_session_id().to_owned(),
            ready_sender: Some(ready_sender),
        };

        sink.send(RusshTerminalOutput::Lifecycle(
            RusshTerminalLifecycle::Ready,
        ))
        .expect("opened lifecycle delivery");

        assert_eq!(
            ready_receiver.recv().expect("ready acknowledgement"),
            Ok(())
        );
        let bodies = delivered.lock().expect("delivery lock");
        assert_eq!(bodies.len(), 1);
        match &bodies[0] {
            InvokeResponseBody::Json(json) => assert_eq!(
                json,
                r#"{"type":"opened","sessionId":"00000000-0000-4000-8000-000000000001"}"#
            ),
            InvokeResponseBody::Raw(_) => panic!("opened lifecycle must use JSON"),
        }
    }

    #[test]
    fn terminal_sink_rejects_open_when_the_channel_rejects_opened() {
        let channel = Channel::new(|_| Err(tauri::Error::FailedToReceiveMessage));
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let mut sink = TauriTerminalOutputSink {
            channel,
            session_id: test_session_id().to_owned(),
            ready_sender: Some(ready_sender),
        };

        assert!(
            sink.send(RusshTerminalOutput::Lifecycle(
                RusshTerminalLifecycle::Ready
            ))
            .is_err()
        );
        assert_eq!(ready_receiver.recv().expect("ready rejection"), Err(()));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_terminal_sink_uses_the_same_open_ack_json_and_raw_channel() {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&delivered);
        let channel = Channel::new(move |body| {
            observed.lock().expect("delivery lock").push(body);
            Ok(())
        });
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let mut sink = TauriTerminalOutputSink {
            channel,
            session_id: test_session_id().to_owned(),
            ready_sender: Some(ready_sender),
        };

        DebugTerminalOutputSink::send_lifecycle(&mut sink, DebugTerminalLifecycle::Opened)
            .expect("debug opened delivery");
        DebugTerminalOutputSink::send_data(&mut sink, vec![0xff, 0x00])
            .expect("debug raw delivery");

        assert_eq!(
            ready_receiver.recv().expect("ready acknowledgement"),
            Ok(())
        );
        let bodies = delivered.lock().expect("delivery lock");
        assert!(matches!(
            bodies.first(),
            Some(InvokeResponseBody::Json(json))
                if json == r#"{"type":"opened","sessionId":"00000000-0000-4000-8000-000000000001"}"#
        ));
        assert!(matches!(
            bodies.get(1),
            Some(InvokeResponseBody::Raw(bytes)) if bytes == &[0xff, 0x00]
        ));
    }

    #[test]
    fn terminal_channel_keeps_non_utf8_data_raw_and_lifecycle_json() {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&delivered);
        let channel = Channel::new(move |body| {
            observed.lock().expect("delivery lock").push(body);
            Ok(())
        });

        channel
            .send(InvokeResponseBody::Raw(vec![0, 0xff, 0x80]))
            .expect("raw delivery");
        channel
            .send(InvokeResponseBody::Json(r#"{"type":"eof"}"#.to_owned()))
            .expect("JSON delivery");

        let bodies = delivered.lock().expect("delivery lock");
        assert!(matches!(
            bodies.first(),
            Some(InvokeResponseBody::Raw(bytes)) if bytes == &[0, 0xff, 0x80]
        ));
        assert!(matches!(
            bodies.get(1),
            Some(InvokeResponseBody::Json(json)) if json == r#"{"type":"eof"}"#
        ));
    }

    #[test]
    fn terminal_lifecycle_and_session_dtos_expose_only_safe_fields() {
        let session = HumanTerminalSessionDto {
            session_id: test_session_id().to_owned(),
            server_id: "server-safe".to_owned(),
            profile_id: "profile-safe".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&session).expect("serialize session"),
            r#"{"sessionId":"00000000-0000-4000-8000-000000000001","serverId":"server-safe","profileId":"profile-safe"}"#
        );

        for (lifecycle, expected) in [
            (
                RusshTerminalLifecycle::Ready,
                r#"{"type":"opened","sessionId":"00000000-0000-4000-8000-000000000001"}"#,
            ),
            (
                RusshTerminalLifecycle::Eof,
                r#"{"type":"eof","sessionId":"00000000-0000-4000-8000-000000000001"}"#,
            ),
            (
                RusshTerminalLifecycle::ExitStatus(23),
                r#"{"type":"exit_status","sessionId":"00000000-0000-4000-8000-000000000001","status":23}"#,
            ),
            (
                RusshTerminalLifecycle::ExitSignal {
                    signal: RusshTerminalExitSignal::Terminate,
                    core_dumped: false,
                },
                r#"{"type":"exit_signal","sessionId":"00000000-0000-4000-8000-000000000001","signal":"terminate","coreDumped":false}"#,
            ),
            (
                RusshTerminalLifecycle::Closed,
                r#"{"type":"closed","sessionId":"00000000-0000-4000-8000-000000000001"}"#,
            ),
            (
                RusshTerminalLifecycle::Disconnected,
                r#"{"type":"disconnected","sessionId":"00000000-0000-4000-8000-000000000001"}"#,
            ),
            (
                RusshTerminalLifecycle::ProtocolFailure,
                r#"{"type":"protocol_failure","sessionId":"00000000-0000-4000-8000-000000000001"}"#,
            ),
        ] {
            assert_eq!(
                serde_json::to_string(&lifecycle_dto(test_session_id(), lifecycle))
                    .expect("serialize lifecycle"),
                expected
            );
        }
    }

    #[test]
    fn terminal_errors_are_stable_and_redacted() {
        for error in [
            map_terminal_registry_error(TerminalRegistryError::GlobalCapacityReached),
            map_terminal_registry_error(TerminalRegistryError::ProfileCapacityReached),
            map_terminal_registry_error(TerminalRegistryError::ProfileDeleting),
            map_terminal_registry_error(TerminalRegistryError::ProfileInUse),
            map_terminal_registry_error(TerminalRegistryError::ControlQueueFull),
            map_terminal_registry_error(TerminalRegistryError::SessionClosed),
            map_terminal_open_error(RusshTerminalOpenError::HostKeyMismatch),
            map_terminal_open_error(RusshTerminalOpenError::AuthRejected),
            map_terminal_open_error(RusshTerminalOpenError::ChannelRejected),
            map_terminal_open_error(RusshTerminalOpenError::PtyRejected),
            map_terminal_open_error(RusshTerminalOpenError::ShellRejected),
            map_terminal_open_error(RusshTerminalOpenError::Disconnected),
            map_terminal_open_error(RusshTerminalOpenError::Timeout),
            map_terminal_open_error(RusshTerminalOpenError::Unavailable),
        ] {
            let serialized = serde_json::to_string(&error).expect("serialize terminal error");
            assert_eq!(serialized.matches(':').count(), 1);
            for forbidden in [
                ".invalid",
                "operator",
                "credential",
                "password",
                "SHA256:",
                "russh",
            ] {
                assert!(!serialized.contains(forbidden));
            }
        }

        for (error, expected) in [
            (
                RusshTerminalOpenError::HostKeyMismatch,
                IpcErrorCode::TerminalHostKeyMismatch,
            ),
            (
                RusshTerminalOpenError::AuthRejected,
                IpcErrorCode::TerminalAuthenticationRejected,
            ),
            (
                RusshTerminalOpenError::ChannelRejected,
                IpcErrorCode::TerminalChannelRejected,
            ),
            (
                RusshTerminalOpenError::PtyRejected,
                IpcErrorCode::TerminalPtyRejected,
            ),
            (
                RusshTerminalOpenError::ShellRejected,
                IpcErrorCode::TerminalShellRejected,
            ),
            (
                RusshTerminalOpenError::InvalidCredentialEncoding,
                IpcErrorCode::TerminalUnavailable,
            ),
            (
                RusshTerminalOpenError::Disconnected,
                IpcErrorCode::TerminalDisconnected,
            ),
            (
                RusshTerminalOpenError::Timeout,
                IpcErrorCode::TerminalTimeout,
            ),
            (
                RusshTerminalOpenError::Unavailable,
                IpcErrorCode::TerminalUnavailable,
            ),
        ] {
            assert_eq!(map_terminal_open_error(error).code, expected);
        }
    }

    #[test]
    fn terminal_input_debug_exposes_only_length_not_bytes() {
        let request = HumanTerminalInputRequestDto {
            session_id: test_session_id().to_owned(),
            data: b"sensitive-command-marker".to_vec(),
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("data_length: 24"));
        assert!(!debug.contains("sensitive-command-marker"));
        assert!(!debug.contains("115, 101, 110"));
    }

    #[test]
    fn workbench_reports_terminal_available_in_production_contract() {
        let storage = SqliteStorage::open_in_memory().expect("open storage");
        let workbench = load_human_workbench_from(&storage).expect("load workbench");
        assert!(workbench.terminal_available);
    }

    fn test_session_id() -> &'static str {
        "00000000-0000-4000-8000-000000000001"
    }
}
