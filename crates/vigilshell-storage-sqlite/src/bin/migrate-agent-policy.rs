#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("Only the default Dev directory is supported".into());
    }
    let directory = std::path::PathBuf::from(std::env::var_os("HOME").ok_or("Missing home")?)
        .join("Library/Application Support/VigilShell-dev");
    vigilshell_storage_sqlite::agent_configuration_conversion::run_dev(&directory)
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Dev policy maintenance is macOS only");
    std::process::exit(1);
}
