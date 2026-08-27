import type { HumanWorkbenchSnapshot } from "$lib/desktop/contracts";

export type WorkspaceInspectorKind = "connection" | "project" | "server";

export type WorkspaceSelection = {
	kind: WorkspaceInspectorKind;
	projectId: string;
	serverId: string;
	profileId: string;
};

export function projectSelection(
	snapshot: HumanWorkbenchSnapshot,
	projectId: string,
): WorkspaceSelection | null {
	const project = snapshot.projects.find((candidate) => candidate.projectId === projectId);
	return project
		? { kind: "project", projectId: project.projectId, serverId: "", profileId: "" }
		: null;
}

export function serverSelection(
	snapshot: HumanWorkbenchSnapshot,
	serverId: string,
): WorkspaceSelection | null {
	const server = snapshot.servers.find((candidate) => candidate.serverId === serverId);
	return server
		? {
				kind: "server",
				projectId: server.projectId ?? "",
				serverId: server.serverId,
				profileId: "",
			}
		: null;
}

export function connectionSelection(
	snapshot: HumanWorkbenchSnapshot,
	profileId: string,
): WorkspaceSelection | null {
	const connection = snapshot.connections.find((candidate) => candidate.profileId === profileId);
	if (!connection) {
		return null;
	}
	const server = snapshot.servers.find((candidate) => candidate.serverId === connection.serverId);
	if (!server) {
		return null;
	}
	return {
		kind: "connection",
		projectId: server.projectId ?? "",
		serverId: server.serverId,
		profileId: connection.profileId,
	};
}

export function resolveWorkspaceSelection(
	snapshot: HumanWorkbenchSnapshot,
	selection: WorkspaceSelection | null,
): WorkspaceSelection | null {
	if (!selection) {
		return null;
	}
	switch (selection.kind) {
		case "project":
			return projectSelection(snapshot, selection.projectId);
		case "server":
			return serverSelection(snapshot, selection.serverId);
		case "connection":
			return connectionSelection(snapshot, selection.profileId);
	}
}
