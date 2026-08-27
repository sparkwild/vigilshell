import { Channel, invoke } from "@tauri-apps/api/core";
import {
	desktopErrorCodes,
	type DesktopErrorCode,
	type ConfigureHumanPasswordRequest,
	type ConnectionEditorView,
	type ConfirmHostTrustRequest,
	type ConfirmHostTrustSettlement,
	type CreateConnectionProfileRequest,
	type CreateProjectRequest,
	type ProjectSummary,
	type UpdateProjectRequest,
	type DeleteProjectRequest,
	type DeleteProjectSettlement,
	type ManagedServerSummary,
	type UpdateManagedServerRequest,
	type DeleteManagedServerRequest,
	type DeleteManagedServerSettlement,
	type AddConnectionProfileToServerRequest,
	type DeleteConnectionProfileRequest,
	type DeleteConnectionProfileSettlement,
	type DiscardHostTrustRequest,
	type DiscardHostTrustSettlement,
	type GetConnectionEditorRequest,
	type GetHumanPasswordStatusRequest,
	type HostTrustProbeResult,
	type HumanPasswordSettlement,
	type HumanPasswordStatusResult,
	type HumanTerminalCloseRequest,
	type HumanTerminalCloseSettlement,
	type HumanTerminalInputRequest,
	type HumanTerminalOutputCreditRequest,
	type HumanTerminalOutputMessage,
	type HumanTerminalResizeRequest,
	type HumanTerminalSession,
	type HumanWorkbenchSnapshot,
	type OpenHumanTerminalRequest,
	type ProbeHostKeyRequest,
	type UpdateConnectionProfileRequest,
} from "./contracts";

const knownErrorCodes = new Set<string>(desktopErrorCodes);

export class DesktopAdapterError extends Error {
	readonly code: DesktopErrorCode;

	constructor(code: DesktopErrorCode) {
		super("The operation could not be completed.");
		this.name = "DesktopAdapterError";
		this.code = code;
	}
}

function errorCodeFrom(error: unknown): DesktopErrorCode {
	if (
		typeof error === "object" &&
		error !== null &&
		"code" in error &&
		typeof error.code === "string" &&
		knownErrorCodes.has(error.code)
	) {
		return error.code as DesktopErrorCode;
	}

	return "unexpected_failure";
}

async function invokeDesktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
	try {
		return await invoke<T>(command, args);
	} catch (error) {
		throw new DesktopAdapterError(errorCodeFrom(error));
	}
}

export function loadHumanWorkbench(): Promise<HumanWorkbenchSnapshot> {
	return invokeDesktop<HumanWorkbenchSnapshot>("load_human_workbench");
}

export function getConnectionEditor(
	request: GetConnectionEditorRequest,
): Promise<ConnectionEditorView> {
	return invokeDesktop<ConnectionEditorView>("get_connection_editor", { request });
}

export function createConnectionProfile(
	request: CreateConnectionProfileRequest,
): Promise<ConnectionEditorView> {
	return invokeDesktop<ConnectionEditorView>("create_connection_profile", { request });
}

export function createProject(request: CreateProjectRequest): Promise<ProjectSummary> {
	return invokeDesktop<ProjectSummary>("create_project", { request });
}

export function updateProject(request: UpdateProjectRequest): Promise<ProjectSummary> {
	return invokeDesktop<ProjectSummary>("update_project", { request });
}

export function deleteProject(request: DeleteProjectRequest): Promise<DeleteProjectSettlement> {
	return invokeDesktop<DeleteProjectSettlement>("delete_project", { request });
}

export function updateManagedServer(
	request: UpdateManagedServerRequest,
): Promise<ManagedServerSummary> {
	return invokeDesktop<ManagedServerSummary>("update_managed_server", { request });
}

export function deleteManagedServer(
	request: DeleteManagedServerRequest,
): Promise<DeleteManagedServerSettlement> {
	return invokeDesktop<DeleteManagedServerSettlement>("delete_managed_server", { request });
}

export function addConnectionProfileToServer(
	request: AddConnectionProfileToServerRequest,
): Promise<ConnectionEditorView> {
	return invokeDesktop<ConnectionEditorView>("add_connection_profile_to_server", { request });
}

export function updateConnectionProfile(
	request: UpdateConnectionProfileRequest,
): Promise<ConnectionEditorView> {
	return invokeDesktop<ConnectionEditorView>("update_connection_profile", { request });
}

export function deleteConnectionProfile(
	request: DeleteConnectionProfileRequest,
): Promise<DeleteConnectionProfileSettlement> {
	return invokeDesktop<DeleteConnectionProfileSettlement>("delete_connection_profile", {
		request,
	});
}

export function configureHumanPassword(
	request: ConfigureHumanPasswordRequest,
): Promise<HumanPasswordSettlement> {
	return invokeDesktop<HumanPasswordSettlement>("configure_human_password", {
		request,
	});
}

export function getHumanPasswordStatus(
	request: GetHumanPasswordStatusRequest,
): Promise<HumanPasswordStatusResult> {
	return invokeDesktop<HumanPasswordStatusResult>("get_human_password_status", { request });
}

export function removeHumanPassword(
	request: ConfigureHumanPasswordRequest,
): Promise<HumanPasswordSettlement> {
	return invokeDesktop<HumanPasswordSettlement>("remove_human_password", { request });
}

export function probeHostKey(request: ProbeHostKeyRequest): Promise<HostTrustProbeResult> {
	return invokeDesktop<HostTrustProbeResult>("probe_host_key", { request });
}

export function confirmHostTrust(
	request: ConfirmHostTrustRequest,
): Promise<ConfirmHostTrustSettlement> {
	return invokeDesktop<ConfirmHostTrustSettlement>("confirm_host_trust", { request });
}

export function discardHostTrust(
	request: DiscardHostTrustRequest,
): Promise<DiscardHostTrustSettlement> {
	return invokeDesktop<DiscardHostTrustSettlement>("discard_host_trust", { request });
}

export function openHumanTerminal(
	request: OpenHumanTerminalRequest,
	onOutput: (message: HumanTerminalOutputMessage) => void,
): {
	output: Channel<HumanTerminalOutputMessage>;
	settlement: Promise<HumanTerminalSession>;
} {
	const output = new Channel<HumanTerminalOutputMessage>(onOutput);
	return {
		output,
		settlement: invokeDesktop<HumanTerminalSession>("open_human_terminal", { request, output }),
	};
}

export function sendHumanTerminalInput(request: HumanTerminalInputRequest): Promise<void> {
	return invokeDesktop<void>("send_human_terminal_input", { request });
}

export function resizeHumanTerminal(request: HumanTerminalResizeRequest): Promise<void> {
	return invokeDesktop<void>("resize_human_terminal", { request });
}

export function grantHumanTerminalOutputCredit(
	request: HumanTerminalOutputCreditRequest,
): Promise<void> {
	return invokeDesktop<void>("grant_human_terminal_output_credit", { request });
}

export function closeHumanTerminal(
	request: HumanTerminalCloseRequest,
): Promise<HumanTerminalCloseSettlement> {
	return invokeDesktop<HumanTerminalCloseSettlement>("close_human_terminal", { request });
}
