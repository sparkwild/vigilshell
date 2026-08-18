<script lang="ts">
	import FingerprintIcon from "@lucide/svelte/icons/fingerprint";
	import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
	import ShieldAlertIcon from "@lucide/svelte/icons/shield-alert";
	import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
	import * as Alert from "$lib/components/ui/alert";
	import * as AlertDialog from "$lib/components/ui/alert-dialog";
	import { Badge } from "$lib/components/ui/badge";
	import { Button } from "$lib/components/ui/button";
	import * as Card from "$lib/components/ui/card";
	import { Separator } from "$lib/components/ui/separator";
	import type {
		HostTrustProbeResult,
		TrustState,
	} from "$lib/desktop/contracts";

	type HostTrustActionKind = "probe" | "confirm" | "discard" | null;

	let {
		trustState,
		enabled,
		probeResult,
		actionKind,
		disabled,
		error,
		notice,
		onprobe,
		onconfirm,
		ondiscard,
	}: {
		trustState: TrustState;
		enabled: boolean;
		probeResult: HostTrustProbeResult | null;
		actionKind: HostTrustActionKind;
		disabled: boolean;
		error: string | null;
		notice: string | null;
		onprobe: () => void | Promise<void>;
		onconfirm: () => void | Promise<void>;
		ondiscard: () => void | Promise<void>;
	} = $props();

	let settlingDialog = false;
	let pendingProbe = $derived(
		probeResult?.status === "first_seen" || probeResult?.status === "changed"
			? probeResult
			: null,
	);
	let fingerprintsDiffer = $derived(
		Boolean(
			pendingProbe?.status === "changed" &&
				pendingProbe.previousFingerprint &&
				pendingProbe.previousFingerprint !== pendingProbe.observedFingerprint,
		),
	);

	function trustLabel(): string {
		if (!enabled) {
			return "Connection disabled";
		}
		switch (trustState) {
			case "ready":
				return "Trust ready";
			case "required":
				return "Trust required";
			case "stale":
				return "Trust review required";
		}
	}

	function actionLabel(): string {
		switch (actionKind) {
			case "probe":
				return "Probing…";
			case "confirm":
				return "Confirming…";
			case "discard":
				return "Closing review…";
			case null:
				if (error) {
					return "Verification error";
				}
				if (pendingProbe) {
					return "Review pending";
				}
				if (notice) {
					return probeResult?.status === "matches" ? "Verified now" : "Trust confirmed";
				}
				return trustLabel();
		}
	}

	function handleDialogOpenChange(open: boolean) {
		if (open || !pendingProbe) {
			return;
		}
		if (settlingDialog) {
			settlingDialog = false;
			return;
		}
		void ondiscard();
	}

	function confirmPending(event: Event) {
		event.preventDefault();
		if (settlingDialog) {
			return;
		}
		settlingDialog = true;
		void onconfirm();
	}

	function discardPending(event: Event) {
		event.preventDefault();
		if (settlingDialog) {
			return;
		}
		settlingDialog = true;
		void ondiscard();
	}

	function confirmPendingFromKeyboard(event: KeyboardEvent) {
		if (event.key === "Enter" || event.key === " ") {
			confirmPending(event);
		}
	}

	function discardPendingFromKeyboard(event: KeyboardEvent) {
		if (event.key === "Enter" || event.key === " ") {
			discardPending(event);
		}
	}

	$effect(() => {
		if (!pendingProbe) {
			settlingDialog = false;
		}
	});
</script>

