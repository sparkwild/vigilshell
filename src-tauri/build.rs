fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "load_human_workbench",
        "get_connection_editor",
        "create_connection_profile",
        "update_connection_profile",
        "delete_connection_profile",
        "get_human_password_status",
        "configure_human_password",
        "remove_human_password",
        "probe_host_key",
        "confirm_host_trust",
        "discard_host_trust",
        "open_human_terminal",
        "send_human_terminal_input",
        "resize_human_terminal",
        "grant_human_terminal_output_credit",
        "close_human_terminal",
    ]);
    let attributes = tauri_build::Attributes::new().app_manifest(manifest);

    tauri_build::try_build(attributes).expect("failed to prepare the Tauri application");
}
