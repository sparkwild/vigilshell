export type TrustState = "ready" | "required" | "stale";

export type ConnectionSummary = {
	profileId: string;
	serverId: string;
	displayName: string;
	enabled: boolean;
	recordVersion: number;
	trustState: TrustState;
};

export type ConnectionEditorView = {
	profileId: string;
	serverId: string;
	displayName: string;
	host: string;
	port: number;
	username: string;
	enabled: boolean;
	recordVersion: number;
	trustState: TrustState;
};

export type GetConnectionEditorRequest = {
	profileId: string;
};

export type CreateConnectionProfileRequest = {
	displayName: string;
	host: string;
	port: number;
	username: string;
	enabled: boolean;
};

export type UpdateConnectionProfileRequest = CreateConnectionProfileRequest & {
	profileId: string;
	expectedRecordVersion: number;
};

export type DeleteConnectionProfileRequest = {
	profileId: string;
	expectedRecordVersion: number;
};

export type DeleteConnectionProfileSettlement = {
	status: "deleted";
};

export type ProbeHostKeyRequest = {
	profileId: string;
	expectedRecordVersion: number;
};

export type HostTrustProbeStatus = "first_seen" | "changed" | "matches";

export type HostTrustProbeResult = {
	profileId: string;
	probeId?: string;
	hostKeyType: string;
	observedFingerprint: string;
	previousFingerprint?: string;
	status: HostTrustProbeStatus;
};

export type ConfirmHostTrustRequest = {
	probeId: string;
};

export type ConfirmHostTrustSettlement = {
	status: "confirmed";
};

export type DiscardHostTrustRequest = {
	probeId: string;
};

export type DiscardHostTrustSettlement = {
	status: "discarded" | "already_unavailable";
};

export type HumanWorkbenchSnapshot = {
	connections: ConnectionSummary[];
	terminalAvailable: boolean;
};

export type OpenHumanTerminalRequest = {
	profileId: string;
	columns: number;
	rows: number;
};

export type HumanTerminalInputRequest = {
	sessionId: string;
	data: number[];
};

export type HumanTerminalResizeRequest = {
	sessionId: string;
	columns: number;
	rows: number;
};

export type HumanTerminalOutputCreditRequest = {
	sessionId: string;
	bytes: number;
};

export type HumanTerminalCloseRequest = {
	sessionId: string;
};

export type HumanTerminalCloseSettlement = {
	status: "closed";
};

export type HumanTerminalExitSignal =
	| "abort"
	| "alarm"
	| "floating_point_exception"
	| "hangup"
	| "illegal_instruction"
	| "interrupt"
	| "kill"
	| "pipe"
	| "quit"
	| "segmentation_violation"
	| "terminate"
	| "user_defined_one"
	| "unknown";

export type HumanTerminalLifecycle =
	| { type: "opened"; sessionId: string }
	| { type: "eof"; sessionId: string }
	| { type: "exit_status"; sessionId: string; status: number }
	| {
			type: "exit_signal";
			sessionId: string;
			signal: HumanTerminalExitSignal;
			coreDumped: boolean;
	  }
	| { type: "closed"; sessionId: string }
	| { type: "disconnected"; sessionId: string }
	| { type: "protocol_failure"; sessionId: string };

export type HumanTerminalOutputMessage = ArrayBuffer | HumanTerminalLifecycle;

export type ConfigureHumanPasswordRequest = {
	profileId: string;
};

export type GetHumanPasswordStatusRequest = {
	profileId: string;
};

export type HumanPasswordStatus = "configured" | "not_configured";

export type HumanPasswordStatusResult = {
	status: HumanPasswordStatus;
};

export type HumanPasswordSettlementStatus =
	| "configured"
	| "removed"
	| "already_missing"
	| "cancelled";

export type HumanPasswordSettlement = {
	status: HumanPasswordSettlementStatus;
};

export type HumanTerminalSession = {
	sessionId: string;
	serverId: string;
	profileId: string;
};

export const desktopErrorCodes = [
	"storage_unavailable",
	"invalid_request",
	"connection_not_found",
	"connection_record_mismatch",
	"connection_conflict",
	"connection_delete_incomplete",
	"version_exhausted",
	"connection_disabled",
	"connection_in_use",
	"connection_delete_in_progress",
	"host_trust_required",
	"host_trust_stale",
	"host_trust_probe_timeout",
	"host_trust_unsupported_key",
	"host_trust_probe_unavailable",
	"host_trust_pending_capacity",
	"host_trust_decision_expired",
	"host_trust_decision_not_found",
	"host_trust_state_unavailable",
	"credential_unavailable",
	"credential_provider_unavailable",
	"credential_access_denied",
	"credential_invalid_secret",
	"terminal_global_capacity",
	"terminal_profile_capacity",
	"terminal_host_key_mismatch",
	"terminal_authentication_rejected",
	"terminal_channel_rejected",
	"terminal_pty_rejected",
	"terminal_shell_rejected",
	"terminal_disconnected",
	"terminal_timeout",
	"terminal_unavailable",
	"terminal_output_unavailable",
	"terminal_control_queue_full",
	"terminal_session_closed",
	"terminal_close_timeout",
	"application_shutting_down",
] as const;

export type DesktopErrorCode = (typeof desktopErrorCodes)[number] | "unexpected_failure";
