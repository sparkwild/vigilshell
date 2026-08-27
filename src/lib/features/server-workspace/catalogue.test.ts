import { describe, expect, it } from "vitest";

import type { HumanWorkbenchSnapshot } from "$lib/desktop/contracts";
import { buildCatalogueHierarchy, connectionLabel, serverForConnection } from "./catalogue";

const snapshot: HumanWorkbenchSnapshot = {
	projects: [{ projectId: "project-1", displayName: "Production", recordVersion: 1 }],
	servers: [
		{
			serverId: "server-2",
			projectId: null,
			displayName: "Build host",
			recordVersion: 1,
		},
		{
			serverId: "server-1",
			projectId: "project-1",
			displayName: "API server",
			recordVersion: 2,
		},
	],
	connections: [
		{
			profileId: "profile-2",
			serverId: "server-1",
			displayName: "Deploy account",
			recordVersion: 1,
			enabled: true,
			trustState: "ready",
		},
		{
			profileId: "profile-1",
			serverId: "server-1",
			displayName: "Admin account",
			recordVersion: 1,
			enabled: false,
			trustState: "required",
		},
	],
	terminalAvailable: true,
};

describe("server workspace catalogue", () => {
	it("builds project, server, and connection hierarchy without identity labels", () => {
		const hierarchy = buildCatalogueHierarchy(snapshot, "");
		expect(hierarchy.map((node) => node.project?.displayName ?? "No project")).toEqual([
			"Production",
			"No project",
		]);
		expect(hierarchy[0]?.servers[0]?.server.displayName).toBe("API server");
		expect(
			hierarchy[0]?.servers[0]?.connections.map((connection) => connection.displayName),
		).toEqual(["Admin account", "Deploy account"]);
	});

	it("searches only human-visible names and keeps matching parents", () => {
		expect(buildCatalogueHierarchy(snapshot, "deploy")[0]?.servers[0]?.connections).toHaveLength(1);
		expect(buildCatalogueHierarchy(snapshot, "production")[0]?.servers[0]?.connections).toHaveLength(
			2,
		);
		expect(buildCatalogueHierarchy(snapshot, "server-1")).toEqual([]);
		expect(buildCatalogueHierarchy(snapshot, "profile-2")).toEqual([]);
	});

	it("resolves visible server and connection labels from opaque state keys", () => {
		expect(serverForConnection(snapshot, "profile-2")?.displayName).toBe("API server");
		expect(connectionLabel(snapshot, "profile-2")).toBe("Deploy account");
		expect(connectionLabel(snapshot, "missing")).toBeUndefined();
	});
});
