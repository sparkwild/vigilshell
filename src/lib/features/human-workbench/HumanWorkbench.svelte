<script lang="ts">
	import { onMount, tick } from "svelte";
	import { resetMode, setMode, userPrefersMode } from "mode-watcher";
	import { toast } from "svelte-sonner";
	import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
	import FolderPlusIcon from "@lucide/svelte/icons/folder-plus";
	import KeyRoundIcon from "@lucide/svelte/icons/key-round";
	import MonitorIcon from "@lucide/svelte/icons/monitor";
	import MoonIcon from "@lucide/svelte/icons/moon";
	import PencilIcon from "@lucide/svelte/icons/pencil";
	import PlusIcon from "@lucide/svelte/icons/plus";
	import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
	import SaveIcon from "@lucide/svelte/icons/save";
	import SearchIcon from "@lucide/svelte/icons/search";
	import ServerIcon from "@lucide/svelte/icons/server";
	import SquareTerminalIcon from "@lucide/svelte/icons/square-terminal";
	import SunIcon from "@lucide/svelte/icons/sun";
	import Trash2Icon from "@lucide/svelte/icons/trash-2";
	import XIcon from "@lucide/svelte/icons/x";
	import * as Alert from "$lib/components/ui/alert";
	import * as AlertDialog from "$lib/components/ui/alert-dialog";
	import { Badge } from "$lib/components/ui/badge";
	import { Button } from "$lib/components/ui/button";
	import * as Card from "$lib/components/ui/card";
	import * as Empty from "$lib/components/ui/empty";
	import * as Field from "$lib/components/ui/field";
	import * as InputGroup from "$lib/components/ui/input-group";
	import { Input } from "$lib/components/ui/input";
	import * as NativeSelect from "$lib/components/ui/native-select";
	import * as Resizable from "$lib/components/ui/resizable";
	import { ScrollArea } from "$lib/components/ui/scroll-area";
	import { Separator } from "$lib/components/ui/separator";
	import { Skeleton } from "$lib/components/ui/skeleton";
	import { Switch } from "$lib/components/ui/switch";
	import * as Tabs from "$lib/components/ui/tabs";
	import * as Tooltip from "$lib/components/ui/tooltip";
	import type {
		ConnectionEditorView,
		ConnectionSummary,
		HumanPasswordSettlement,
		HumanPasswordSettlementStatus,
		HumanPasswordStatus,
		HumanTerminalLifecycle,
		HumanTerminalSession,
		HumanWorkbenchSnapshot,
		HostTrustProbeResult,
		ManagedServerSummary,
		ProjectSummary,
		TrustState,
	} from "$lib/desktop/contracts";
	import {
		addConnectionProfileToServer,
		confirmHostTrust,
		configureHumanPassword,
		createConnectionProfile,
		createProject,
		deleteConnectionProfile,
		deleteManagedServer,
		deleteProject,
		DesktopAdapterError,
		discardHostTrust,
		getConnectionEditor,
		getHumanPasswordStatus,
		loadHumanWorkbench,
		probeHostKey,
		removeHumanPassword,
		updateConnectionProfile,
		updateManagedServer,
		updateProject,
	} from "$lib/desktop/tauri-desktop-adapter";
	import CatalogueTree from "$lib/features/server-workspace/CatalogueTree.svelte";
	import {
		buildCatalogueHierarchy,
		connectionLabel,
		serverForConnection,
	} from "$lib/features/server-workspace/catalogue";
	import {
		connectionSelection,
		projectSelection,
		resolveWorkspaceSelection,
		serverSelection,
		type WorkspaceInspectorKind,
		type WorkspaceSelection,
	} from "$lib/features/server-workspace/selection";
	import { desktopErrorMessage } from "./error-messages";
	import HostTrustCard from "./HostTrustCard.svelte";
	import TerminalSurface from "./TerminalSurface.svelte";
	import { humanTerminalController } from "./tauri-human-terminal-controller";

	const terminalColumns = 120;
	const terminalRows = 36;
	const controlCharacterPattern = /[\u0000-\u001f\u007f-\u009f]/;
	type ThemePreference = "system" | "light" | "dark";
	type EditorMode = "view" | "create" | "edit";
	type InspectorKind = WorkspaceInspectorKind;
	type EntityEditorMode = "view" | "create" | "edit";
	type HostTrustActionKind = "probe" | "confirm" | "discard" | null;
	type HumanPasswordStatusPhase = "idle" | "loading" | HumanPasswordStatus | "unavailable";
	type HumanTerminalPhase =
		| "connected"
		| "eof"
		| "exited"
		| "closed"
		| "disconnected"
		| "protocol_failure";
	type HumanTerminalSessionView = HumanTerminalSession & {
		phase: HumanTerminalPhase;
		closing: boolean;
		controlError: string | null;
		exitStatus?: number;
		exitSignal?: string;
		coreDumped?: boolean;
	};
	type ConnectionDraft = {
		displayName: string;
		host: string;
		port: string;
		username: string;
		enabled: boolean;
	};
	type ConnectionDraftErrors = Partial<Record<"displayName" | "host" | "port" | "username", string>>;
	type LoadWorkbenchOptions = {
		preferredProfileId?: string;
		preferredServerId?: string;
		preferredProjectId?: string;
		inspectorKind?: InspectorKind;
		selectFallback?: boolean;
	};
	type InspectorSelection = WorkspaceSelection;

	function newConnectionDraft(): ConnectionDraft {
		return { displayName: "", host: "", port: "22", username: "", enabled: true };
	}

	function draftFromEditor(editor: ConnectionEditorView): ConnectionDraft {
		return {
			displayName: editor.displayName,
			host: editor.host,
			port: String(editor.port),
			username: editor.username,
			enabled: editor.enabled,
		};
	}

	function draftsMatch(left: ConnectionDraft, right: ConnectionDraft): boolean {
		return (
			left.displayName === right.displayName &&
			left.host === right.host &&
			left.port === right.port &&
			left.username === right.username &&
			left.enabled === right.enabled
		);
	}

	function utf8Length(value: string): number {
		return new TextEncoder().encode(value).length;
	}

	function validateWorkspaceName(value: string): string | null {
		const name = value.trim();
		if (name.length === 0) {
			return "Enter a name.";
		}
		if (utf8Length(name) > 128) {
			return "Name must be at most 128 UTF-8 bytes.";
		}
		return null;
	}

	function validateConnectionDraft(draft: ConnectionDraft): ConnectionDraftErrors {
		const errors: ConnectionDraftErrors = {};
		const displayName = draft.displayName.trim();
		const host = draft.host.trim();
		const username = draft.username.trim();
		const port = Number(draft.port);

		if (displayName.length === 0) {
			errors.displayName = "Enter a display name.";
		} else if (utf8Length(displayName) > 128) {
			errors.displayName = "Name must be at most 128 UTF-8 bytes.";
		}

		if (host.length === 0) {
			errors.host = "Enter an SSH host.";
		} else if (utf8Length(host) > 255) {
			errors.host = "Host must be at most 255 UTF-8 bytes.";
		} else if (/\s/.test(host) || controlCharacterPattern.test(host)) {
			errors.host = "Host cannot contain whitespace or control characters.";
		}

		if (username.length === 0) {
			errors.username = "Enter an SSH username.";
		} else if (utf8Length(username) > 128) {
			errors.username = "Username must be at most 128 UTF-8 bytes.";
		} else if (controlCharacterPattern.test(username)) {
			errors.username = "Username cannot contain control characters.";
		}

		if (!Number.isInteger(port) || port < 1 || port > 65_535) {
			errors.port = "Port must be a whole number from 1 to 65535.";
		}

		return errors;
	}

	let snapshot = $state<HumanWorkbenchSnapshot | null>(null);
	let isLoading = $state(true);
	let loadError = $state<string | null>(null);
	let query = $state("");
	let selectedProfileId = $state("");
	let selectedServerId = $state("");
	let selectedProjectId = $state("");
	let inspectorKind = $state<InspectorKind>("connection");
	let addingToServerId = $state("");
	let sessions = $state<HumanTerminalSessionView[]>([]);
	let activeSessionId = $state("");
	let openingProfileId = $state<string | null>(null);
	let reconnectingSessionId = $state<string | null>(null);
	let credentialAction = $state<{ kind: "configure" | "remove" | null; profileId: string | null }>({
		kind: null,
		profileId: null,
	});
	let passwordStatuses = $state<Record<string, HumanPasswordStatusPhase>>({});
	let hostTrustAction = $state<{ kind: HostTrustActionKind; profileId: string | null }>({
		kind: null,
		profileId: null,
	});
	let hostTrustProbeResult = $state<HostTrustProbeResult | null>(null);
	let hostTrustError = $state<string | null>(null);
	let hostTrustNotice = $state<string | null>(null);
	let editorMode = $state<EditorMode>("view");
	let editorView = $state<ConnectionEditorView | null>(null);
	let editorDraft = $state<ConnectionDraft>(newConnectionDraft());
	let editorBaseline = $state<ConnectionDraft>(newConnectionDraft());
	let editorLoading = $state(false);
	let editorSaving = $state(false);
	let editorDeleting = $state(false);
	let editorConflict = $state(false);
	let editorError = $state<string | null>(null);
	let editorValidationVisible = $state(false);
	let deleteDialogOpen = $state(false);
	let createReturnSelection = $state<InspectorSelection | null>(null);
	let projectEditorMode = $state<EntityEditorMode>("view");
	let projectDraftName = $state("");
	let projectBaselineName = $state("");
	let projectSaving = $state(false);
	let projectError = $state<string | null>(null);
	let projectConflict = $state(false);
	let projectValidationVisible = $state(false);
	let projectDeleteDialogOpen = $state(false);
	let serverEditorMode = $state<EntityEditorMode>("view");
	let serverDraftName = $state("");
	let serverDraftProjectId = $state("");
	let serverBaselineName = $state("");
	let serverBaselineProjectId = $state("");
	let serverSaving = $state(false);
	let serverError = $state<string | null>(null);
	let serverConflict = $state(false);
	let serverValidationVisible = $state(false);
	let serverDeleteDialogOpen = $state(false);
	let editorLoadSequence = 0;
	let workbenchLoadSequence = 0;
	const passwordStatusLoadSequences = new Map<string, number>();
	let searchInput = $state<HTMLInputElement | null>(null);
	let displayNameInput = $state<HTMLInputElement | null>(null);
	let projectNameInput = $state<HTMLInputElement | null>(null);
	let serverNameInput = $state<HTMLInputElement | null>(null);
	let openTerminalButton = $state<HTMLButtonElement | null>(null);
	let workbenchMounted = false;

	let connections = $derived(snapshot?.connections ?? []);
	let projects = $derived(snapshot?.projects ?? []);
	let servers = $derived(snapshot?.servers ?? []);
	let terminalAvailable = $derived(snapshot?.terminalAvailable ?? false);
	let selectedConnection = $derived(
		connections.find((connection) => connection.profileId === selectedProfileId),
	);
	let selectedServer = $derived(servers.find((server) => server.serverId === selectedServerId));
	let selectedProject = $derived(
		projects.find((project) => project.projectId === selectedProjectId),
	);
	let selectedServerConnections = $derived(
		connections.filter((connection) => connection.serverId === selectedServerId),
	);
	let activeSession = $derived(
		sessions.find((session) => session.sessionId === activeSessionId),
	);
	let activeSessionIndex = $derived(
		sessions.findIndex((session) => session.sessionId === activeSessionId),
	);
	let normalizedQuery = $derived(query.trim().toLocaleLowerCase());
	let catalogueGroups = $derived(buildCatalogueHierarchy(snapshot, query));
	let canOpenSelected = $derived(
		Boolean(
			selectedConnection &&
				!isLoading &&
				selectedConnection.enabled &&
				selectedConnection.trustState === "ready" &&
				passwordStatusForProfile(selectedConnection.profileId) === "configured" &&
				terminalAvailable &&
				credentialAction.kind === null &&
				hostTrustAction.kind === null &&
				openingProfileId === null,
		),
	);
	let isCredentialActionInFlight = $derived(credentialAction.kind !== null);
	let isHostTrustActionInFlight = $derived(hostTrustAction.kind !== null);
	let isInteractionActionInFlight = $derived(
		isCredentialActionInFlight || isHostTrustActionInFlight,
	);
	let workspaceMutationBusy = $derived(
		isLoading ||
			editorMode === "create" ||
			projectEditorMode === "create" ||
			editorSaving ||
			editorDeleting ||
			projectSaving ||
			serverSaving ||
			isInteractionActionInFlight,
	);
	let editorDraftErrors = $derived(validateConnectionDraft(editorDraft));
	let editorDirty = $derived(
		editorMode !== "view" && !draftsMatch(editorDraft, editorBaseline),
	);
	let editorInputsDisabled = $derived(
		editorMode === "view" ||
			isLoading ||
			editorLoading ||
			editorSaving ||
			editorDeleting ||
			isHostTrustActionInFlight,
	);
	let canSaveEditor = $derived(
			editorMode !== "view" &&
			editorDirty &&
			!isLoading &&
			!editorLoading &&
			!editorSaving &&
			!editorDeleting &&
			!isHostTrustActionInFlight &&
			!editorConflict,
	);
	let editorProfileHasSession = $derived(
		Boolean(editorView && sessions.some((session) => session.profileId === editorView?.profileId)),
	);
	let projectDirty = $derived(
		projectEditorMode !== "view" && projectDraftName.trim() !== projectBaselineName,
	);
	let projectNameError = $derived(validateWorkspaceName(projectDraftName));
	let serverDirty = $derived(
		serverEditorMode !== "view" &&
			(serverDraftName.trim() !== serverBaselineName ||
				serverDraftProjectId !== serverBaselineProjectId),
	);
	let serverNameError = $derived(validateWorkspaceName(serverDraftName));
	let preferredMode = $derived(userPrefersMode.current);
	let nextMode: ThemePreference = $derived(
		preferredMode === "system" ? "light" : preferredMode === "light" ? "dark" : "system",
	);

	function editorStatusLabel(): string {
		if (editorLoading) {
			return "Loading";
		}
		if (editorSaving) {
			return "Saving";
		}
		if (editorDeleting) {
			return "Deleting";
		}
		if (editorConflict) {
			return "Conflict";
		}
		if (editorError) {
			return "Error";
		}
		if (editorDirty) {
			return "Unsaved";
		}
		if (editorMode === "create") {
			return "New draft";
		}
		return "Saved";
	}

	function editorStatusVariant(): "outline" | "secondary" | "destructive" {
		if (editorConflict || editorError) {
			return "destructive";
		}
		if (editorDirty || editorLoading || editorSaving || editorDeleting) {
			return "secondary";
		}
		return "outline";
	}

	function sessionTabLabel(session: HumanTerminalSessionView, sessionIndex: number): string {
		const label = connectionLabel(snapshot, session.profileId) ?? "Terminal";
		const matchingSessions = sessions.filter(
			(candidate) => candidate.profileId === session.profileId,
		);
		if (matchingSessions.length === 1) {
			return label;
		}

		const ordinal = sessions
			.slice(0, sessionIndex + 1)
			.filter((candidate) => candidate.profileId === session.profileId).length;
		return `${label} · ${ordinal}`;
	}

	function openButtonLabel(): string {
		if (openingProfileId !== null) {
			return "Opening…";
		}
		if (!selectedConnection) {
			return "Select a connection";
		}
		if (!selectedConnection.enabled) {
			return "Connection disabled";
		}
		if (selectedConnection.trustState === "required") {
			return "Trust required";
		}
		if (selectedConnection.trustState === "stale") {
			return "Trust review required";
		}
		const selectedPasswordStatus = passwordStatusForProfile(selectedConnection.profileId);
		if (selectedPasswordStatus === "idle" || selectedPasswordStatus === "loading") {
			return "Checking password";
		}
		if (selectedPasswordStatus === "not_configured") {
			return "Password required";
		}
		if (selectedPasswordStatus === "unavailable") {
			return "Password status unavailable";
		}
		if (!terminalAvailable) {
			return "Terminal unavailable";
		}
		return "Open terminal";
	}

	function terminalPhaseLabel(session: HumanTerminalSessionView): string {
		if (session.closing) {
			return "Closing";
		}
		switch (session.phase) {
			case "connected":
				return "Connected";
			case "eof":
				return "End of stream";
			case "exited":
				return "Remote process exited";
			case "closed":
				return "Closed";
			case "disconnected":
				return "Disconnected";
			case "protocol_failure":
				return "Protocol failure";
		}
	}

	function terminalPhaseVariant(
		session: HumanTerminalSessionView,
	): "outline" | "secondary" | "destructive" {
		if (session.controlError || session.phase === "disconnected" || session.phase === "protocol_failure") {
			return "destructive";
		}
		return session.phase === "connected" ? "secondary" : "outline";
	}

	function sessionCanReconnect(session: HumanTerminalSessionView): boolean {
		if (
			session.closing ||
			reconnectingSessionId !== null ||
			openingProfileId !== null ||
			isInteractionActionInFlight ||
			!terminalAvailable ||
			(session.phase !== "closed" &&
				session.phase !== "disconnected" &&
				session.phase !== "protocol_failure")
		) {
			return false;
		}

		const connection = connections.find((candidate) => candidate.profileId === session.profileId);
		return Boolean(
			connection?.enabled &&
				connection.trustState === "ready" &&
				passwordStatusForProfile(session.profileId) === "configured",
		);
	}

	function reconnectButtonLabel(session: HumanTerminalSessionView): string {
		if (reconnectingSessionId === session.sessionId) {
			return "Reconnecting…";
		}
		const status = passwordStatusForProfile(session.profileId);
		if (status === "idle" || status === "loading") {
			return "Checking password";
		}
		if (status === "not_configured") {
			return "Password required";
		}
		if (status === "unavailable") {
			return "Password status unavailable";
		}
		return "Reconnect";
	}

	function passwordStatusForProfile(profileId: string): HumanPasswordStatusPhase {
		return passwordStatuses[profileId] ?? "idle";
	}

	function passwordStatusLabel(phase: HumanPasswordStatusPhase): string {
		switch (phase) {
			case "idle":
			case "loading":
				return "Checking";
			case "configured":
				return "Configured";
			case "not_configured":
				return "Not configured";
			case "unavailable":
				return "Unavailable";
		}
	}

	function passwordStatusDescription(phase: HumanPasswordStatusPhase): string {
		switch (phase) {
			case "idle":
			case "loading":
				return "Checking whether a local password is configured.";
			case "configured":
				return "A local password is configured for terminal authentication.";
			case "not_configured":
				return "Configure a local password before opening or reconnecting a terminal.";
			case "unavailable":
				return "Password status is unavailable. Reload the connection before terminal access.";
		}
	}

	function passwordStatusVariant(
		phase: HumanPasswordStatusPhase,
	): "outline" | "secondary" | "destructive" {
		if (phase === "configured") {
			return "secondary";
		}
		return phase === "unavailable" ? "destructive" : "outline";
	}

	function nextPasswordStatusLoadSequence(profileId: string): number {
		const nextSequence = (passwordStatusLoadSequences.get(profileId) ?? 0) + 1;
		passwordStatusLoadSequences.set(profileId, nextSequence);
		return nextSequence;
	}

	function setPasswordStatus(profileId: string, phase: HumanPasswordStatusPhase) {
		passwordStatuses = { ...passwordStatuses, [profileId]: phase };
	}

	function settlePasswordStatus(profileId: string, phase: HumanPasswordStatusPhase) {
		nextPasswordStatusLoadSequence(profileId);
		setPasswordStatus(profileId, phase);
	}

	function forgetPasswordStatus(profileId: string) {
		nextPasswordStatusLoadSequence(profileId);
		const remainingStatuses = { ...passwordStatuses };
		delete remainingStatuses[profileId];
		passwordStatuses = remainingStatuses;
	}

	async function loadPasswordStatus(profileId: string) {
		const loadSequence = nextPasswordStatusLoadSequence(profileId);
		setPasswordStatus(profileId, "loading");
		try {
			const result = await getHumanPasswordStatus({ profileId });
			if (loadSequence === passwordStatusLoadSequences.get(profileId)) {
				setPasswordStatus(profileId, result.status);
			}
		} catch {
			if (loadSequence === passwordStatusLoadSequences.get(profileId)) {
				setPasswordStatus(profileId, "unavailable");
			}
		}
	}

	function credentialActionLabel(): string {
		if (!credentialAction.kind) {
			return "…";
		}
		return credentialAction.kind === "configure" ? "Configuring…" : "Removing…";
	}

	function handleCredentialResult(
		status: HumanPasswordSettlementStatus,
		action: "configured" | "removed",
	) {
		if (status === "configured" && action === "configured") {
			toast.success("Password configured");
			return;
		}
		if (status === "removed" && action === "removed") {
			toast.success("Password removed");
			return;
		}
		if (status === "already_missing") {
			toast.info("No password was stored");
		}
	}

	async function configurePasswordForProfile(profileId: string, afterCreate = false) {
		credentialAction = { kind: "configure", profileId };
		try {
			const settlement: HumanPasswordSettlement = await configureHumanPassword({ profileId });
			if (settlement.status === "cancelled") {
				if (afterCreate) {
					settlePasswordStatus(profileId, "not_configured");
					toast.info("Connection saved without a password. Configure it later in the credential card.");
				}
				return;
			}
			if (settlement.status === "configured") {
				settlePasswordStatus(profileId, "configured");
			}
			handleCredentialResult(settlement.status, "configured");
		} catch (error) {
			settlePasswordStatus(profileId, "unavailable");
			const message = desktopErrorMessage(error);
			toast.error(afterCreate ? `Connection saved. ${message}` : message);
		} finally {
			credentialAction = { kind: null, profileId: null };
		}
	}

	async function configureSelectedPassword() {
		if (selectedConnection && editorMode === "view" && !isInteractionActionInFlight) {
			await configurePasswordForProfile(selectedConnection.profileId);
		}
	}

	async function removeSelectedPassword() {
		const connection = selectedConnection;
		if (!connection || editorMode !== "view" || isInteractionActionInFlight) {
			return;
		}
		const profileId = connection.profileId;
		credentialAction = { kind: "remove", profileId };
		try {
			const settlement: HumanPasswordSettlement = await removeHumanPassword({ profileId });
			if (settlement.status !== "cancelled") {
				if (settlement.status === "removed" || settlement.status === "already_missing") {
					settlePasswordStatus(profileId, "not_configured");
				}
				handleCredentialResult(settlement.status, "removed");
			}
		} catch (error) {
			settlePasswordStatus(profileId, "unavailable");
			toast.error(desktopErrorMessage(error));
		} finally {
			credentialAction = { kind: null, profileId: null };
		}
	}

	function pendingHostTrustProbeId(): string | null {
		if (
			(hostTrustProbeResult?.status === "first_seen" ||
				hostTrustProbeResult?.status === "changed") &&
			hostTrustProbeResult.probeId
		) {
			return hostTrustProbeResult.probeId;
		}
		return null;
	}

	function clearHostTrustPresentation() {
		hostTrustProbeResult = null;
		hostTrustError = null;
		hostTrustNotice = null;
	}

	async function refreshHostTrustProfile(profileId: string) {
		if (selectedProfileId === profileId) {
			await loadWorkbench({ preferredProfileId: profileId, selectFallback: false });
		}
	}

	async function discardPendingHostTrust(options: { reportError: boolean; refresh: boolean }) {
		const probeId = pendingHostTrustProbeId();
		const profileId = hostTrustProbeResult?.profileId ?? selectedProfileId;
		if (!probeId) {
			clearHostTrustPresentation();
			return;
		}

		hostTrustAction = { kind: "discard", profileId };
		hostTrustProbeResult = null;
		hostTrustError = null;
		hostTrustNotice = null;
		try {
			await discardHostTrust({ probeId });
		} catch (error) {
			if (options.reportError) {
				hostTrustError = desktopErrorMessage(error);
			}
		}

		if (options.refresh && profileId) {
			await refreshHostTrustProfile(profileId);
		}
		hostTrustAction = { kind: null, profileId: null };
	}

	async function prepareForHostTrustTransition(): Promise<boolean> {
		if (isHostTrustActionInFlight) {
			return false;
		}
		await discardPendingHostTrust({ reportError: false, refresh: false });
		clearHostTrustPresentation();
		return true;
	}

	async function probeSelectedHostKey() {
		const view = editorView;
		if (
			!view ||
			editorMode !== "view" ||
			view.profileId !== selectedProfileId ||
			!view.enabled ||
			isInteractionActionInFlight ||
			editorSaving ||
			editorDeleting ||
			openingProfileId !== null
		) {
			return;
		}

		clearHostTrustPresentation();
		hostTrustAction = { kind: "probe", profileId: view.profileId };
		try {
			const result = await probeHostKey({
				profileId: view.profileId,
				expectedRecordVersion: view.recordVersion,
			});
			if (selectedProfileId !== view.profileId || editorView?.profileId !== view.profileId) {
				if (result.probeId) {
					await discardHostTrust({ probeId: result.probeId }).catch(() => undefined);
				}
				return;
			}
			if (result.profileId !== view.profileId) {
				if (result.probeId) {
					await discardHostTrust({ probeId: result.probeId }).catch(() => undefined);
				}
				throw new DesktopAdapterError("host_trust_state_unavailable");
			}
			if (result.status !== "matches" && !result.probeId) {
				throw new DesktopAdapterError("host_trust_state_unavailable");
			}

			hostTrustProbeResult = result;
			if (result.status === "matches") {
				hostTrustNotice = "This probe verified the live host identity against current trust.";
				await refreshHostTrustProfile(view.profileId);
				const refreshed = editorView;
				if (
					!refreshed ||
					refreshed.profileId !== view.profileId ||
					refreshed.recordVersion !== view.recordVersion ||
					refreshed.trustState !== "ready"
				) {
					hostTrustProbeResult = null;
					hostTrustNotice = null;
					hostTrustError = desktopErrorMessage(
						new DesktopAdapterError(
							refreshed?.profileId === view.profileId && refreshed.trustState === "stale"
								? "host_trust_stale"
								: "connection_conflict",
						),
					);
				}
			}
		} catch (error) {
			hostTrustProbeResult = null;
			hostTrustNotice = null;
			hostTrustError = desktopErrorMessage(error);
			await refreshHostTrustProfile(view.profileId);
		} finally {
			hostTrustAction = { kind: null, profileId: null };
		}
	}

	async function confirmPendingHostTrust() {
		const result = hostTrustProbeResult;
		const probeId = pendingHostTrustProbeId();
		if (!result || !probeId || isHostTrustActionInFlight) {
			return;
		}

		const profileId = result.profileId;
		hostTrustAction = { kind: "confirm", profileId };
		hostTrustProbeResult = null;
		hostTrustError = null;
		hostTrustNotice = null;
		try {
			await confirmHostTrust({ probeId });
			hostTrustNotice = "Host Trust confirmed for the current saved connection.";
		} catch (error) {
			hostTrustError = desktopErrorMessage(error);
		} finally {
			await refreshHostTrustProfile(profileId);
			hostTrustAction = { kind: null, profileId: null };
		}
	}

	async function discardPendingHostTrustFromDialog() {
		if (!isHostTrustActionInFlight) {
			await discardPendingHostTrust({ reportError: true, refresh: true });
		}
	}

	function modeLabel(mode: ThemePreference): string {
		return mode[0].toLocaleUpperCase() + mode.slice(1);
	}

	function cycleMode() {
		if (preferredMode === "system") {
			setMode("light");
		} else if (preferredMode === "light") {
			setMode("dark");
		} else {
			resetMode();
		}
	}

	async function clearConnectionSearch() {
		query = "";
		await tick();
		searchInput?.focus();
	}

	function clearEditorView() {
		editorLoadSequence += 1;
		clearHostTrustPresentation();
		editorMode = "view";
		editorView = null;
		editorDraft = newConnectionDraft();
		editorBaseline = newConnectionDraft();
		editorLoading = false;
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		deleteDialogOpen = false;
	}

	function applyEditorView(view: ConnectionEditorView) {
		const draft = draftFromEditor(view);
		editorMode = "view";
		editorView = view;
		editorDraft = draft;
		editorBaseline = { ...draft };
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		deleteDialogOpen = false;
	}

	async function loadEditor(profileId: string) {
		const loadSequence = ++editorLoadSequence;
		settlePasswordStatus(profileId, "idle");
		editorMode = "view";
		editorView = null;
		editorLoading = true;
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		deleteDialogOpen = false;

		try {
			const view = await getConnectionEditor({ profileId });
			if (loadSequence === editorLoadSequence && selectedProfileId === profileId) {
				applyEditorView(view);
				void loadPasswordStatus(profileId);
			}
		} catch (error) {
			if (loadSequence === editorLoadSequence && selectedProfileId === profileId) {
				settlePasswordStatus(profileId, "unavailable");
				editorError = desktopErrorMessage(error);
			}
		} finally {
			if (loadSequence === editorLoadSequence) {
				editorLoading = false;
			}
		}
	}

	function captureInspectorSelection(): InspectorSelection {
		return {
			kind: inspectorKind,
			projectId: selectedProjectId,
			serverId: selectedServerId,
			profileId: selectedProfileId,
		};
	}

	function resetProjectEditorState() {
		projectEditorMode = "view";
		projectDraftName = selectedProject?.displayName ?? "";
		projectBaselineName = selectedProject?.displayName ?? "";
		projectError = null;
		projectConflict = false;
		projectValidationVisible = false;
	}

	function resetServerEditorState() {
		serverEditorMode = "view";
		serverDraftName = selectedServer?.displayName ?? "";
		serverBaselineName = selectedServer?.displayName ?? "";
		serverDraftProjectId = selectedServer?.projectId ?? "";
		serverBaselineProjectId = selectedServer?.projectId ?? "";
		serverError = null;
		serverConflict = false;
		serverValidationVisible = false;
	}

	function discardDraftsForNavigation() {
		if (editorDirty || projectDirty || serverDirty) {
			toast.info("Unsaved changes were discarded.");
		}
		resetProjectEditorState();
		resetServerEditorState();
	}

	function restoreInspectorSelection(selection: InspectorSelection | null) {
		createReturnSelection = null;
		projectEditorMode = "view";
		projectValidationVisible = false;
		serverEditorMode = "view";
		serverValidationVisible = false;
		const resolvedSelection = snapshot
			? resolveWorkspaceSelection(snapshot, selection)
			: null;
		if (!resolvedSelection) {
			inspectorKind = "connection";
			selectedProjectId = "";
			selectedServerId = "";
			selectedProfileId = "";
			clearEditorView();
			return;
		}

		if (resolvedSelection.kind === "connection") {
			const connection = connections.find(
				(candidate) => candidate.profileId === resolvedSelection.profileId,
			);
			if (connection) {
				inspectorKind = "connection";
				selectedProfileId = connection.profileId;
				selectedServerId = connection.serverId;
				selectedProjectId =
					servers.find((server) => server.serverId === connection.serverId)?.projectId ?? "";
				void loadEditor(connection.profileId);
				return;
			}
		}

		if (resolvedSelection.kind === "server") {
			const server = servers.find(
				(candidate) => candidate.serverId === resolvedSelection.serverId,
			);
			if (server) {
				inspectorKind = "server";
				selectedProjectId = server.projectId ?? "";
				selectedServerId = server.serverId;
				selectedProfileId = "";
				resetServerEditorState();
				clearEditorView();
				return;
			}
		}

		if (resolvedSelection.kind === "project") {
			const project = projects.find(
				(candidate) => candidate.projectId === resolvedSelection.projectId,
			);
			if (project) {
				inspectorKind = "project";
				selectedProjectId = project.projectId;
				selectedServerId = "";
				selectedProfileId = "";
				resetProjectEditorState();
				clearEditorView();
				return;
			}
		}

		inspectorKind = "connection";
		selectedProjectId = "";
		selectedServerId = "";
		selectedProfileId = "";
		clearEditorView();
	}

	async function loadWorkbench(options: LoadWorkbenchOptions = {}) {
		const loadSequence = ++workbenchLoadSequence;
		const preferredProfileId = options.preferredProfileId ?? selectedProfileId;
		const preferredServerId = options.preferredServerId ?? selectedServerId;
		const preferredProjectId = options.preferredProjectId ?? selectedProjectId;
		const preferredInspectorKind = options.inspectorKind ?? inspectorKind;
		const selectFallback = options.selectFallback ?? true;
		isLoading = true;
		loadError = null;

		try {
			const nextSnapshot = await loadHumanWorkbench();
			if (loadSequence !== workbenchLoadSequence) {
				return false;
			}
			snapshot = nextSnapshot;
			if (preferredInspectorKind === "project") {
				const project = nextSnapshot.projects.find(
					(candidate) => candidate.projectId === preferredProjectId,
				);
				if (project) {
					inspectorKind = "project";
					selectedProjectId = project.projectId;
					selectedServerId = "";
					selectedProfileId = "";
					projectEditorMode = "view";
					projectDraftName = project.displayName;
					projectBaselineName = project.displayName;
					clearEditorView();
					return true;
				}
			}
			if (preferredInspectorKind === "server") {
				const server = nextSnapshot.servers.find(
					(candidate) => candidate.serverId === preferredServerId,
				);
				if (server) {
					inspectorKind = "server";
					selectedServerId = server.serverId;
					selectedProjectId = server.projectId ?? "";
					selectedProfileId = "";
					serverEditorMode = "view";
					serverDraftName = server.displayName;
					serverBaselineName = server.displayName;
					serverDraftProjectId = server.projectId ?? "";
					serverBaselineProjectId = server.projectId ?? "";
					clearEditorView();
					return true;
				}
			}
			const preferredConnection = nextSnapshot.connections.find(
				(connection) => connection.profileId === preferredProfileId,
			);
			const fallbackConnection = selectFallback
				? (nextSnapshot.connections.find(
						(connection) => connection.enabled && connection.trustState === "ready",
					) ?? nextSnapshot.connections[0])
				: undefined;
			const nextSelection = preferredConnection?.profileId ?? fallbackConnection?.profileId ?? "";
			inspectorKind = "connection";
			selectedProfileId = nextSelection;
			const nextServer = serverForConnection(nextSnapshot, nextSelection);
			selectedServerId = nextServer?.serverId ?? "";
			selectedProjectId = nextServer?.projectId ?? "";
			if (nextSelection) {
				await loadEditor(nextSelection);
			} else {
				clearEditorView();
			}
			return true;
		} catch (error) {
			if (loadSequence !== workbenchLoadSequence) {
				return false;
			}
			snapshot = null;
			selectedProfileId = "";
			selectedServerId = "";
			selectedProjectId = "";
			inspectorKind = "connection";
			clearEditorView();
			loadError = desktopErrorMessage(error);
			return false;
		} finally {
			if (loadSequence === workbenchLoadSequence) {
				isLoading = false;
			}
		}
	}

	async function selectConnection(profileId: string) {
		if (
			workspaceMutationBusy ||
			(profileId === selectedProfileId && inspectorKind === "connection")
		) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		const nextSelection = snapshot ? connectionSelection(snapshot, profileId) : null;
		if (!nextSelection) {
			return;
		}
		discardDraftsForNavigation();
		createReturnSelection = null;
		addingToServerId = "";
		inspectorKind = "connection";
		selectedProfileId = nextSelection.profileId;
		selectedServerId = nextSelection.serverId;
		selectedProjectId = nextSelection.projectId;
		void loadEditor(nextSelection.profileId);
	}

	async function selectProject(projectId: string) {
		if (workspaceMutationBusy) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		const nextSelection = snapshot ? projectSelection(snapshot, projectId) : null;
		const project = projects.find((candidate) => candidate.projectId === nextSelection?.projectId);
		if (!nextSelection || !project) {
			return;
		}
		discardDraftsForNavigation();
		inspectorKind = "project";
		selectedProjectId = nextSelection.projectId;
		selectedServerId = nextSelection.serverId;
		selectedProfileId = nextSelection.profileId;
		projectEditorMode = "view";
		projectDraftName = project.displayName;
		projectBaselineName = project.displayName;
		projectError = null;
		projectConflict = false;
		clearEditorView();
	}

	async function selectServer(serverId: string) {
		if (workspaceMutationBusy) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		const nextSelection = snapshot ? serverSelection(snapshot, serverId) : null;
		const server = servers.find((candidate) => candidate.serverId === nextSelection?.serverId);
		if (!nextSelection || !server) {
			return;
		}
		discardDraftsForNavigation();
		inspectorKind = "server";
		selectedServerId = nextSelection.serverId;
		selectedProjectId = nextSelection.projectId;
		selectedProfileId = nextSelection.profileId;
		serverEditorMode = "view";
		serverDraftName = server.displayName;
		serverBaselineName = server.displayName;
		serverDraftProjectId = server.projectId ?? "";
		serverBaselineProjectId = server.projectId ?? "";
		serverError = null;
		serverConflict = false;
		clearEditorView();
	}

	async function beginProjectCreate() {
		if (workspaceMutationBusy) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		createReturnSelection = captureInspectorSelection();
		discardDraftsForNavigation();
		inspectorKind = "project";
		selectedProjectId = "";
		selectedServerId = "";
		selectedProfileId = "";
		projectEditorMode = "create";
		projectDraftName = "";
		projectBaselineName = "";
		projectError = null;
		projectConflict = false;
		projectValidationVisible = false;
		clearEditorView();
		await tick();
		projectNameInput?.focus();
	}

	async function beginProjectEdit() {
		if (!selectedProject || workspaceMutationBusy) {
			return;
		}
		projectEditorMode = "edit";
		projectDraftName = selectedProject.displayName;
		projectBaselineName = selectedProject.displayName;
		projectError = null;
		projectConflict = false;
		projectValidationVisible = false;
		await tick();
		projectNameInput?.focus();
	}

	async function cancelProjectEditor() {
		if (projectSaving || isLoading) {
			return;
		}
		if (projectEditorMode === "create") {
			restoreInspectorSelection(createReturnSelection);
			await tick();
			searchInput?.focus();
			return;
		}
		projectEditorMode = "view";
		projectDraftName = selectedProject?.displayName ?? "";
		projectBaselineName = selectedProject?.displayName ?? "";
		projectError = null;
		projectConflict = false;
		projectValidationVisible = false;
		await tick();
		searchInput?.focus();
	}

	async function saveProjectEditor() {
		const displayName = projectDraftName.trim();
		if (isLoading || projectSaving || !projectDirty || projectConflict) {
			return;
		}
		projectValidationVisible = true;
		if (projectNameError) {
			return;
		}
		projectSaving = true;
		projectError = null;
		try {
			const project =
				projectEditorMode === "create"
					? await createProject({ displayName })
					: selectedProject
						? await updateProject({
								projectId: selectedProject.projectId,
								expectedRecordVersion: selectedProject.recordVersion,
								displayName,
							})
						: null;
			if (!project) {
				return;
			}
			createReturnSelection = null;
			toast.success(projectEditorMode === "create" ? "Project created" : "Project saved");
			await loadWorkbench({
				preferredProjectId: project.projectId,
				inspectorKind: "project",
				selectFallback: false,
			});
		} catch (error) {
			projectConflict = requiresWorkspaceReload(error);
			projectError = desktopErrorMessage(error);
		} finally {
			projectSaving = false;
		}
	}

	async function deleteSelectedProject() {
		if (!selectedProject || projectSaving) {
			return;
		}
		projectSaving = true;
		try {
			await deleteProject({
				projectId: selectedProject.projectId,
				expectedRecordVersion: selectedProject.recordVersion,
			});
			toast.success(`Deleted ${selectedProject.displayName}`);
			selectedProjectId = "";
			inspectorKind = "connection";
			await loadWorkbench({ selectFallback: true, inspectorKind: "connection" });
		} catch (error) {
			projectConflict = requiresWorkspaceReload(error);
			projectError = desktopErrorMessage(error);
			toast.error(projectError);
		} finally {
			projectSaving = false;
			projectDeleteDialogOpen = false;
		}
	}

	async function beginServerEdit() {
		if (!selectedServer || workspaceMutationBusy) {
			return;
		}
		serverEditorMode = "edit";
		serverDraftName = selectedServer.displayName;
		serverBaselineName = selectedServer.displayName;
		serverDraftProjectId = selectedServer.projectId ?? "";
		serverBaselineProjectId = selectedServer.projectId ?? "";
		serverError = null;
		serverConflict = false;
		serverValidationVisible = false;
		await tick();
		serverNameInput?.focus();
	}

	async function cancelServerEditor() {
		if (serverSaving || isLoading) {
			return;
		}
		serverEditorMode = "view";
		serverDraftName = selectedServer?.displayName ?? "";
		serverBaselineName = selectedServer?.displayName ?? "";
		serverDraftProjectId = selectedServer?.projectId ?? "";
		serverBaselineProjectId = selectedServer?.projectId ?? "";
		serverError = null;
		serverConflict = false;
		serverValidationVisible = false;
		await tick();
		searchInput?.focus();
	}

	async function saveServerEditor() {
		const displayName = serverDraftName.trim();
		if (!selectedServer || isLoading || serverSaving || !serverDirty || serverConflict) {
			return;
		}
		serverValidationVisible = true;
		if (serverNameError) {
			return;
		}
		serverSaving = true;
		serverError = null;
		try {
			const server = await updateManagedServer({
				serverId: selectedServer.serverId,
				expectedRecordVersion: selectedServer.recordVersion,
				projectId: serverDraftProjectId || null,
				displayName,
			});
			toast.success("Server saved");
			await loadWorkbench({
				preferredServerId: server.serverId,
				preferredProfileId: selectedProfileId,
				inspectorKind: "server",
				selectFallback: false,
			});
		} catch (error) {
			serverConflict = requiresWorkspaceReload(error);
			serverError = desktopErrorMessage(error);
		} finally {
			serverSaving = false;
		}
	}

	async function deleteSelectedServer() {
		if (!selectedServer || serverSaving || selectedServerConnections.length > 0) {
			return;
		}
		serverSaving = true;
		try {
			await deleteManagedServer({
				serverId: selectedServer.serverId,
				expectedRecordVersion: selectedServer.recordVersion,
			});
			toast.success(`Deleted ${selectedServer.displayName}`);
			selectedServerId = "";
			inspectorKind = "connection";
			await loadWorkbench({ selectFallback: true, inspectorKind: "connection" });
		} catch (error) {
			serverConflict = requiresWorkspaceReload(error);
			serverError = desktopErrorMessage(error);
			toast.error(serverError);
		} finally {
			serverSaving = false;
			serverDeleteDialogOpen = false;
		}
	}

	async function beginCreate(serverId = "") {
		if (workspaceMutationBusy) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		createReturnSelection = captureInspectorSelection();
		discardDraftsForNavigation();
		addingToServerId = serverId;
		inspectorKind = "connection";
		if (serverId) {
			selectedServerId = serverId;
			selectedProjectId = servers.find((server) => server.serverId === serverId)?.projectId ?? "";
		} else {
			selectedServerId = "";
			selectedProjectId = "";
		}
		selectedProfileId = "";
		editorLoadSequence += 1;
		editorMode = "create";
		editorView = null;
		editorDraft = newConnectionDraft();
		editorBaseline = newConnectionDraft();
		editorLoading = false;
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		deleteDialogOpen = false;
		await tick();
		displayNameInput?.focus();
	}

	async function beginEdit() {
		if (
			!editorView ||
			isLoading ||
			editorLoading ||
			editorSaving ||
			editorDeleting ||
			isInteractionActionInFlight
		) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		const draft = draftFromEditor(editorView);
		editorMode = "edit";
		editorDraft = draft;
		editorBaseline = { ...draft };
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		await tick();
		displayNameInput?.focus();
	}

	async function cancelEditor() {
		if (editorMode === "create") {
			addingToServerId = "";
			restoreInspectorSelection(createReturnSelection);
			await tick();
			searchInput?.focus();
			return;
		}

		editorDraft = { ...editorBaseline };
		editorMode = "view";
		editorConflict = false;
		editorError = null;
		editorValidationVisible = false;
		await tick();
		searchInput?.focus();
	}

	function isConflictError(error: unknown): boolean {
		return error instanceof DesktopAdapterError && error.code === "connection_conflict";
	}

	function requiresWorkspaceReload(error: unknown): boolean {
		return (
			error instanceof DesktopAdapterError &&
			(error.code === "workspace_conflict" ||
				error.code === "workspace_record_mismatch" ||
				error.code === "project_not_found" ||
				error.code === "managed_server_not_found")
		);
	}

	function reloadSelectedProject() {
		if (selectedProjectId) {
			void loadWorkbench({
				preferredProjectId: selectedProjectId,
				inspectorKind: "project",
				selectFallback: false,
			});
		}
	}

	function reloadSelectedServer() {
		if (selectedServerId) {
			void loadWorkbench({
				preferredServerId: selectedServerId,
				inspectorKind: "server",
				selectFallback: false,
			});
		}
	}

	function reloadSelectedEditor() {
		const profileId = editorView?.profileId ?? selectedProfileId;
		if (profileId) {
			void loadEditor(profileId);
		}
	}

	async function saveEditor() {
		if (
			editorMode === "view" ||
			isLoading ||
			!editorDirty ||
			editorSaving ||
			editorDeleting ||
			isHostTrustActionInFlight ||
			editorConflict
		) {
			return;
		}

		editorValidationVisible = true;
		if (Object.keys(editorDraftErrors).length > 0) {
			return;
		}

		const request = {
			displayName: editorDraft.displayName.trim(),
			host: editorDraft.host.trim(),
			port: Number(editorDraft.port),
			username: editorDraft.username.trim(),
			enabled: editorDraft.enabled,
		};
		editorSaving = true;
		editorError = null;

		try {
			if (editorMode === "create") {
				const created = addingToServerId
					? await addConnectionProfileToServer({ ...request, serverId: addingToServerId })
					: await createConnectionProfile(request);
				addingToServerId = "";
				createReturnSelection = null;
				editorSaving = false;
				selectedProfileId = created.profileId;
				applyEditorView(created);
				toast.success("Connection created");
				await loadWorkbench({ preferredProfileId: created.profileId, selectFallback: false });
				await configurePasswordForProfile(created.profileId, true);
				return;
			}

			if (!editorView) {
				return;
			}
			const updated = await updateConnectionProfile({
				...request,
				profileId: editorView.profileId,
				expectedRecordVersion: editorView.recordVersion,
			});
			editorSaving = false;
			applyEditorView(updated);
			await loadWorkbench({ preferredProfileId: updated.profileId, selectFallback: false });
			toast.success("Connection saved");
		} catch (error) {
			editorConflict = isConflictError(error);
			editorError = desktopErrorMessage(error);
		} finally {
			editorSaving = false;
		}
	}

	function removeSessionsForProfile(profileId: string) {
		const removedSessionIds = sessions
			.filter((session) => session.profileId === profileId)
			.map((session) => session.sessionId);
		const removedActiveSession = sessions.some(
			(session) => session.profileId === profileId && session.sessionId === activeSessionId,
		);
		sessions = sessions.filter((session) => session.profileId !== profileId);
		for (const sessionId of removedSessionIds) {
			humanTerminalController.forget(sessionId);
		}
		if (removedActiveSession) {
			activeSessionId = sessions[0]?.sessionId ?? "";
		}
	}

	async function deleteEditorConnection() {
		const view = editorView;
		if (!view || editorDeleting || isInteractionActionInFlight) {
			return;
		}
		if (editorProfileHasSession) {
			toast.info("Close this connection's terminal tabs before deleting it.");
			deleteDialogOpen = false;
			return;
		}

		editorDeleting = true;
		editorConflict = false;
		editorError = null;
		try {
			await deleteConnectionProfile({
				profileId: view.profileId,
				expectedRecordVersion: view.recordVersion,
			});
			forgetPasswordStatus(view.profileId);
			removeSessionsForProfile(view.profileId);
			selectedProfileId = "";
			clearEditorView();
			toast.success(`Deleted ${view.displayName}`);
			await loadWorkbench({
				preferredServerId: view.serverId,
				preferredProfileId: "",
				inspectorKind: "server",
				selectFallback: false,
			});
			await tick();
			searchInput?.focus();
		} catch (error) {
			if (error instanceof DesktopAdapterError && error.code === "connection_delete_incomplete") {
				settlePasswordStatus(view.profileId, "not_configured");
			}
			editorConflict = isConflictError(error);
			editorError = desktopErrorMessage(error);
			toast.error(editorError);
		} finally {
			editorDeleting = false;
			deleteDialogOpen = false;
		}
	}

	function updateTerminalSession(
		sessionId: string,
		update: (session: HumanTerminalSessionView) => HumanTerminalSessionView,
	) {
		if (!workbenchMounted) {
			return;
		}
		sessions = sessions.map((session) => (session.sessionId === sessionId ? update(session) : session));
	}

	function handleTerminalLifecycle(event: HumanTerminalLifecycle) {
		updateTerminalSession(event.sessionId, (session) => {
			switch (event.type) {
				case "opened":
					return { ...session, phase: "connected" };
				case "eof":
					return { ...session, phase: "eof" };
				case "exit_status":
					return { ...session, phase: "exited", exitStatus: event.status };
				case "exit_signal":
					return {
						...session,
						phase: "exited",
						exitSignal: event.signal,
						coreDumped: event.coreDumped,
					};
				case "closed":
					return { ...session, phase: "closed" };
				case "disconnected":
					return { ...session, phase: "disconnected" };
				case "protocol_failure":
					return { ...session, phase: "protocol_failure" };
			}
		});
	}

	function handleTerminalControlError(sessionId: string, error: unknown) {
		updateTerminalSession(sessionId, (session) => ({
			...session,
			controlError: desktopErrorMessage(error),
		}));
	}

	async function openTerminalForProfile(
		profileId: string,
		dimensions: { columns: number; rows: number } = {
			columns: terminalColumns,
			rows: terminalRows,
		},
	): Promise<HumanTerminalSessionView | null> {
		openingProfileId = profileId;
		try {
			let observedSessionId = "";
			const session = await humanTerminalController.open(
				{
					profileId,
					columns: dimensions.columns,
					rows: dimensions.rows,
				},
				{
					onLifecycle: handleTerminalLifecycle,
					onError: (error) => {
						if (observedSessionId) {
							handleTerminalControlError(observedSessionId, error);
						}
					},
				},
			);
			observedSessionId = session.sessionId;
			const sessionView: HumanTerminalSessionView = {
				...session,
				phase: "connected",
				closing: false,
				controlError: null,
			};
			sessions = [...sessions, sessionView];
			activeSessionId = session.sessionId;
			humanTerminalController.activate(session.sessionId);
			return sessionView;
		} catch (error) {
			toast.error(desktopErrorMessage(error));
			return null;
		} finally {
			openingProfileId = null;
		}
	}

	async function openSelectedTerminal() {
		const connection = selectedConnection;
		if (!connection || !canOpenSelected) {
			return;
		}
		await openTerminalForProfile(connection.profileId);
	}

	async function reconnectSession(sessionId: string) {
		const session = sessions.find((candidate) => candidate.sessionId === sessionId);
		if (!session || !sessionCanReconnect(session)) {
			return;
		}

		reconnectingSessionId = sessionId;
		const dimensions = humanTerminalController.dimensions(sessionId) ?? {
			columns: terminalColumns,
			rows: terminalRows,
		};
		try {
			const replacement = await openTerminalForProfile(session.profileId, dimensions);
			if (replacement) {
				await closeSession(sessionId, false);
			}
		} finally {
			reconnectingSessionId = null;
		}
	}

	async function openDeleteConnectionDialog() {
		if (
			!editorView ||
			editorMode !== "view" ||
			editorDeleting ||
			isInteractionActionInFlight
		) {
			return;
		}
		if (!(await prepareForHostTrustTransition())) {
			return;
		}
		deleteDialogOpen = true;
	}

	async function closeSession(sessionId: string, restoreFocus = true) {
		const sessionIndex = sessions.findIndex((session) => session.sessionId === sessionId);
		if (sessionIndex === -1) {
			return;
		}

		updateTerminalSession(sessionId, (session) => ({ ...session, closing: true, controlError: null }));
		try {
			await humanTerminalController.close(sessionId);
			const remainingSessions = sessions.filter((session) => session.sessionId !== sessionId);
			sessions = remainingSessions;
			if (activeSessionId === sessionId) {
				activeSessionId =
					remainingSessions[Math.min(sessionIndex, remainingSessions.length - 1)]?.sessionId ?? "";
			}
			await tick();
			humanTerminalController.forget(sessionId);

			if (restoreFocus && remainingSessions.length === 0) {
				if (openTerminalButton && !openTerminalButton.disabled) {
					openTerminalButton.focus();
				} else {
					searchInput?.focus();
				}
			}
		} catch (error) {
			updateTerminalSession(sessionId, (session) => ({
				...session,
				closing: false,
				controlError: desktopErrorMessage(error),
			}));
			toast.error(desktopErrorMessage(error));
		}
	}

	onMount(() => {
		workbenchMounted = true;
		void loadWorkbench();
		return () => {
			workbenchMounted = false;
			humanTerminalController.closeAllBestEffort();
			const probeId = pendingHostTrustProbeId();
			if (probeId) {
				void discardHostTrust({ probeId }).catch(() => undefined);
			}
		};
	});
