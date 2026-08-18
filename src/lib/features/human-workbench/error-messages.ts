import type { DesktopErrorCode } from "$lib/desktop/contracts";
import { DesktopAdapterError } from "$lib/desktop/tauri-desktop-adapter";

const errorMessages: Record<DesktopErrorCode, string> = {
	storage_unavailable: "Local connection data is unavailable.",
	invalid_request: "The desktop request was rejected. Review the connection fields and try again.",
	connection_not_found: "The selected connection is no longer available.",
	connection_record_mismatch: "The selected connection changed. Reload the catalog and try again.",
	connection_conflict: "This connection changed elsewhere. Reload it before saving or deleting.",
	connection_delete_incomplete:
		"The password was removed, but the connection remains. Configure its password again before use.",
	version_exhausted: "This connection reached its version limit and can no longer be changed.",
	connection_disabled: "The selected connection is disabled.",
	connection_in_use: "Close this connection's active or opening terminals before deleting it.",
	connection_delete_in_progress: "This connection is being deleted. Wait for it to finish.",
	host_trust_required: "Host trust must be established before opening this connection.",
	host_trust_stale: "Host trust changed and must be reviewed before opening this connection.",
	host_trust_probe_timeout: "Host identity verification timed out. Check the connection and try again.",
	host_trust_unsupported_key:
		"The server presented a host key type that VigilShell does not support.",
	host_trust_probe_unavailable:
		"Host identity could not be verified. Check connectivity and try again.",
	host_trust_pending_capacity:
		"Too many Host Trust reviews are pending. Close another review and try again.",
	host_trust_decision_expired: "This Host Trust review expired. Probe the host again.",
	host_trust_decision_not_found:
		"This Host Trust review is no longer available. Probe the host again.",
	host_trust_state_unavailable: "Host Trust state is unavailable. Reload and try again.",
	credential_unavailable: "Credentials are unavailable for the selected connection.",
	credential_provider_unavailable:
		"The desktop credential provider is unavailable. Check system sign-in and app permissions.",
	credential_access_denied:
		"The system denied access to secure credential storage. Update OS credential access settings.",
	credential_invalid_secret: "The stored credential is invalid and could not be used.",
	terminal_global_capacity: "The terminal session limit has been reached. Close a terminal and try again.",
	terminal_profile_capacity:
		"This connection has reached its terminal session limit. Close one and try again.",
	terminal_host_key_mismatch:
		"The live host identity no longer matches current trust. Verify Host Trust again.",
	terminal_authentication_rejected:
		"The server rejected the stored password. Configure the password and try again.",
	terminal_channel_rejected: "The server rejected the terminal session channel.",
	terminal_pty_rejected: "The server rejected the terminal allocation request.",
	terminal_shell_rejected: "The server rejected the interactive shell request.",
	terminal_disconnected: "The SSH connection was interrupted.",
	terminal_timeout: "The SSH terminal operation timed out.",
	terminal_unavailable: "The SSH terminal could not be opened.",
	terminal_output_unavailable: "The terminal display channel became unavailable.",
	terminal_control_queue_full: "The terminal is busy. Input and resize controls have been paused.",
	terminal_session_closed: "The terminal session is already closed.",
	terminal_close_timeout: "The terminal did not finish closing in time.",
	application_shutting_down: "VigilShell is closing. Reopen it before starting another operation.",
	unexpected_failure: "The desktop operation could not be completed.",
};

export function desktopErrorMessage(error: unknown): string {
	if (error instanceof DesktopAdapterError) {
		return errorMessages[error.code];
	}

	return errorMessages.unexpected_failure;
}
