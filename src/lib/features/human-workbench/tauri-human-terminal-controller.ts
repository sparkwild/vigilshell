import {
	closeHumanTerminal,
	DesktopAdapterError,
	grantHumanTerminalOutputCredit,
	openHumanTerminal,
	resizeHumanTerminal,
	sendHumanTerminalInput,
} from "$lib/desktop/tauri-desktop-adapter";
import {
	HumanTerminalController,
	type HumanTerminalDesktopPort,
} from "./human-terminal-controller";

const tauriHumanTerminalDesktopPort: HumanTerminalDesktopPort = {
	createError: (code) => new DesktopAdapterError(code),
	hasErrorCode: (error, code) => error instanceof DesktopAdapterError && error.code === code,
	open: openHumanTerminal,
	sendInput: sendHumanTerminalInput,
	resize: resizeHumanTerminal,
	grantOutputCredit: grantHumanTerminalOutputCredit,
	close: closeHumanTerminal,
};

export const humanTerminalController = new HumanTerminalController(
	tauriHumanTerminalDesktopPort,
);