</script>

<div class="flex h-full min-h-0 flex-col bg-background text-foreground">
	<main class="min-h-0 flex-1" aria-label="Workbench">
		<Resizable.PaneGroup direction="horizontal">
			<Resizable.Pane defaultSize={20} minSize={17} class="min-w-0">
				<section class="flex h-full min-h-0 flex-col" aria-label="Connections">
					<div class="shrink-0 p-3">
						<InputGroup.Root>
							<InputGroup.Input
								bind:ref={searchInput}
								bind:value={query}
								type="search"
								placeholder="Search"
								aria-label="Search projects, servers, and connections by name"
							/>
							<InputGroup.Addon>
								<SearchIcon />
							</InputGroup.Addon>
							{#if query.length > 0}
								<InputGroup.Addon align="inline-end">
									<InputGroup.Button
										size="icon-xs"
										onclick={() => void clearConnectionSearch()}
										aria-label="Clear connection search"
									>
										<XIcon data-icon="inline-start" />
									</InputGroup.Button>
								</InputGroup.Addon>
							{/if}
						</InputGroup.Root>
					</div>

					<ScrollArea class="min-h-0 flex-1">
						<div class="flex flex-col gap-1 px-2 pb-3">
							{#if isLoading}
								<div role="status" aria-label="Loading connections">
									{#each Array(5) as _, index (index)}
										<div class="flex items-center gap-2 px-2 py-2">
											<Skeleton class="size-2 shrink-0 rounded-full" />
											<div class="flex min-w-0 flex-1 flex-col gap-1.5">
												<Skeleton class="h-3 w-3/5" />
												<Skeleton class="h-2.5 w-4/5" />
											</div>
											<Skeleton class="h-5 w-14 rounded-full" />
										</div>
									{/each}
								</div>
							{:else if loadError}
								<Empty.Root>
									<Empty.Header>
										<Empty.Media variant="icon"><ServerIcon /></Empty.Media>
										<Empty.Title>Connections unavailable</Empty.Title>
										<Empty.Description>{loadError}</Empty.Description>
									</Empty.Header>
									<Empty.Content>
										<Button variant="outline" size="sm" onclick={() => void loadWorkbench()}>
											Reload
										</Button>
									</Empty.Content>
								</Empty.Root>
							{:else if catalogueGroups.length === 0}
								<Empty.Root>
									<Empty.Header>
										<Empty.Media variant="icon"><SearchIcon /></Empty.Media>
										<Empty.Title>Nothing found</Empty.Title>
										<Empty.Description>
											{normalizedQuery
												? "No project, server, or connection name matches this search."
												: "Create a connection to begin."}
										</Empty.Description>
									</Empty.Header>
									{#if normalizedQuery.length === 0}
										<Empty.Content>
											<Button
												disabled={workspaceMutationBusy}
												onclick={() => void beginCreate()}
											>
												<PlusIcon data-icon="inline-start" />
												New connection
											</Button>
										</Empty.Content>
									{/if}
								</Empty.Root>
							{:else}
								{#key normalizedQuery}
									<CatalogueTree
										groups={catalogueGroups}
										{selectedProjectId}
										{selectedServerId}
										{selectedProfileId}
										disabled={workspaceMutationBusy}
										onselectproject={(projectId) => void selectProject(projectId)}
										onselectserver={(serverId) => void selectServer(serverId)}
										onselectconnection={(profileId) => void selectConnection(profileId)}
									/>
								{/key}
							{/if}
						</div>
					</ScrollArea>
				</section>
			</Resizable.Pane>

			<Resizable.Handle withHandle />

			<Resizable.Pane defaultSize={57} minSize={40} class="min-w-0">
				<section class="flex h-full min-h-0 flex-col" aria-label="Terminal workspace">
					{#if sessions.length === 0}
						<Empty.Root class="min-h-0 flex-1">
							<Empty.Header>
								<Empty.Media variant="icon"><SquareTerminalIcon /></Empty.Media>
								<Empty.Title>No terminal open</Empty.Title>
								<Empty.Description>
									Open a trusted connection to start a live SSH terminal.
								</Empty.Description>
							</Empty.Header>
						</Empty.Root>
					{:else}
						<Tabs.Root bind:value={activeSessionId} class="min-h-0 flex-1 gap-0">
							<div class="flex h-9 shrink-0 items-center gap-1 px-2">
								<ScrollArea orientation="horizontal" class="min-w-0 flex-1">
									<Tabs.List variant="line" class="w-max">
										{#each sessions as session, sessionIndex (session.sessionId)}
											<Tabs.Trigger value={session.sessionId}>
												{sessionTabLabel(session, sessionIndex)}
											</Tabs.Trigger>
										{/each}
									</Tabs.List>
								</ScrollArea>
								{#if activeSession}
									<Badge variant={terminalPhaseVariant(activeSession)}>
										{terminalPhaseLabel(activeSession)}
									</Badge>
									{#if activeSession.phase === "closed" ||
									activeSession.phase === "disconnected" ||
									activeSession.phase === "protocol_failure"}
										<Button
											variant="outline"
											size="sm"
											disabled={!sessionCanReconnect(activeSession)}
											onclick={() => void reconnectSession(activeSession.sessionId)}
										>
											<RefreshCwIcon data-icon="inline-start" />
											{reconnectButtonLabel(activeSession)}
										</Button>
									{/if}
									<Tooltip.Root>
										<Tooltip.Trigger>
											{#snippet child({ props })}
												<Button
													{...props}
													variant="ghost"
													size="icon-sm"
													disabled={activeSession.closing}
													onclick={() => void closeSession(activeSession.sessionId)}
													aria-label={`Close terminal ${sessionTabLabel(activeSession, activeSessionIndex)}`}
												>
													<XIcon data-icon="inline-start" />
												</Button>
											{/snippet}
										</Tooltip.Trigger>
										<Tooltip.Content>Close active terminal</Tooltip.Content>
									</Tooltip.Root>
								{/if}
							</div>
							<Separator />
							{#each sessions as session (session.sessionId)}
								<Tabs.Content value={session.sessionId} class="m-0 min-h-0 overflow-hidden">
									<TerminalSurface
										sessionId={session.sessionId}
										label={`Terminal for ${connectionLabel(snapshot, session.profileId) ?? "connection"}`}
										active={session.sessionId === activeSessionId}
										interactive={session.phase === "connected" &&
											!session.closing &&
											!session.controlError}
									/>
								</Tabs.Content>
							{/each}
						</Tabs.Root>
					{/if}
				</section>
			</Resizable.Pane>

			<Resizable.Handle withHandle />

			<Resizable.Pane defaultSize={23} minSize={20} class="min-w-0">
				<section class="flex h-full min-h-0 flex-col" aria-label="Inspector">
					<ScrollArea class="min-h-0 flex-1">
						<div class="flex flex-col gap-3 p-3">
							{#if inspectorKind === "project"}
								<Card.Root data-size="sm">
									<Card.Header>
										<Card.Title>
											{projectEditorMode === "create"
												? "New project"
												: projectEditorMode === "edit"
													? "Edit project"
													: selectedProject?.displayName ?? "Project"}
										</Card.Title>
									</Card.Header>
									<Card.Content class="flex flex-col gap-3">
										{#if projectError}
											<Alert.Root variant="destructive">
												<CircleAlertIcon />
												<Alert.Title class="sr-only">Project error</Alert.Title>
												<Alert.Description>
													{projectError}
													{#if projectConflict}
														<span class="block">Reload to use the latest project details.</span>
													{/if}
												</Alert.Description>
												{#if projectConflict}
													<Alert.Action>
														<Button
															variant="ghost"
															size="xs"
															disabled={isLoading}
															aria-busy={isLoading}
															onclick={reloadSelectedProject}
														>Reload</Button>
													</Alert.Action>
												{/if}
											</Alert.Root>
										{/if}
										<form
											id="project-editor-form"
											onsubmit={(event) => {
												event.preventDefault();
												void saveProjectEditor();
											}}
										>
											<Field.Group>
												<Field.Field
													data-disabled={projectEditorMode === "view" || isLoading}
													data-invalid={projectValidationVisible && Boolean(projectNameError)}
												>
													<Field.Label for="project-display-name">Name</Field.Label>
													<Input
														bind:ref={projectNameInput}
														id="project-display-name"
														bind:value={projectDraftName}
														disabled={projectEditorMode === "view" || projectSaving || isLoading}
														aria-invalid={projectValidationVisible && Boolean(projectNameError)}
													/>
													{#if projectValidationVisible && projectNameError}
														<Field.Error>{projectNameError}</Field.Error>
													{/if}
												</Field.Field>
											</Field.Group>
										</form>
									</Card.Content>
									<Card.Footer class="flex gap-2">
										{#if projectEditorMode === "view"}
											<Button
												size="sm"
												disabled={!selectedProject || workspaceMutationBusy}
												onclick={() => void beginProjectEdit()}
											>
												<PencilIcon data-icon="inline-start" />
												Edit
											</Button>
											<Button
												variant="destructive"
												size="sm"
												disabled={!selectedProject || workspaceMutationBusy}
												onclick={() => (projectDeleteDialogOpen = true)}
											>
												<Trash2Icon data-icon="inline-start" />
												Delete
											</Button>
										{:else}
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={projectSaving || isLoading}
												onclick={() => void cancelProjectEditor()}
											>Cancel</Button>
											<Button
												type="submit"
												form="project-editor-form"
												size="sm"
												disabled={!projectDirty || projectSaving || projectConflict || isLoading}
											>
												<SaveIcon data-icon="inline-start" />
												{projectSaving ? "Saving…" : "Save"}
											</Button>
										{/if}
									</Card.Footer>
								</Card.Root>
							{:else if inspectorKind === "server" && selectedServer}
								<Card.Root data-size="sm">
									<Card.Header>
										<Card.Title>
											{serverEditorMode === "edit" ? "Edit server" : selectedServer.displayName}
										</Card.Title>
									</Card.Header>
									<Card.Content class="flex flex-col gap-3">
										{#if serverError}
											<Alert.Root variant="destructive">
												<CircleAlertIcon />
												<Alert.Title class="sr-only">Server error</Alert.Title>
												<Alert.Description>
													{serverError}
													{#if serverConflict}
														<span class="block">Reload to use the latest server details.</span>
													{/if}
												</Alert.Description>
												{#if serverConflict}
													<Alert.Action>
														<Button
															variant="ghost"
															size="xs"
															disabled={isLoading}
															aria-busy={isLoading}
															onclick={reloadSelectedServer}
														>Reload</Button>
													</Alert.Action>
												{/if}
											</Alert.Root>
										{/if}
										<form
											id="server-editor-form"
											onsubmit={(event) => {
												event.preventDefault();
												void saveServerEditor();
											}}
										>
											<Field.Group>
												<Field.Field
													data-disabled={serverEditorMode === "view" || isLoading}
													data-invalid={serverValidationVisible && Boolean(serverNameError)}
												>
													<Field.Label for="server-display-name">Name</Field.Label>
													<Input
														bind:ref={serverNameInput}
														id="server-display-name"
														bind:value={serverDraftName}
														disabled={serverEditorMode === "view" || serverSaving || isLoading}
														aria-invalid={serverValidationVisible && Boolean(serverNameError)}
													/>
													{#if serverValidationVisible && serverNameError}
														<Field.Error>{serverNameError}</Field.Error>
													{/if}
												</Field.Field>
												<Field.Field data-disabled={serverEditorMode === "view" || isLoading}>
													<Field.Label for="server-project">Project</Field.Label>
													<NativeSelect.Root
														id="server-project"
														class="w-full"
														bind:value={serverDraftProjectId}
														disabled={serverEditorMode === "view" || serverSaving || isLoading}
													>
														<NativeSelect.Option value="">No project</NativeSelect.Option>
														{#each projects as project (project.projectId)}
															<NativeSelect.Option value={project.projectId}>
																{project.displayName}
															</NativeSelect.Option>
														{/each}
													</NativeSelect.Root>
												</Field.Field>
											</Field.Group>
										</form>
										<p class="text-muted-foreground text-xs">
											{selectedServerConnections.length === 1
												? "1 connection"
												: `${selectedServerConnections.length} connections`}
										</p>
									</Card.Content>
									<Card.Footer class="flex flex-wrap gap-2">
										{#if serverEditorMode === "view"}
											<Button
												size="sm"
												disabled={workspaceMutationBusy}
												onclick={() => void beginServerEdit()}
											>
												<PencilIcon data-icon="inline-start" />
												Edit
											</Button>
											<Button
												variant="outline"
												size="sm"
												disabled={workspaceMutationBusy}
												onclick={() => void beginCreate(selectedServer.serverId)}
											>
												<PlusIcon data-icon="inline-start" />
												Add connection
											</Button>
											<Button
												variant="destructive"
												size="sm"
												disabled={selectedServerConnections.length > 0 || workspaceMutationBusy}
												onclick={() => (serverDeleteDialogOpen = true)}
											>
												<Trash2Icon data-icon="inline-start" />
												Delete
											</Button>
										{:else}
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={serverSaving || isLoading}
												onclick={() => void cancelServerEditor()}
											>Cancel</Button>
											<Button
												type="submit"
												form="server-editor-form"
												size="sm"
												disabled={!serverDirty || serverSaving || serverConflict || isLoading}
											>
												<SaveIcon data-icon="inline-start" />
												{serverSaving ? "Saving…" : "Save"}
											</Button>
										{/if}
									</Card.Footer>
								</Card.Root>
							{:else if editorLoading}
								<div class="flex flex-col gap-3" role="status" aria-label="Loading connection details">
									<Skeleton class="h-24 w-full" />
									<Skeleton class="h-48 w-full" />
								</div>
							{:else if editorMode === "create" || editorView}
								<Card.Root data-size="sm">
									<Card.Header>
										<Card.Title>
											{editorMode === "create"
												? "New connection"
													: editorMode === "edit"
														? "Edit connection"
														: editorView?.displayName}
											</Card.Title>
										</Card.Header>
									<Card.Content class="flex flex-col gap-3">
										{#if editorError}
											<Alert.Root variant="destructive">
												<CircleAlertIcon />
												<Alert.Title class="sr-only">Connection error</Alert.Title>
												<Alert.Description>
													{editorError}
													{#if editorConflict}
														<span class="block">
															Your input is preserved. Reload this connection before editing again.
														</span>
													{/if}
												</Alert.Description>
												{#if editorConflict}
													<Alert.Action>
														<Button variant="ghost" size="xs" onclick={reloadSelectedEditor}>Reload</Button>
													</Alert.Action>
												{/if}
											</Alert.Root>
											<Separator />
										{/if}

										<form
											id="connection-editor-form"
											onsubmit={(event) => {
												event.preventDefault();
												void saveEditor();
											}}
										>
											<Field.Set>
												<Field.Group>
													<Field.Field
														data-invalid={editorValidationVisible && Boolean(editorDraftErrors.displayName)}
														data-disabled={editorInputsDisabled}
													>
													<Field.Label for="connection-display-name">Name</Field.Label>
														<Input
															bind:ref={displayNameInput}
															bind:value={editorDraft.displayName}
															id="connection-display-name"
															autocomplete="off"
															disabled={editorInputsDisabled}
															aria-invalid={editorValidationVisible && Boolean(editorDraftErrors.displayName)}
														/>
														{#if editorValidationVisible && editorDraftErrors.displayName}
															<Field.Error>{editorDraftErrors.displayName}</Field.Error>
														{/if}
													</Field.Field>

													<Field.Field
														data-invalid={editorValidationVisible && Boolean(editorDraftErrors.host)}
														data-disabled={editorInputsDisabled}
													>
														<Field.Label for="connection-host">SSH host</Field.Label>
														<Input
															bind:value={editorDraft.host}
															id="connection-host"
															autocomplete="off"
															spellcheck="false"
															disabled={editorInputsDisabled}
															aria-invalid={editorValidationVisible && Boolean(editorDraftErrors.host)}
														/>
														{#if editorValidationVisible && editorDraftErrors.host}
															<Field.Error>{editorDraftErrors.host}</Field.Error>
														{/if}
													</Field.Field>

													<div class="grid grid-cols-[minmax(0,1fr)_minmax(5rem,0.55fr)] gap-3">
														<Field.Field
															data-invalid={editorValidationVisible && Boolean(editorDraftErrors.username)}
															data-disabled={editorInputsDisabled}
														>
															<Field.Label for="connection-username">Username</Field.Label>
															<Input
																bind:value={editorDraft.username}
																id="connection-username"
																autocomplete="off"
																disabled={editorInputsDisabled}
																aria-invalid={editorValidationVisible && Boolean(editorDraftErrors.username)}
															/>
															{#if editorValidationVisible && editorDraftErrors.username}
																<Field.Error>{editorDraftErrors.username}</Field.Error>
															{/if}
														</Field.Field>

														<Field.Field
															data-invalid={editorValidationVisible && Boolean(editorDraftErrors.port)}
															data-disabled={editorInputsDisabled}
														>
															<Field.Label for="connection-port">Port</Field.Label>
															<Input
																bind:value={editorDraft.port}
																id="connection-port"
																type="number"
																min="1"
																max="65535"
																step="1"
																disabled={editorInputsDisabled}
																aria-invalid={editorValidationVisible && Boolean(editorDraftErrors.port)}
															/>
															{#if editorValidationVisible && editorDraftErrors.port}
																<Field.Error>{editorDraftErrors.port}</Field.Error>
															{/if}
														</Field.Field>
													</div>

													<Field.Field orientation="horizontal" data-disabled={editorInputsDisabled}>
														<Field.Content>
															<Field.Label for="connection-enabled">Enabled</Field.Label>
															<Field.Description>
																Disabled connections cannot open new terminal sessions.
															</Field.Description>
														</Field.Content>
														<Switch
															bind:checked={editorDraft.enabled}
															id="connection-enabled"
															disabled={editorInputsDisabled}
														/>
													</Field.Field>
												</Field.Group>
											</Field.Set>
										</form>

									</Card.Content>
									<Card.Footer class="flex flex-wrap gap-2">
										{#if editorMode === "view"}
											<Button
												size="sm"
												disabled={!editorView || workspaceMutationBusy}
												onclick={() => void beginEdit()}
											>
												<PencilIcon data-icon="inline-start" />
												Edit
											</Button>
											<Button
												variant="destructive"
												size="sm"
												disabled={editorProfileHasSession || workspaceMutationBusy}
												onclick={() => void openDeleteConnectionDialog()}
											>
												<Trash2Icon data-icon="inline-start" />
												Delete
											</Button>
										{:else}
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={editorSaving}
												onclick={() => void cancelEditor()}
											>
												Cancel
											</Button>
											<Button
												type="submit"
												form="connection-editor-form"
												size="sm"
												disabled={!canSaveEditor}
											>
												<SaveIcon data-icon="inline-start" />
												{editorSaving ? "Saving…" : editorMode === "create" ? "Create" : "Save"}
											</Button>
										{/if}
									</Card.Footer>
								</Card.Root>

							{#if editorProfileHasSession}
								<p class="text-muted-foreground text-[0.625rem]">
									Close this connection's terminal tabs before deleting it.
								</p>
							{/if}

							{#if editorMode === "view" && selectedConnection && editorView}
								<HostTrustCard
									trustState={editorView.trustState}
									enabled={editorView.enabled}
									probeResult={hostTrustProbeResult?.profileId === editorView.profileId
										? hostTrustProbeResult
										: null}
									actionKind={hostTrustAction.profileId === editorView.profileId
										? hostTrustAction.kind
										: null}
									disabled={isLoading ||
										isInteractionActionInFlight ||
										editorSaving ||
										editorDeleting ||
										openingProfileId !== null}
									error={hostTrustError}
									notice={hostTrustNotice}
									onprobe={probeSelectedHostKey}
									onconfirm={confirmPendingHostTrust}
									ondiscard={discardPendingHostTrustFromDialog}
								/>
							{/if}

							{#if selectedConnection}
								<Card.Root data-size="sm">
									<Card.Header>
										<Card.Title class="flex items-center gap-1">
											<KeyRoundIcon data-icon="inline-start" />
											Password
										</Card.Title>
										<Card.Description>
											{passwordStatusDescription(passwordStatusForProfile(selectedConnection.profileId))}
										</Card.Description>
									</Card.Header>
									<Card.Content class="flex flex-col gap-2">
										<div class="flex items-center justify-between gap-2">
											<span class="text-muted-foreground text-[0.625rem]">Status</span>
											<Badge
												variant={passwordStatusVariant(
													passwordStatusForProfile(selectedConnection.profileId),
												)}
											>
												{passwordStatusLabel(passwordStatusForProfile(selectedConnection.profileId))}
											</Badge>
										</div>
										<Separator />
										<div class="flex flex-wrap gap-2">
											<Button
												variant="outline"
												size="sm"
											disabled={isLoading ||
												isInteractionActionInFlight ||
													editorMode !== "view" ||
													editorSaving ||
													editorDeleting}
												onclick={() => void configureSelectedPassword()}
											>
												<KeyRoundIcon data-icon="inline-start" />
												{isCredentialActionInFlight && credentialAction.kind === "configure"
													? credentialActionLabel()
													: "Configure password…"}
											</Button>
											<Button
												variant="destructive"
												size="sm"
											disabled={isLoading ||
												isInteractionActionInFlight ||
													editorMode !== "view" ||
													editorSaving ||
													editorDeleting ||
													passwordStatusForProfile(selectedConnection.profileId) ===
														"not_configured"}
												onclick={() => void removeSelectedPassword()}
											>
												<XIcon data-icon="inline-start" />
												{isCredentialActionInFlight && credentialAction.kind === "remove"
													? credentialActionLabel()
													: "Remove password"}
											</Button>
										</div>
									</Card.Content>
								</Card.Root>

								{#if activeSession?.controlError}
									<Alert.Root variant="destructive">
										<CircleAlertIcon />
										<Alert.Title class="sr-only">Terminal error</Alert.Title>
										<Alert.Description>{activeSession.controlError}</Alert.Description>
									</Alert.Root>
								{/if}
								{/if}
							{:else if editorError && selectedProfileId}
								<Empty.Root>
									<Empty.Header>
										<Empty.Media variant="icon"><ServerIcon /></Empty.Media>
										<Empty.Title>Connection unavailable</Empty.Title>
										<Empty.Description>{editorError}</Empty.Description>
									</Empty.Header>
									<Empty.Content>
										<Button variant="outline" size="sm" onclick={reloadSelectedEditor}>Reload</Button>
									</Empty.Content>
								</Empty.Root>
							{:else}
								<Empty.Root>
									<Empty.Header>
										<Empty.Media variant="icon"><ServerIcon /></Empty.Media>
										<Empty.Title>No connection selected</Empty.Title>
										<Empty.Description>
											Select a project, server, or connection, or create a new connection.
										</Empty.Description>
									</Empty.Header>
									<Empty.Content>
										<Button
											disabled={workspaceMutationBusy}
											onclick={() => void beginCreate()}
										>
											<PlusIcon data-icon="inline-start" />
											New connection
										</Button>
									</Empty.Content>
								</Empty.Root>
							{/if}
						</div>
					</ScrollArea>
				</section>
			</Resizable.Pane>
		</Resizable.PaneGroup>
	</main>

	<AlertDialog.Root bind:open={deleteDialogOpen}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Media><Trash2Icon /></AlertDialog.Media>
				<AlertDialog.Title>Delete {editorView?.displayName ?? "this connection"}?</AlertDialog.Title>
				<AlertDialog.Description>
					This permanently deletes the connection, removes its saved password,
					and removes its Host Trust record. This action cannot be undone.
				</AlertDialog.Description>
			</AlertDialog.Header>
			<AlertDialog.Footer>
				<AlertDialog.Cancel disabled={editorDeleting}>Cancel</AlertDialog.Cancel>
				<AlertDialog.Action
					variant="destructive"
					disabled={editorDeleting || editorProfileHasSession || isInteractionActionInFlight}
					onclick={() => void deleteEditorConnection()}
				>
					{editorDeleting ? "Deleting…" : "Delete connection"}
				</AlertDialog.Action>
			</AlertDialog.Footer>
		</AlertDialog.Content>
	</AlertDialog.Root>

	<AlertDialog.Root bind:open={projectDeleteDialogOpen}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Media><Trash2Icon /></AlertDialog.Media>
				<AlertDialog.Title>Delete {selectedProject?.displayName ?? "this project"}?</AlertDialog.Title>
				<AlertDialog.Description>
					Its servers will move to No project. Connections, credentials, and Host Trust remain unchanged.
				</AlertDialog.Description>
			</AlertDialog.Header>
			<AlertDialog.Footer>
				<AlertDialog.Cancel disabled={projectSaving}>Cancel</AlertDialog.Cancel>
				<AlertDialog.Action
					variant="destructive"
					disabled={!selectedProject || projectSaving}
					onclick={() => void deleteSelectedProject()}
				>
					{projectSaving ? "Deleting…" : "Delete project"}
				</AlertDialog.Action>
			</AlertDialog.Footer>
		</AlertDialog.Content>
	</AlertDialog.Root>

	<AlertDialog.Root bind:open={serverDeleteDialogOpen}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Media><Trash2Icon /></AlertDialog.Media>
				<AlertDialog.Title>Delete {selectedServer?.displayName ?? "this server"}?</AlertDialog.Title>
				<AlertDialog.Description>
					This permanently removes the empty server. It cannot be undone.
				</AlertDialog.Description>
			</AlertDialog.Header>
			<AlertDialog.Footer>
				<AlertDialog.Cancel disabled={serverSaving}>Cancel</AlertDialog.Cancel>
				<AlertDialog.Action
					variant="destructive"
					disabled={!selectedServer || selectedServerConnections.length > 0 || serverSaving}
					onclick={() => void deleteSelectedServer()}
				>
					{serverSaving ? "Deleting…" : "Delete server"}
				</AlertDialog.Action>
			</AlertDialog.Footer>
		</AlertDialog.Content>
	</AlertDialog.Root>
</div>
