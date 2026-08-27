import { describe, expect, it } from "vitest";

import type { HumanWorkbenchSnapshot } from "$lib/desktop/contracts";

import {
	connectionSelection,
	projectSelection,
	resolveWorkspaceSelection,
	serverSelection,
} from "./selection";

const snapshot: HumanWorkbenchSnapshot = {
	projects: [{ projectId: "project-1", displayName: "Production", recordVersion: 1 }],
	servers: [
		{
			serverId: "server-1",
			projectId: "project-1",
			displayName: "API server",
			recordVersion: 2,
		},
	],
	connections: [
		{
			profileId: "profile-1",
			serverId: "server-1",
			displayName: "Primary route",
			recordVersion: 1,
			enabled: true,
			trustState: "required",
		},
		{
			profileId: "profile-2",
			serverId: "server-1",
			displayName: "Secondary route",
			recordVersion: 1,
			enabled: true,
			trustState: "ready",
		},
	],
	terminalAvailable: true,
};

describe("server workspace selection", () => {
	it("does not implicitly activate a server or connection when selecting a project", () => {
		expect(projectSelection(snapshot, "project-1")).toEqual({
			kind: "project",
			projectId: "project-1",
			serverId: "",
			profileId: "",
		});
	});

	it("keeps a server selected without choosing an arbitrary connection", () => {
		expect(serverSelection(snapshot, "server-1")).toEqual({
			kind: "server",
			projectId: "project-1",
			serverId: "server-1",
			profileId: "",
		});
	});

	it("activates only the exact selected connection and its owning server", () => {
		expect(connectionSelection(snapshot, "profile-2")).toEqual({
			kind: "connection",
			projectId: "project-1",
			serverId: "server-1",
			profileId: "profile-2",
		});
	});

	it("restores a retained empty server and rejects removed or inconsistent selections", () => {
		const emptyServerSnapshot = { ...snapshot, connections: [] };
		expect(
			resolveWorkspaceSelection(emptyServerSnapshot, {
				kind: "server",
				projectId: "project-1",
				serverId: "server-1",
				profileId: "",
			}),
		).toEqual({
			kind: "server",
			projectId: "project-1",
			serverId: "server-1",
			profileId: "",
		});
		expect(connectionSelection(emptyServerSnapshot, "profile-1")).toBeNull();
		expect(serverSelection(snapshot, "missing-server")).toBeNull();
	});
});
