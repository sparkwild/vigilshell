<script lang="ts">
	import { onMount } from "svelte";
	import { mode } from "mode-watcher";
	import { FitAddon } from "@xterm/addon-fit";
	import { Terminal } from "@xterm/xterm";
	import "@xterm/xterm/css/xterm.css";
	import { humanTerminalController } from "./tauri-human-terminal-controller";

	let {
		sessionId,
		label,
		active,
		interactive,
	}: {
		sessionId: string;
		label: string;
		active: boolean;
		interactive: boolean;
	} = $props();

	let host: HTMLDivElement;
	let terminal: Terminal | undefined;
	let fitAddon: FitAddon | undefined;
	let fitFrame: number | undefined;
	let themeFrame: number | undefined;

	function terminalTheme() {
		const styles = getComputedStyle(document.documentElement);

		return {
			background: styles.getPropertyValue("--terminal-background").trim(),
			foreground: styles.getPropertyValue("--terminal-foreground").trim(),
			cursor: styles.getPropertyValue("--terminal-cursor").trim(),
			selectionBackground: styles.getPropertyValue("--terminal-selection").trim(),
		};
	}

	function scheduleFit() {
		if (fitFrame !== undefined) {
			cancelAnimationFrame(fitFrame);
		}

		fitFrame = requestAnimationFrame(() => {
			fitFrame = undefined;
			if (active && host.clientWidth > 0 && host.clientHeight > 0) {
				fitAddon?.fit();
				terminal?.focus();
			}
		});
	}

	function scheduleTheme() {
		if (themeFrame !== undefined) {
			cancelAnimationFrame(themeFrame);
		}

		themeFrame = requestAnimationFrame(() => {
			themeFrame = undefined;
			if (terminal) {
				terminal.options.theme = terminalTheme();
			}
		});
	}

	function preventClipboard(event: Event) {
		event.preventDefault();
		event.stopPropagation();
	}

	function preventPastedInput(event: InputEvent) {
		if (event.inputType === "insertFromPaste" || event.inputType === "insertFromDrop") {
			preventClipboard(event);
		}
	}

	onMount(() => {
		terminal = new Terminal({
			allowProposedApi: false,
			allowTransparency: false,
			altClickMovesCursor: false,
			convertEol: false,
			cursorBlink: true,
			disableStdin: !interactive,
			fontFamily: "'SFMono-Regular', Consolas, 'Liberation Mono', monospace",
			fontSize: 12,
			lineHeight: 1.45,
			linkHandler: { activate: () => undefined },
			logLevel: "off",
			rightClickSelectsWord: false,
			screenReaderMode: false,
			scrollback: 0,
			theme: terminalTheme(),
			windowOptions: {},
		});
		fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);
		terminal.open(host);
		terminal.attachCustomKeyEventHandler((event) => {
			const key = event.key.toLocaleLowerCase();
			const clipboardShortcut =
				(event.metaKey && (key === "c" || key === "v" || key === "x")) ||
				(event.ctrlKey && event.shiftKey && (key === "c" || key === "v" || key === "x"));
			return !clipboardShortcut;
		});

		const inputDisposable = terminal.onData((data) => {
			if (interactive) {
				humanTerminalController.sendInput(sessionId, new TextEncoder().encode(data));
			}
		});
		const resizeDisposable = terminal.onResize(({ cols, rows }) => {
			humanTerminalController.resize(sessionId, cols, rows);
		});
		const resizeObserver = new ResizeObserver(scheduleFit);
		resizeObserver.observe(host);
		for (const eventName of ["copy", "cut", "paste", "contextmenu", "dragover", "drop"] as const) {
			host.addEventListener(eventName, preventClipboard, true);
		}
		host.addEventListener("beforeinput", preventPastedInput, true);
		humanTerminalController.attachTerminal(sessionId, terminal);
		scheduleFit();

		return () => {
			resizeObserver.disconnect();
			inputDisposable.dispose();
			resizeDisposable.dispose();
			for (const eventName of ["copy", "cut", "paste", "contextmenu", "dragover", "drop"] as const) {
				host.removeEventListener(eventName, preventClipboard, true);
			}
			host.removeEventListener("beforeinput", preventPastedInput, true);
			if (fitFrame !== undefined) {
				cancelAnimationFrame(fitFrame);
			}
			if (themeFrame !== undefined) {
				cancelAnimationFrame(themeFrame);
			}
			if (terminal) {
				humanTerminalController.detachTerminal(sessionId, terminal);
				terminal.dispose();
			}
			terminal = undefined;
			fitAddon = undefined;
		};
	});

	$effect(() => {
		if (terminal) {
			terminal.options.disableStdin = !interactive;
		}
	});

	$effect(() => {
		active;
		scheduleFit();
	});

	$effect(() => {
		mode.current;
		scheduleTheme();
	});
</script>

<div
	bind:this={host}
	class="terminal-surface h-full min-h-0 w-full overflow-hidden p-3"
	role="region"
	aria-label={label}
></div>
