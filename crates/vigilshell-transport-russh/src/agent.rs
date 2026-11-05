//! Agent 使用独立 SSH/PTY/SFTP 通道，与人工终端共享传输基础。
use super::*;
use vigilshell_application_contracts::ports::{AgentConnection, AgentProcess, AgentProcessEvent};
mod files;
mod native;
mod process;
mod script;
use process::Process;
mod control;
mod inspection;
mod observation;

struct Connection {
    encoding: &'static encoding_rs::Encoding,
    handle: client::Handle<Handler>,
    _guard: SocketGuard,
    listings: Mutex<std::collections::HashMap<String, files::Listing>>,
    sftp: tokio::sync::OnceCell<crate::sftp::Session>,
    previous_cpu: Mutex<Option<(u64, u64)>>,
}
pub(super) async fn connect(
    input: ConnectionInput,
    key: HostKey,
    secret: SecretMaterial,
    route: Option<ProxyRoute>,
) -> Result<Arc<dyn AgentConnection>, WorkspaceError> {
    let encoding = encoding_rs::Encoding::for_label(input.terminal.encoding.as_bytes())
        .filter(|e| e.is_ascii_compatible())
        .ok_or(WorkspaceError::UnmappableInput)?;
    let (handle, guard) = authenticated(input, key, secret, route, None, None).await?;
    Ok(Arc::new(Connection {
        encoding,
        handle,
        _guard: guard,
        sftp: tokio::sync::OnceCell::new(),
        listings: Mutex::default(),
        previous_cpu: Mutex::default(),
    }))
}
impl AgentConnection for Connection {
    fn probe_environment(
        &self,
    ) -> FutureResult<'_, vigilshell_domain::agent::ExecutionEnvironment> {
        Box::pin(self.inspect_environment())
    }

    fn execute_terminal(
        &self,
        operation: vigilshell_domain::agent::CheckedTerminal,
    ) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            let body = format!(
                "cd {} || exit 125; exec /usr/bin/env -i HOME=\"$HOME\" USER=\"$USER\" LOGNAME=\"$LOGNAME\" PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin TERM=xterm-256color LANG=C.UTF-8 /bin/sh -i",
                vigilshell_domain::agent::shell_quote(operation.directory())
            );
            let mut process = self.open_exec(body, true, true).await?;
            process
                .resize(u32::from(operation.columns()), u32::from(operation.rows()))
                .await?;
            Ok(process)
        })
    }

    fn probe_script(
        &self,
        path: String,
    ) -> FutureResult<'_, Option<vigilshell_domain::agent::ScriptInterpreter>> {
        Box::pin(self.recognize_script(path))
    }

    fn prepare_script(
        &self,
        operation: vigilshell_domain::agent::CheckedScriptOperation,
    ) -> FutureResult<'_, vigilshell_application_contracts::ports::ScriptPreparation> {
        Box::pin(self.snapshot_script(operation))
    }
    fn prepare_script_launch(
        self: Arc<Self>,
        operation: vigilshell_domain::agent::CheckedScriptLaunch,
    ) -> FutureResult<'static, Box<dyn vigilshell_application_contracts::ports::PreparedScript>>
    {
        Box::pin(self.prepare_script_launch_native(operation))
    }

    fn prepare_native(
        self: Arc<Self>,
        operation: vigilshell_domain::agent::CheckedNativeOperation,
    ) -> FutureResult<
        'static,
        Box<dyn vigilshell_application_contracts::ports::PreparedNativeOperation>,
    > {
        Box::pin(native::prepare(self, operation))
    }

    fn execute_files(
        &self,
        operation: vigilshell_domain::agent::CheckedFileOperation,
        io: vigilshell_application_contracts::ports::FileTransferHandles,
        control: vigilshell_application_contracts::ports::FileExecutionControl,
    ) -> FutureResult<'_, vigilshell_domain::agent::FileExecutionReport> {
        Box::pin(files::execute(self, operation, io, control))
    }

    fn execute_command(
        &self,
        command: vigilshell_domain::agent::CheckedCommand,
    ) -> FutureResult<'_, Box<dyn AgentProcess>> {
        Box::pin(async move {
            use vigilshell_domain::agent::shell_quote;
            let body = format!(
                "cd {} || exit 125; {}",
                shell_quote(command.directory()),
                command.shell_source()
            );
            // 账号身份不变；环境固定，避免 BASH_ENV、函数及用户 PATH 改写检查语义。
            let wire = format!(
                "exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C HOME=\"$HOME\" USER=\"$USER\" LOGNAME=\"$LOGNAME\" /bin/sh -c {}",
                shell_quote(&body)
            );
            self.open_exec(wire, false, false).await
        })
    }

    fn inspect_resources(
        &self,
        query: vigilshell_domain::agent::PolicyResourceQuery,
    ) -> FutureResult<'_, vigilshell_domain::agent::PolicyResourcePage> {
        Box::pin(async move { self.inspect(query).await })
    }
    fn encoding(&self) -> &str {
        self.encoding.name()
    }
    fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async {
            self.handle
                .disconnect(russh::Disconnect::ByApplication, "", "")
                .await
                .map_err(error)
        })
    }
}
impl Connection {
    async fn open_exec(
        &self,
        command: String,
        pty: bool,
        keep_input: bool,
    ) -> Result<Box<dyn AgentProcess>, WorkspaceError> {
        self.open_exec_encoded(command, pty, keep_input, self.encoding)
            .await
    }
    async fn open_exec_encoded(
        &self,
        command: String,
        pty: bool,
        keep_input: bool,
        encoding: &'static encoding_rs::Encoding,
    ) -> Result<Box<dyn AgentProcess>, WorkspaceError> {
        let channel = deadline(self.handle.channel_open_session(), 10).await?;
        let mut process = Process {
            channel: Some(channel),
            pending: std::collections::VecDeque::new(),
        };
        if pty {
            process
                .channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .request_pty(true, "xterm-256color", 120, 40, 0, 0, &[])
                .await
                .map_err(error)?;
            process.confirm().await?;
        }
        let (command, _, invalid) = encoding.encode(&command);
        if invalid {
            return Err(WorkspaceError::UnmappableInput);
        }
        process
            .channel
            .as_ref()
            .ok_or(WorkspaceError::Disconnected)?
            .exec(true, command.as_ref())
            .await
            .map_err(error)?;
        process.confirm().await?;
        if !keep_input {
            process
                .channel
                .as_ref()
                .ok_or(WorkspaceError::Disconnected)?
                .eof()
                .await
                .map_err(error)?;
        }
        Ok(Box::new(process) as Box<dyn AgentProcess>)
    }
}
