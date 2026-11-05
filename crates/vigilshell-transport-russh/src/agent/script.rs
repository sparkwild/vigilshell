//! 原生脚本文件入口；审阅原文件在启动前复核，保留解释器文件语义。
use super::*;
use sha2::{Digest, Sha256};
use vigilshell_application_contracts::ports::{
    FileExecutionControl, FileTransferHandles, ScriptPreparation,
};
use vigilshell_domain::agent::*;
fn scoped(path: &str) -> ScopedPath {
    ScopedPath {
        scope_id: ACCOUNT_SCOPE_ID.into(),
        path: path.trim_start_matches('/').into(),
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl Connection {
    pub(super) async fn recognize_script(
        &self,
        path: String,
    ) -> Result<Option<ScriptInterpreter>, WorkspaceError> {
        use crate::sftp::Handle;
        let session = self
            .sftp
            .get_or_try_init(|| crate::sftp::open(&self.handle))
            .await?;
        let attrs = match session.0.lstat(&path).await {
            Ok(value) => value.attrs,
            Err(_) => return Ok(None),
        };
        if !attrs.is_regular() {
            return Ok(None);
        }
        let file = match session
            .0
            .open(
                &path,
                russh_sftp::protocol::OpenFlags::READ,
                crate::sftp::empty_attrs(),
            )
            .await
        {
            Ok(file) => Handle {
                sftp: session.0.clone(),
                id: Some(file.handle),
            },
            Err(_) => return Ok(None),
        };
        let bytes = session
            .0
            .read(file.id(), 0, 256)
            .await
            .map(|p| p.data.to_vec())
            .unwrap_or_default();
        file.close().await?;
        if bytes.starts_with(b"\x7fELF") {
            return Ok(None);
        }
        let line = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.lines().next())
            .unwrap_or("");
        let Some(shebang) = line.strip_prefix("#!") else {
            return Ok(None);
        };
        let words: Vec<_> = shebang.split_whitespace().collect();
        let program = match words.as_slice() {
            [program] => *program,
            ["/usr/bin/env", program] => *program,
            _ => return Err(WorkspaceError::ToolsUnavailable),
        };
        Ok(match program {
            "sh" | "/bin/sh" | "/usr/bin/sh" => Some(ScriptInterpreter::Sh),
            "bash" | "/bin/bash" | "/usr/bin/bash" => Some(ScriptInterpreter::Bash),
            "python3" | "/usr/bin/python3" => Some(ScriptInterpreter::Python3),
            "node" | "/usr/bin/node" => Some(ScriptInterpreter::Node),
            _ => return Err(WorkspaceError::ToolsUnavailable),
        })
    }

    async fn script_home(&self) -> Result<String, WorkspaceError> {
        let id_program = native::programs::trusted_program(self, "/usr/bin/id")
            .await
            .map_err(|_| WorkspaceError::ToolsUnavailable)?;
        let uid = self
            .control_bounded(format!("exec {} -u", shell_quote(&id_program)), 32)
            .await?;
        let uid = uid
            .trim()
            .parse::<u32>()
            .map_err(|_| WorkspaceError::InvalidInput)?;
        let getent = native::programs::trusted_program(self, "/usr/bin/getent")
            .await
            .map_err(|_| WorkspaceError::ToolsUnavailable)?;
        let entry = self
            .control_bounded(
                format!("exec {} passwd {}", shell_quote(&getent), uid),
                8192,
            )
            .await?;
        let fields: Vec<_> = entry.trim_end().split(':').collect();
        if fields.len() != 7 || fields[2].parse::<u32>().ok() != Some(uid) {
            return Err(WorkspaceError::InvalidInput);
        }
        let home = fields[5].to_owned();
        if !home.starts_with('/')
            || home == "/"
            || normalize_path("/", &home).ok().as_ref() != Some(&home)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(home)
    }
    async fn script_file(
        &self,
        checked: &CheckedScriptOperation,
        root: &str,
        op: RuntimeOperation,
    ) -> Result<serde_json::Value, WorkspaceError> {
        let permit =
            script_file_operation(checked, op, root).map_err(|_| WorkspaceError::InvalidInput)?;
        let report = files::execute(
            self,
            permit,
            FileTransferHandles::default(),
            FileExecutionControl::default(),
        )
        .await?;
        if report.error.is_some() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        serde_json::from_str(
            report
                .json
                .as_deref()
                .ok_or(WorkspaceError::StreamUnavailable)?,
        )
        .map_err(|_| WorkspaceError::StreamUnavailable)
    }

    async fn read_script(
        &self,
        path: &str,
        capture: bool,
    ) -> Result<(String, String), WorkspaceError> {
        use crate::sftp::{Handle, sftp_error};
        use russh_sftp::{
            client::error::Error,
            protocol::{OpenFlags, StatusCode},
        };
        let session = self
            .sftp
            .get_or_try_init(|| crate::sftp::open(&self.handle))
            .await?;
        let opened = session
            .0
            .open(path, OpenFlags::READ, crate::sftp::empty_attrs())
            .await
            .map_err(sftp_error)?;
        let file = Handle {
            sftp: session.0.clone(),
            id: Some(opened.handle),
        };
        let attrs = session.0.fstat(file.id()).await.map_err(sftp_error)?.attrs;
        if !attrs.is_regular() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let mut bytes = Vec::new();
        let mut hash = Sha256::new();
        let mut offset = 0;
        loop {
            let chunk = match session.0.read(file.id(), offset, 32768).await {
                Ok(chunk) => chunk.data,
                Err(Error::Status(status)) if status.status_code == StatusCode::Eof => break,
                Err(error) => return Err(sftp_error(error)),
            };
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            hash.update(&chunk);
            if capture {
                if offset > SCRIPT_LIMIT as u64 {
                    return Err(WorkspaceError::FilesTooLarge);
                }
                bytes.extend(chunk);
            }
        }
        file.close().await?;
        Ok((
            String::from_utf8(bytes).map_err(|_| WorkspaceError::UnmappableInput)?,
            format!("{:x}", hash.finalize()),
        ))
    }
    pub(super) async fn snapshot_script(
        &self,
        checked: CheckedScriptOperation,
    ) -> Result<ScriptPreparation, WorkspaceError> {
        let RuntimeOperation::Script {
            source,
            interpreter,
            arguments,
            directory,
        } = &checked.request().operation
        else {
            return Err(WorkspaceError::InvalidInput);
        };
        let home = self.script_home().await?;
        let root = format!(
            "{home}/.local/share/vigilshell/script-staging/{}",
            checked.request().identity.task_id
        );
        let (path, text, staging_path) = match source {
            ScriptSource::Inline { text } => {
                let path = format!("{root}/entry");
                (path.clone(), text.clone(), Some(path))
            }
            ScriptSource::RemoteFile {
                target,
                expected_hash,
            } => {
                let path =
                    normalize_path("/", &target.path).map_err(|_| WorkspaceError::InvalidInput)?;
                let (text, hash_value) = self.read_script(&path, true).await?;
                if expected_hash
                    .as_ref()
                    .is_some_and(|hash| *hash != hash_value)
                {
                    return Err(WorkspaceError::Conflict);
                }
                (path, text, None)
            }
        };
        let snapshot = ScriptSnapshot {
            hash: digest(text.as_bytes()),
            text,
            path,
            source_version: None,
            interpreter: *interpreter,
            arguments: arguments.clone(),
            directory: normalize_path(
                "/",
                if directory.path.is_empty() {
                    "."
                } else {
                    &directory.path
                },
            )
            .map_err(|_| WorkspaceError::InvalidInput)?,
        };
        checked
            .bind(snapshot.clone())
            .map_err(|_| WorkspaceError::InvalidInput)?;
        Ok(ScriptPreparation {
            snapshot,
            staging_path,
        })
    }
    pub(super) async fn prepare_script_launch_native(
        self: Arc<Self>,
        checked: CheckedScriptLaunch,
    ) -> Result<Box<dyn vigilshell_application_contracts::ports::PreparedScript>, WorkspaceError>
    {
        let permit = checked.operation();
        let RuntimeOperation::Script {
            source,
            interpreter,
            arguments,
            directory,
        } = &permit.request().operation
        else {
            return Err(WorkspaceError::InvalidInput);
        };
        let home = self.script_home().await?;
        let root = format!(
            "{home}/.local/share/vigilshell/script-staging/{}",
            permit.request().identity.task_id
        );
        let path = match source {
            ScriptSource::Inline { .. } => format!("{root}/entry"),
            ScriptSource::RemoteFile { target, .. } => {
                normalize_path("/", &target.path).map_err(|_| WorkspaceError::InvalidInput)?
            }
        };
        let mut cleanup = None;
        if let ScriptSource::Inline { text } = source {
            let file = path.as_str();
            if file != format!("{root}/entry") {
                return Err(WorkspaceError::Conflict);
            }
            self.script_file(
                permit,
                &root,
                RuntimeOperation::FilesMkdir {
                    target: scoped(root.rsplit_once('/').ok_or(WorkspaceError::InvalidInput)?.0),
                    parents: true,
                },
            )
            .await?;
            self.script_file(
                permit,
                &root,
                RuntimeOperation::FilesMkdir {
                    target: scoped(&root),
                    parents: false,
                },
            )
            .await?;
            cleanup = Some(InlineFiles {
                session: self
                    .sftp
                    .get()
                    .ok_or(WorkspaceError::FilesUnsupported)?
                    .0
                    .clone(),
                file: file.into(),
                root: root.clone(),
                armed: true,
            });
            self.script_file(
                permit,
                &root,
                RuntimeOperation::FilesSetAttributes {
                    target: scoped(&root),
                    mode: Some(0o700),
                    modified_seconds: None,
                },
            )
            .await?;
            let operation = script_file_operation(
                permit,
                RuntimeOperation::FilesWrite {
                    target: scoped(file),
                    data: String::new(),
                    mode: FileWriteMode::Create,
                    expected_hash: None,
                },
                &root,
            )
            .map_err(|_| WorkspaceError::InvalidInput)?;
            let report = files::write_maintenance(
                &self,
                operation,
                text.as_bytes().to_vec(),
                FileExecutionControl::default(),
            )
            .await?;
            if report.error.is_some() {
                return Err(WorkspaceError::FilesUnsupported);
            }
            self.script_file(
                permit,
                &root,
                RuntimeOperation::FilesSetAttributes {
                    target: scoped(file),
                    mode: Some(0o400),
                    modified_seconds: None,
                },
            )
            .await?;
        } else if let ScriptSource::RemoteFile { expected_hash, .. } = source {
            // 仅在用户选择查看或调用者显式指定摘要时读正文；属性变化不作为拒绝理由。
            let expected = checked
                .snapshot()
                .map(|s| &s.hash)
                .or(expected_hash.as_ref());
            if let Some(expected) = expected {
                let (_, actual) = self.read_script(&path, false).await?;
                if *expected != actual {
                    return Err(WorkspaceError::Conflict);
                }
            }
        }
        let program = native::programs::trusted_program(&self, interpreter.program())
            .await
            .map_err(|_| WorkspaceError::ToolsUnavailable)?;
        let arguments = arguments
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        let wire = format!(
            "cd {} || exit 125; exec /usr/bin/env -i PATH=/usr/bin:/bin HOME={} LANG=C.UTF-8 LC_ALL=C.UTF-8 {} -- {} {arguments}",
            shell_quote(
                &normalize_path(
                    "/",
                    if directory.path.is_empty() {
                        "."
                    } else {
                        &directory.path
                    }
                )
                .map_err(|_| WorkspaceError::InvalidInput)?
            ),
            shell_quote(&home),
            shell_quote(&program),
            shell_quote(&path)
        );
        Ok(Box::new(Prepared {
            connection: self,
            wire,
            cleanup,
        }))
    }
}
struct Prepared {
    connection: Arc<Connection>,
    wire: String,
    cleanup: Option<InlineFiles>,
}
impl vigilshell_application_contracts::ports::PreparedScript for Prepared {
    fn launch(mut self: Box<Self>) -> FutureResult<'static, Box<dyn AgentProcess>> {
        Box::pin(async move {
            // 请求一旦开始发送，断线不能证明解释器未运行，保留 Inline 文件用于结果核对。
            if let Some(files) = &mut self.cleanup {
                files.armed = false;
            }
            let inner = self
                .connection
                .open_exec_encoded(self.wire.clone(), false, false, encoding_rs::UTF_8)
                .await?;
            if let Some(files) = self.cleanup.take() {
                Ok(Box::new(ScriptProcess {
                    inner,
                    files,
                    exited: false,
                    cleaned: false,
                }) as Box<dyn AgentProcess>)
            } else {
                Ok(inner)
            }
        })
    }
}
struct InlineFiles {
    session: Arc<russh_sftp::client::RawSftpSession>,
    file: String,
    root: String,
    armed: bool,
}
impl InlineFiles {
    fn clean(&self) {
        let session = self.session.clone();
        let file = self.file.clone();
        let root = self.root.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                let _ = session.remove(file).await;
                let _ = session.rmdir(root).await;
            })
            .await;
        });
    }
}
impl Drop for InlineFiles {
    fn drop(&mut self) {
        if self.armed {
            self.clean();
        }
    }
}
struct ScriptProcess {
    inner: Box<dyn AgentProcess>,
    files: InlineFiles,
    exited: bool,
    cleaned: bool,
}
impl AgentProcess for ScriptProcess {
    fn read(&mut self) -> FutureResult<'_, AgentProcessEvent> {
        Box::pin(async move {
            let event = self.inner.read().await?;
            if matches!(event, AgentProcessEvent::Exit(_)) {
                self.exited = true;
            }
            if matches!(event, AgentProcessEvent::Closed) && self.exited && !self.cleaned {
                self.cleaned = true;
                self.files.clean();
            }
            Ok(event)
        })
    }
    fn input<'a>(&'a mut self, bytes: &'a [u8]) -> FutureResult<'a, ()> {
        self.inner.input(bytes)
    }
    fn signal_stop(&mut self) -> FutureResult<'_, ()> {
        self.inner.signal_stop()
    }
    fn stop(&mut self) -> FutureResult<'_, ()> {
        self.inner.stop()
    }
}
