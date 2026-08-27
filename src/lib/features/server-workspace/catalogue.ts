import type {
	ConnectionSummary,
	HumanWorkbenchSnapshot,
	ManagedServerSummary,
	ProjectSummary,
} from "$lib/desktop/contracts";

export type ServerCatalogueNode = {
	server: ManagedServerSummary;
	connections: ConnectionSummary[];
};

export type ProjectCatalogueNode = {
	project: ProjectSummary | null;
	servers: ServerCatalogueNode[];
};

function compareLabels(left: { displayName: string }, right: { displayName: string }): number {
	return left.displayName.localeCompare(right.displayName, undefined, { sensitivity: "base" });
}

export function buildCatalogueHierarchy(
	snapshot: HumanWorkbenchSnapshot | null,
	query: string,
): ProjectCatalogueNode[] {
	if (!snapshot) {
		return [];
	}

	const normalizedQuery = query.trim().toLocaleLowerCase();
	const connectionsByServer = new Map<string, ConnectionSummary[]>();
	for (const connection of snapshot.connections) {
		const connections = connectionsByServer.get(connection.serverId) ?? [];
		connections.push(connection);
		connectionsByServer.set(connection.serverId, connections);
	}

	const serversByProject = new Map<string | null, ServerCatalogueNode[]>();
	for (const server of snapshot.servers) {
		const project = server.projectId
			? (snapshot.projects.find((candidate) => candidate.projectId === server.projectId) ?? null)
			: null;
		const projectMatches = Boolean(
			project?.displayName.toLocaleLowerCase().includes(normalizedQuery),
		);
		const serverMatches = server.displayName.toLocaleLowerCase().includes(normalizedQuery);
		const allConnections = (connectionsByServer.get(server.serverId) ?? []).sort(compareLabels);
		const connections =
			normalizedQuery.length === 0 || projectMatches || serverMatches
				? allConnections
				: allConnections.filter((connection) =>
						connection.displayName.toLocaleLowerCase().includes(normalizedQuery),
					);
		if (normalizedQuery.length > 0 && !projectMatches && !serverMatches && connections.length === 0) {
			continue;
		}

		const nodes = serversByProject.get(server.projectId) ?? [];
		nodes.push({ server, connections });
		serversByProject.set(server.projectId, nodes);
	}

	const projectNodes: ProjectCatalogueNode[] = snapshot.projects
		.map((project) => ({
			project,
			servers: (serversByProject.get(project.projectId) ?? []).sort((left, right) =>
				compareLabels(left.server, right.server),
			),
		}))
		.filter(
			(node) =>
				normalizedQuery.length === 0 ||
				node.servers.length > 0 ||
				(normalizedQuery.length > 0 &&
					node.project.displayName.toLocaleLowerCase().includes(normalizedQuery)),
		)
		.sort((left, right) => compareLabels(left.project, right.project));

	const unassigned = (serversByProject.get(null) ?? []).sort((left, right) =>
		compareLabels(left.server, right.server),
	);
	if (unassigned.length > 0) {
		projectNodes.push({ project: null, servers: unassigned });
	}

	return projectNodes;
}

export function serverForConnection(
	snapshot: HumanWorkbenchSnapshot | null,
	profileId: string,
): ManagedServerSummary | undefined {
	const connection = snapshot?.connections.find((candidate) => candidate.profileId === profileId);
	return snapshot?.servers.find((server) => server.serverId === connection?.serverId);
}

export function connectionLabel(
	snapshot: HumanWorkbenchSnapshot | null,
	profileId: string,
): string | undefined {
	return snapshot?.connections.find((connection) => connection.profileId === profileId)?.displayName;
}
