use std::{env, io, path::PathBuf, process::ExitCode};
use vigilshell_application::SettingsService;
use vigilshell_storage_sqlite::SqlitePreferences;
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--data-dir")) {
        return Err("Expected --data-dir".into());
    }
    let path = PathBuf::from(args.next().ok_or("Missing data directory")?);
    let terminal_pipes = match args.next() {
        None => false,
        Some(flag) if flag == "--terminal-pipes" => true,
        _ => return Err("Invalid host arguments".into()),
    };
    if !path.is_absolute() || args.next().is_some() {
        return Err("Expected an absolute data directory".into());
    }
    let runtime_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let executable = env::current_exe()?;
    let bundle = executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent());
    let bundled = bundle.is_some_and(|p| {
        matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("VigilShell.app" | "VigilShell Dev.app")
        )
    });
    let native_root = if bundled {
        bundle.unwrap().parent().ok_or("Missing native root")?
    } else {
        executable.parent().ok_or("Missing executable directory")?
    };
    let development = if bundled {
        bundle.unwrap().file_name().unwrap() == "VigilShell Dev.app"
    } else {
        cfg!(debug_assertions)
    };
    let agent_data_root = path.clone();
    vigilshell_backend::rpc::serve_with_workspace(
        io::BufReader::new(io::stdin()),
        io::stdout(),
        move || {
            SqlitePreferences::open(&path)
                .map(SettingsService::new)
                .map_err(|_| {
                    tracing::error!(
                        application = "vigilshell",
                        pid = std::process::id(),
                        stage = "storage_open",
                        outcome = "failed",
                        code = "storage_unavailable"
                    );
                    io::Error::other("storage_unavailable")
                })
        },
        runtime_id,
        vigilshell_backend::workspace::WorkspaceOptions {
            data_root: agent_data_root,
            helper: native_root.join(
                "VigilShell Credential Input.app/Contents/MacOS/vigilshell-credential-prompt",
            ),
            development,
            terminal_pipes,
        },
    )
    .await?;
    Ok(())
}
#[tokio::main]
async fn main() -> ExitCode {
    vigilshell_backend::support::diagnostics("vigilshell_backend", "vigilshell");
    tracing::info!(
        application = "vigilshell",
        pid = std::process::id(),
        stage = "startup",
        outcome = "started"
    );
    match run().await {
        Ok(()) => {
            tracing::info!(
                application = "vigilshell",
                pid = std::process::id(),
                stage = "shutdown",
                outcome = "ok"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            let code = if error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::TimedOut)
            {
                "shutdown_timeout"
            } else {
                "runtime_failed"
            };
            tracing::error!(
                application = "vigilshell",
                pid = std::process::id(),
                stage = "runtime",
                code,
                outcome = "failed"
            );
            ExitCode::FAILURE
        }
    }
}
