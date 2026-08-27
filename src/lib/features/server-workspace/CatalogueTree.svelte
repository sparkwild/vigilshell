<script lang="ts">
	import ChevronsUpDownIcon from "@lucide/svelte/icons/chevrons-up-down";
	import FolderIcon from "@lucide/svelte/icons/folder";
	import ServerIcon from "@lucide/svelte/icons/server";

	import { Badge } from "$lib/components/ui/badge";
	import { Button, buttonVariants } from "$lib/components/ui/button";
	import * as Collapsible from "$lib/components/ui/collapsible";
	import { cn } from "$lib/utils";
	import type { ConnectionSummary } from "$lib/desktop/contracts";

	import type { ProjectCatalogueNode } from "./catalogue";

	type Props = {
		groups: ProjectCatalogueNode[];
		selectedProjectId: string;
		selectedServerId: string;
		selectedProfileId: string;
		disabled?: boolean;
		onselectproject: (projectId: string) => void;
		onselectserver: (serverId: string) => void;
		onselectconnection: (profileId: string) => void;
	};

	let {
		groups,
		selectedProjectId,
		selectedServerId,
		selectedProfileId,
		disabled = false,
		onselectproject,
		onselectserver,
		onselectconnection,
	}: Props = $props();

	function connectionStatus(connection: ConnectionSummary): string {
		if (!connection.enabled) {
			return "Disabled";
		}
		switch (connection.trustState) {
			case "ready":
				return "Ready";
			case "required":
				return "Trust required";
			case "stale":
				return "Trust review";
		}
	}
</script>

<nav class="flex flex-col gap-1" aria-label="Projects, servers, and connections">
	{#each groups as group (group.project?.projectId ?? "no-project")}
		<Collapsible.Root open={true}>
			<div class="flex items-center gap-0.5">
				<Collapsible.Trigger
					class={buttonVariants({ variant: "ghost", size: "icon-xs" })}
					aria-label={`Toggle ${group.project?.displayName ?? "No project"}`}
				>
					<ChevronsUpDownIcon />
				</Collapsible.Trigger>
				{#if group.project}
					<Button
						variant={selectedProjectId === group.project.projectId ? "secondary" : "ghost"}
						size="xs"
						class="min-w-0 flex-1 justify-start"
						disabled={disabled}
						onclick={() => onselectproject(group.project?.projectId ?? "")}
						aria-pressed={selectedProjectId === group.project.projectId}
					>
						<FolderIcon data-icon="inline-start" />
						<span class="truncate">{group.project.displayName}</span>
					</Button>
				{:else}
					<div class="text-muted-foreground flex min-w-0 flex-1 items-center gap-1 px-2 text-[0.625rem]">
						<FolderIcon />
						<span class="truncate">No project</span>
					</div>
				{/if}
			</div>

			<Collapsible.Content class="flex flex-col gap-0.5 pl-3">
				{#each group.servers as serverNode (serverNode.server.serverId)}
					<Collapsible.Root open={true}>
						<div class="flex items-center gap-0.5">
							<Collapsible.Trigger
								class={buttonVariants({ variant: "ghost", size: "icon-xs" })}
								aria-label={`Toggle ${serverNode.server.displayName}`}
							>
								<ChevronsUpDownIcon />
							</Collapsible.Trigger>
							<Button
								variant={selectedServerId === serverNode.server.serverId ? "secondary" : "ghost"}
								size="xs"
								class="min-w-0 flex-1 justify-start"
								disabled={disabled}
								onclick={() => onselectserver(serverNode.server.serverId)}
								aria-pressed={selectedServerId === serverNode.server.serverId}
							>
								<ServerIcon data-icon="inline-start" />
								<span class="truncate">{serverNode.server.displayName}</span>
							</Button>
						</div>

						<Collapsible.Content class="flex flex-col gap-0.5 pl-5">
							{#each serverNode.connections as connection (connection.profileId)}
								<Button
									variant={selectedProfileId === connection.profileId ? "secondary" : "ghost"}
									size="sm"
									class="h-auto w-full justify-start px-2 py-1"
									disabled={disabled}
									onclick={() => onselectconnection(connection.profileId)}
									aria-pressed={selectedProfileId === connection.profileId}
								>
									<span
										class={cn("workbench-status-dot", !connection.enabled && "is-disabled")}
										data-trust={connection.trustState}
										aria-hidden="true"
									></span>
									<span class="min-w-0 flex-1 truncate text-left">{connection.displayName}</span>
									<Badge
										variant={
											!connection.enabled
												? "outline"
												: connection.trustState === "ready"
													? "secondary"
													: "destructive"
										}
									>
										{connectionStatus(connection)}
									</Badge>
								</Button>
							{/each}
							{#if serverNode.connections.length === 0}
								<p class="text-muted-foreground px-2 py-1 text-[0.625rem]">No connections</p>
							{/if}
						</Collapsible.Content>
					</Collapsible.Root>
				{/each}
			</Collapsible.Content>
		</Collapsible.Root>
	{/each}
</nav>