<Card.Root data-size="sm">
	<Card.Header>
		<Card.Title class="flex items-center gap-1">
			<ShieldCheckIcon data-icon="inline-start" />
			Host Trust
		</Card.Title>
		<Card.Description>
			Probe the saved route and explicitly review the server identity before use.
		</Card.Description>
		<Card.Action>
			<Badge
				variant={error
					? "destructive"
					: trustState === "ready" && enabled
						? "secondary"
						: "outline"}
			>
				{actionLabel()}
			</Badge>
		</Card.Action>
	</Card.Header>
	<Card.Content class="flex flex-col gap-2">
		{#if error}
			<Alert.Root variant="destructive">
				<ShieldAlertIcon />
				<Alert.Title class="sr-only">Host Trust error</Alert.Title>
				<Alert.Description>{error}</Alert.Description>
			</Alert.Root>
		{:else if notice}
			<p class="text-muted-foreground text-xs" role="status">{notice}</p>
		{:else if trustState === "ready"}
			<p class="text-muted-foreground text-xs">
				A trust record exists for the current security revision. Opening a terminal will still
				recheck the live host key.
			</p>
		{:else if trustState === "stale"}
			<p class="text-muted-foreground text-xs">
				The saved connection security context changed. Probe and review the host identity again.
			</p>
		{:else}
			<p class="text-muted-foreground text-xs">
				No current Host Trust decision exists for this saved connection.
			</p>
		{/if}

		{#if probeResult?.status === "matches"}
			<Separator />
			<div class="flex flex-col gap-1">
				<span class="text-muted-foreground text-[0.625rem]">Verified key type</span>
				<code class="break-all text-xs">{probeResult.hostKeyType}</code>
			</div>
			<div class="flex flex-col gap-1">
				<span class="text-muted-foreground text-[0.625rem]">Observed fingerprint</span>
				<code class="break-all text-xs">{probeResult.observedFingerprint}</code>
			</div>
		{/if}
	</Card.Content>
	<Card.Footer>
		<Button
			variant="outline"
			size="sm"
			disabled={disabled || !enabled || Boolean(pendingProbe)}
			onclick={() => void onprobe()}
			aria-busy={actionKind === "probe"}
		>
			{#if actionKind === "probe"}
				<RefreshCwIcon data-icon="inline-start" />
				Probing…
			{:else if trustState === "ready"}
				<FingerprintIcon data-icon="inline-start" />
				Verify again
			{:else}
				<FingerprintIcon data-icon="inline-start" />
				Probe host key
			{/if}
		</Button>
	</Card.Footer>
</Card.Root>

<AlertDialog.Root open={Boolean(pendingProbe)} onOpenChange={handleDialogOpenChange}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Media><ShieldAlertIcon /></AlertDialog.Media>
			<AlertDialog.Title>
				{pendingProbe?.status === "changed"
					? "Review changed host identity"
					: "Trust this host identity?"}
			</AlertDialog.Title>
			<AlertDialog.Description>
				{#if pendingProbe?.status === "changed"}
					{#if fingerprintsDiffer}
						The observed host key changed. Verify the new fingerprint through a trusted channel
						before replacing trust.
					{:else}
						The host identity or trust context changed. Review both values before replacing trust.
					{/if}
				{:else}
					No current trust record matches this saved connection. Verify the fingerprint through a
					trusted channel before continuing.
				{/if}
			</AlertDialog.Description>
		</AlertDialog.Header>

		{#if pendingProbe}
			<div class="flex flex-col gap-3">
				<div class="flex flex-col gap-1">
					<span class="text-muted-foreground text-[0.625rem]">Host key type</span>
					<code class="break-all text-xs">{pendingProbe.hostKeyType}</code>
				</div>
				{#if pendingProbe.status === "changed"}
					<div class="flex flex-col gap-1">
						<span class="text-muted-foreground text-[0.625rem]">Previous fingerprint</span>
						<code class="break-all text-xs">{pendingProbe.previousFingerprint ?? "Unavailable"}</code>
					</div>
				{/if}
				<div class="flex flex-col gap-1">
					<span class="text-muted-foreground text-[0.625rem]">Observed fingerprint</span>
					<code class="break-all text-xs">{pendingProbe.observedFingerprint}</code>
				</div>
			</div>
		{/if}

		<AlertDialog.Footer>
			<AlertDialog.Cancel
				disabled={actionKind !== null}
				onclick={discardPending}
				onkeydown={discardPendingFromKeyboard}
			>
				Cancel
			</AlertDialog.Cancel>
			<AlertDialog.Action
				disabled={actionKind !== null}
				onclick={confirmPending}
				onkeydown={confirmPendingFromKeyboard}
			>
				{actionKind === "confirm" ? "Confirming…" : "Trust this identity"}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
