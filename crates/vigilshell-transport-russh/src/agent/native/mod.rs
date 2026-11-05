//! 具名原生维护的共同 SSH 结果与文件基础；不接受程序、UID、总线或额外参数。
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::atomic::Ordering};
use vigilshell_application_contracts::ports::{
    FileExecutionControl, FileTransferHandles, NativeOperationOutput, PreparedNativeOperation,
};
use vigilshell_domain::agent::*;
mod account;
mod backend;
mod environment;
mod file_io;
pub(super) mod programs;
pub(super) use environment::inspect_environment;
mod process;
mod service_definition;
mod services;
mod settings;
mod source_write;
mod sources;

pub(super) async fn prepare(
    connection: Arc<Connection>,
    operation: CheckedNativeOperation,
) -> Result<Box<dyn PreparedNativeOperation>, WorkspaceError> {
    let null = Arc::new(process::Silent);
    let control = FileExecutionControl::default();
    let home = process::capture(
        &connection,
        "printf '%s' \"$HOME\"".into(),
        null.clone(),
        &control,
        false,
    )
    .await
    .map_err(|_| WorkspaceError::ToolsUnavailable)?
    .text;
    if !home.starts_with('/')
        || normalize_path("/", &home).map_err(|_| WorkspaceError::InvalidInput)? != home
        || home == "/"
    {
        return Err(WorkspaceError::FilesUnsupported);
    }
    let programs = programs::probe(&connection, &control)
        .await
        .map_err(|_| WorkspaceError::ToolsUnavailable)?;
    let root = format!(
        "{home}/.local/share/vigilshell/native-staging/{}",
        operation.id()
    );
    Ok(Box::new(Plan {
        connection,
        operation,
        root,
        programs,
    }))
}
struct Plan {
    connection: Arc<Connection>,
    operation: CheckedNativeOperation,
    root: String,
    programs: programs::Programs,
}
impl PreparedNativeOperation for Plan {
    fn recovery_paths(&self) -> Vec<String> {
        if !self.operation.operation().mutating()
            || matches!(
                self.operation.operation(),
                RuntimeOperation::PowerControl { .. }
            )
        {
            return vec![];
        }
        let mut paths = vec![self.root.clone()];
        if self.operation.operation().is_package_source() {
            paths.push(
                self.operation
                    .backup_root(&self.root)
                    .expect("locally constructed native staging path"),
            );
            for parent in [
                "/etc/apt",
                "/etc/apt/sources.list.d",
                "/etc/yum.repos.d",
                "/usr/share/keyrings",
                "/etc/pki/rpm-gpg",
            ] {
                paths.push(format!("{parent}/.vigilshell-{}.part", self.operation.id()));
            }
        }
        if !self.operation.operation().is_package_operation() {
            paths.push(
                self.operation
                    .backup_root(&self.root)
                    .expect("locally constructed native staging path"),
            );
        }
        paths
    }
    fn execute(
        self: Box<Self>,
        output: Arc<dyn NativeOperationOutput>,
        control: FileExecutionControl,
    ) -> FutureResult<'static, NativeExecutionReport> {
        Box::pin(async move {
            let paths = self.recovery_paths();
            let Plan {
                connection,
                operation,
                root,
                programs,
            } = *self;
            let mut run = Run {
                connection: &connection,
                operation,
                root,
                programs,
                output,
                control,
                report: NativeExecutionReport {
                    outcome: None,
                    effects: FileCommitState::Unchanged,
                    error: None,
                    json: None,
                    recovery_paths: paths,
                },
            };
            let result = if run.operation.operation().is_service_operation() {
                run.services().await
            } else if !run.operation.operation().is_package_operation() {
                run.settings().await
            } else if run.operation.operation().is_package_source() {
                run.sources().await
            } else {
                run.packages().await
            };
            match result {
                Ok(value) => {
                    run.report.json = Some(value.to_string());
                    if run.report.outcome.is_none() {
                        run.report.outcome = Some(ExecutionOutcome::Completed { exit_code: 0 });
                    }
                    if matches!(
                        run.report.outcome,
                        Some(ExecutionOutcome::RequestAccepted {})
                    ) {
                        run.report.effects = FileCommitState::Unknown;
                    } else if run.report.effects != FileCommitState::Unchanged {
                        run.report.effects = FileCommitState::Committed;
                    }
                }
                Err(code) => {
                    run.report.error = Some(code.into());
                    let interrupted = run.report.effects == FileCommitState::Unknown
                        || run.control.cancel.load(Ordering::Acquire)
                        || matches!(
                            code,
                            "native_result_unknown"
                                | "native_package_result_unknown"
                                | "native_service_result_unknown"
                                | "native_setting_result_unknown"
                                | "native_output_unavailable"
                                | "native_output_limit"
                                | "native_output_invalid"
                                | "native_process_disconnected"
                        );
                    run.report.outcome = Some(if interrupted {
                        run.report.effects = FileCommitState::Unknown;
                        ExecutionOutcome::Unknown {
                            reason: if run.control.cancel.load(Ordering::Acquire) {
                                ExecutionStopReason::Cancelled
                            } else {
                                ExecutionStopReason::Disconnected
                            },
                        }
                    } else {
                        ExecutionOutcome::Completed { exit_code: 1 }
                    });
                }
            }
            // 仅已确认结果后清理本任务暂存；备份和不可确认对象始终保留。
            if run.report.effects != FileCommitState::Unknown
                && run.operation.operation().mutating()
            {
                let operation = RuntimeOperation::FilesRemove {
                    target: scoped(&run.root),
                    recursive: true,
                };
                if let Ok(permit) = native_file_operation(&run.operation, operation, &run.root) {
                    let clean = FileExecutionControl::default();
                    if let Ok(report) = files::execute(
                        run.connection,
                        permit,
                        FileTransferHandles::default(),
                        clean,
                    )
                    .await
                        && (report.error.is_none()
                            || report.error.as_deref() == Some("file_not_found"))
                    {
                        run.report.recovery_paths.retain(|p| p != &run.root);
                    }
                }
            }
            Ok(run.report)
        })
    }
}
struct Run<'a> {
    connection: &'a Connection,
    operation: CheckedNativeOperation,
    root: String,
    programs: programs::Programs,
    output: Arc<dyn NativeOperationOutput>,
    control: FileExecutionControl,
    report: NativeExecutionReport,
}
impl Run<'_> {
    fn check(&self) -> AgentResult<()> {
        if self.control.cancel.load(Ordering::Acquire) {
            Err("native_cancelled")
        } else {
            Ok(())
        }
    }
    async fn exec(
        &self,
        name: &str,
        args: &[String],
        stream: bool,
    ) -> AgentResult<process::ResultData> {
        self.check()?;
        self.exec_env(name, args, stream, &[]).await
    }
    async fn exec_env(
        &self,
        name: &str,
        args: &[String],
        stream: bool,
        environment: &[(&str, String)],
    ) -> AgentResult<process::ResultData> {
        self.check()?;
        let path = self.programs.path(name)?;
        let body = process::command(path, args, environment);
        process::capture(
            self.connection,
            body,
            self.output.clone(),
            &self.control,
            stream,
        )
        .await
    }
}

fn scoped(path: &str) -> ScopedPath {
    ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: path.trim_start_matches('/').into(),
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
