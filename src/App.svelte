<script lang="ts">
	import { ModeWatcher } from "mode-watcher";
	import * as Tooltip from "$lib/components/ui/tooltip";
	import { Toaster } from "$lib/components/ui/sonner";
	import HumanWorkbench from "$lib/features/human-workbench/HumanWorkbench.svelte";
</script>

<ModeWatcher defaultMode="system" />

<Tooltip.Provider delayDuration={400}>
	<HumanWorkbench />
	<Toaster position="bottom-right" />
</Tooltip.Provider>
