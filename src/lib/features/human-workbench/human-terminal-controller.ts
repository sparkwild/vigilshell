import type { Terminal } from "@xterm/xterm";
import type {
	DesktopErrorCode,
	HumanTerminalCloseRequest,
	HumanTerminalCloseSettlement,
	HumanTerminalExitSignal,
	HumanTerminalInputRequest,
	HumanTerminalLifecycle,
	HumanTerminalOutputCreditRequest,
	HumanTerminalOutputMessage,
	HumanTerminalResizeRequest,
	HumanTerminalSession,
	OpenHumanTerminalRequest,
} from "$lib/desktop/contracts";

const initialOutputCreditBytes = 256 * 1024;
const maximumBufferedOutputBytes = 1024 * 1024;
const maximumInputBytes = 64 * 1024;
const maximumPendingInputBytes = 256 * 1024;
const maximumCreditGrantBytes = 1024 * 1024;
const maximumPendingLifecycleEvents = 16;
const maximumTerminalDimension = 65_535;
const uuidV4Pattern =
	/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const exitSignals = new Set<HumanTerminalExitSignal>([
	"abort",
	"alarm",
	"floating_point_exception",
	"hangup",
	"illegal_instruction",
	"interrupt",
	"kill",
	"pipe",
	"quit",
	"segmentation_violation",
	"terminate",
	"user_defined_one",
	"unknown",
]);

export type HumanTerminalObserver = {
	onLifecycle: (event: HumanTerminalLifecycle) => void;
	onError: (error: unknown) => void;
};

export type HumanTerminalDesktopPort = {
	createError: (code: DesktopErrorCode) => Error;
	hasErrorCode: (error: unknown, code: DesktopErrorCode) => boolean;
	open: (
		request: OpenHumanTerminalRequest,
		onOutput: (message: HumanTerminalOutputMessage) => void,
	) => {
		output: unknown;
		settlement: Promise<HumanTerminalSession>;
	};
	sendInput: (request: HumanTerminalInputRequest) => Promise<void>;
	resize: (request: HumanTerminalResizeRequest) => Promise<void>;
	grantOutputCredit: (request: HumanTerminalOutputCreditRequest) => Promise<void>;
	close: (request: HumanTerminalCloseRequest) => Promise<HumanTerminalCloseSettlement>;
};

type TerminalDimensions = {
	columns: number;
	rows: number;
};

type TerminalEntry = {
	channel: unknown | null;
	observer: HumanTerminalObserver;
	openedSessionId: string | null;
	sessionId: string | null;
	activated: boolean;
	aborted: boolean;
	settled: boolean;
	closing: Promise<void> | null;
	cleanupSessionIds: Set<string>;
	forgotten: boolean;
	controlFailed: boolean;
	pendingLifecycle: HumanTerminalLifecycle[];
	pendingError: unknown | null;
	terminal: Terminal | null;
	outputQueue: Uint8Array[];
	bufferedOutputBytes: number;
	writingOutput: boolean;
	pendingCreditBytes: number;
	grantingCredit: boolean;
	inputTail: Promise<void>;
	pendingInputBytes: number;
	pendingResize: TerminalDimensions | null;
	resizing: boolean;
	lastDimensions: TerminalDimensions;
};

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}

function isSessionId(value: unknown): value is string {
	return typeof value === "string" && uuidV4Pattern.test(value);
}

function canonicalLifecycle(value: unknown): HumanTerminalLifecycle | null {
	if (!isRecord(value) || !isSessionId(value.sessionId) || typeof value.type !== "string") {
		return null;
	}

	const sessionId = value.sessionId;
	switch (value.type) {
		case "opened":
		case "eof":
		case "closed":
		case "disconnected":
		case "protocol_failure":
			return { type: value.type, sessionId };
		case "exit_status":
			if (
				typeof value.status !== "number" ||
				!Number.isInteger(value.status) ||
				value.status < 0 ||
				value.status > 0xffff_ffff
			) {
				return null;
			}
			return { type: "exit_status", sessionId, status: value.status };
		case "exit_signal":
			if (
				typeof value.signal !== "string" ||
				!exitSignals.has(value.signal as HumanTerminalExitSignal) ||
				typeof value.coreDumped !== "boolean"
			) {
				return null;
			}
			return {
				type: "exit_signal",
				sessionId,
				signal: value.signal as HumanTerminalExitSignal,
				coreDumped: value.coreDumped,
			};
		default:
			return null;
	}
}

function validDimensions(columns: number, rows: number): boolean {
	return (
		Number.isInteger(columns) &&
		Number.isInteger(rows) &&
		columns > 0 &&
		rows > 0 &&
		columns <= maximumTerminalDimension &&
		rows <= maximumTerminalDimension
	);
}

function validSession(session: HumanTerminalSession, request: OpenHumanTerminalRequest): boolean {
	return (
		isSessionId(session.sessionId) &&
		typeof session.serverId === "string" &&
		session.serverId.length > 0 &&
		typeof session.profileId === "string" &&
		session.profileId === request.profileId
	);
}

export class HumanTerminalController {
	readonly #desktop: HumanTerminalDesktopPort;
	readonly #openingEntries = new Set<TerminalEntry>();
	readonly #entries = new Map<string, TerminalEntry>();

	constructor(desktop: HumanTerminalDesktopPort) {
		this.#desktop = desktop;
	}

	async open(
		request: OpenHumanTerminalRequest,
		observer: HumanTerminalObserver,
	): Promise<HumanTerminalSession> {
		if (!validDimensions(request.columns, request.rows)) {
			throw this.#desktop.createError("invalid_request");
		}

		const entry: TerminalEntry = {
			channel: null,
			observer,
			openedSessionId: null,
			sessionId: null,
			activated: false,
			aborted: false,
			settled: false,
			closing: null,
			cleanupSessionIds: new Set(),
			forgotten: false,
			controlFailed: false,
			pendingLifecycle: [],
			pendingError: null,
			terminal: null,
			outputQueue: [],
			bufferedOutputBytes: 0,
			writingOutput: false,
			pendingCreditBytes: 0,
			grantingCredit: false,
			inputTail: Promise.resolve(),
			pendingInputBytes: 0,
			pendingResize: null,
			resizing: false,
			lastDimensions: { columns: request.columns, rows: request.rows },
		};
		const attempt = this.#desktop.open(request, (message) => this.#receive(entry, message));
		entry.channel = attempt.output;
		this.#openingEntries.add(entry);

		try {
			const session = await attempt.settlement;
			if (
				!validSession(session, request) ||
				entry.openedSessionId === null ||
				entry.openedSessionId !== session.sessionId
			) {
				entry.aborted = true;
				await this.#settleAbortedEntry(
					entry,
					isSessionId(session.sessionId) ? session.sessionId : null,
					entry.openedSessionId,
				);
				throw this.#desktop.createError("terminal_output_unavailable");
			}

			entry.sessionId = session.sessionId;
			this.#openingEntries.delete(entry);
			if (entry.aborted) {
				await this.#settleAbortedEntry(entry, session.sessionId, entry.openedSessionId);
				throw this.#desktop.createError("terminal_output_unavailable");
			}
			if (this.#entries.has(session.sessionId)) {
				entry.aborted = true;
				await this.#settleAbortedEntry(entry, session.sessionId);
				throw this.#desktop.createError("terminal_output_unavailable");
			}

			this.#entries.set(session.sessionId, entry);
			return session;
		} catch (error) {
			this.#openingEntries.delete(entry);
			entry.aborted = true;
			if (entry.openedSessionId !== null) {
				await this.#settleAbortedEntry(entry, entry.openedSessionId);
			}
			throw error;
		}
	}

	activate(sessionId: string): void {
		const entry = this.#entries.get(sessionId);
		if (!entry || entry.activated || entry.forgotten) {
			return;
		}

		entry.activated = true;
		const events = entry.pendingLifecycle.splice(0);
		for (const event of events) {
			entry.observer.onLifecycle(event);
		}
		if (entry.pendingError) {
			const error = entry.pendingError;
			entry.pendingError = null;
			entry.observer.onError(error);
		}
	}

	attachTerminal(sessionId: string, terminal: Terminal): void {
		const entry = this.#entries.get(sessionId);
		if (!entry || entry.forgotten || (entry.settled && entry.outputQueue.length === 0)) {
			return;
		}
		if (entry.terminal && entry.terminal !== terminal) {
			this.#failEntry(entry);
			return;
		}

		entry.terminal = terminal;
		this.#pumpOutput(entry);
	}

	detachTerminal(sessionId: string, terminal: Terminal): void {
		const entry = this.#entries.get(sessionId);
		if (entry?.terminal === terminal) {
			entry.terminal = null;
		}
	}

	sendInput(sessionId: string, data: Uint8Array): void {
		const entry = this.#entries.get(sessionId);
		if (!entry || entry.settled || entry.closing || entry.controlFailed || data.byteLength === 0) {
			return;
		}

		for (let offset = 0; offset < data.byteLength; offset += maximumInputBytes) {
			const chunk = data.slice(offset, Math.min(offset + maximumInputBytes, data.byteLength));
			if (entry.pendingInputBytes + chunk.byteLength > maximumPendingInputBytes) {
				this.#controlFailure(entry, this.#desktop.createError("terminal_control_queue_full"));
				return;
			}
			entry.pendingInputBytes += chunk.byteLength;
			entry.inputTail = entry.inputTail
				.then(async () => {
					if (entry.settled || entry.closing || entry.controlFailed || entry.forgotten) {
						return;
					}
					await this.#desktop.sendInput({
						sessionId,
						data: Array.from(chunk),
					});
				})
				.catch((error: unknown) => this.#controlFailure(entry, error))
				.finally(() => {
					entry.pendingInputBytes = Math.max(0, entry.pendingInputBytes - chunk.byteLength);
				});
		}
	}

	resize(sessionId: string, columns: number, rows: number): void {
		const entry = this.#entries.get(sessionId);
		if (
			!entry ||
			entry.settled ||
			entry.closing ||
			entry.controlFailed ||
			!validDimensions(columns, rows) ||
			(entry.lastDimensions.columns === columns && entry.lastDimensions.rows === rows)
		) {
			return;
		}

		entry.lastDimensions = { columns, rows };
		entry.pendingResize = { columns, rows };
		void this.#drainResize(entry);
	}

	dimensions(sessionId: string): TerminalDimensions | null {
		const dimensions = this.#entries.get(sessionId)?.lastDimensions;
		return dimensions ? { ...dimensions } : null;
	}

	close(sessionId: string): Promise<void> {
		const entry = this.#entries.get(sessionId);
		if (entry?.closing) {
			return entry.closing;
		}

		let completed = false;
		const attempt = (async () => {
			try {
				const settlement = await this.#desktop.close({ sessionId });
				if (settlement.status !== "closed") {
					throw this.#desktop.createError("unexpected_failure");
				}
				completed = true;
				if (entry) {
					entry.settled = true;
				}
			} catch (error) {
				if (this.#desktop.hasErrorCode(error, "terminal_session_closed")) {
					completed = true;
					if (entry) {
						entry.settled = true;
					}
					return;
				}
				throw error;
			} finally {
				if (entry && !completed) {
					entry.closing = null;
					void this.#drainCredit(entry);
					void this.#drainResize(entry);
				}
			}
		})();
		if (entry) {
			entry.closing = attempt;
		}
		return attempt;
	}

	forget(sessionId: string): void {
		const entry = this.#entries.get(sessionId);
		if (!entry) {
			return;
		}

		this.#entries.delete(sessionId);
		entry.forgotten = true;
		entry.channel = null;
		entry.terminal = null;
		entry.outputQueue.length = 0;
		entry.bufferedOutputBytes = 0;
		entry.pendingLifecycle.length = 0;
		entry.pendingError = null;
		entry.pendingCreditBytes = 0;
		entry.pendingInputBytes = 0;
		entry.pendingResize = null;
	}

	closeAllBestEffort(): void {
		for (const entry of this.#openingEntries) {
			entry.aborted = true;
			if (entry.openedSessionId) {
				void this.#desktop.close({ sessionId: entry.openedSessionId }).catch(() => undefined);
			}
		}
		for (const [sessionId, entry] of this.#entries) {
			if (!entry.settled && !entry.closing) {
				void this.close(sessionId).catch(() => undefined);
			}
		}
	}

	#receive(entry: TerminalEntry, message: HumanTerminalOutputMessage): void {
		if (entry.forgotten || entry.settled) {
			return;
		}
		if (entry.aborted) {
			if (!(message instanceof ArrayBuffer)) {
				const lifecycle = canonicalLifecycle(message);
				if (
					lifecycle?.type === "opened" &&
					entry.openedSessionId === null &&
					!entry.closing
				) {
					entry.openedSessionId = lifecycle.sessionId;
					entry.closing = this.#desktop
						.close({ sessionId: lifecycle.sessionId })
						.then(() => {
							entry.settled = true;
						})
						.catch((error: unknown) => {
							if (this.#desktop.hasErrorCode(error, "terminal_session_closed")) {
								entry.settled = true;
							}
						})
						.finally(() => {
							entry.closing = null;
						});
					return;
				}
				if (
					lifecycle &&
					lifecycle.sessionId === entry.openedSessionId &&
					(lifecycle.type === "closed" ||
						lifecycle.type === "disconnected" ||
						lifecycle.type === "protocol_failure")
				) {
					entry.settled = true;
					this.#publishLifecycle(entry, lifecycle);
				}
			}
			return;
		}
		if (message instanceof ArrayBuffer) {
			if (!entry.openedSessionId) {
				this.#failEntry(entry);
				return;
			}
			this.#enqueueOutput(entry, new Uint8Array(message));
			return;
		}

		const lifecycle = canonicalLifecycle(message);
		if (!lifecycle) {
			this.#failEntry(entry);
			return;
		}
		if (lifecycle.type === "opened") {
			if (entry.openedSessionId !== null) {
				this.#failEntry(entry);
				return;
			}
			entry.openedSessionId = lifecycle.sessionId;
		} else if (entry.openedSessionId !== lifecycle.sessionId) {
			this.#failEntry(entry);
			return;
		}

		if (
			lifecycle.type === "closed" ||
			lifecycle.type === "disconnected" ||
			lifecycle.type === "protocol_failure"
		) {
			entry.settled = true;
		}
		this.#publishLifecycle(entry, lifecycle);
	}

	#enqueueOutput(entry: TerminalEntry, data: Uint8Array): void {
		if (data.byteLength === 0) {
			return;
		}
		const limit = entry.terminal ? maximumBufferedOutputBytes : initialOutputCreditBytes;
		if (entry.bufferedOutputBytes + data.byteLength > limit) {
			this.#failEntry(entry);
			return;
		}

		entry.outputQueue.push(data);
		entry.bufferedOutputBytes += data.byteLength;
		this.#pumpOutput(entry);
	}

	#pumpOutput(entry: TerminalEntry): void {
		const terminal = entry.terminal;
		if (!terminal || entry.writingOutput || entry.forgotten) {
			return;
		}
		const chunk = entry.outputQueue.shift();
		if (!chunk) {
			return;
		}

		entry.writingOutput = true;
		try {
			terminal.write(chunk, () => {
				entry.writingOutput = false;
				entry.bufferedOutputBytes -= chunk.byteLength;
				if (
					!entry.settled &&
					!entry.forgotten &&
					!entry.controlFailed &&
					entry.terminal === terminal
				) {
					this.#queueCredit(entry, chunk.byteLength);
				}
				this.#pumpOutput(entry);
			});
		} catch {
			entry.writingOutput = false;
			this.#failEntry(entry);
		}
	}

	#queueCredit(entry: TerminalEntry, bytes: number): void {
		entry.pendingCreditBytes = Math.min(
			maximumCreditGrantBytes,
			entry.pendingCreditBytes + bytes,
		);
		void this.#drainCredit(entry);
	}

	async #drainCredit(entry: TerminalEntry): Promise<void> {
		if (entry.grantingCredit || !entry.sessionId) {
			return;
		}

		entry.grantingCredit = true;
		try {
			while (
				entry.pendingCreditBytes > 0 &&
				!entry.settled &&
				!entry.closing &&
				!entry.forgotten &&
				!entry.controlFailed
			) {
				const bytes = Math.min(entry.pendingCreditBytes, maximumCreditGrantBytes);
				entry.pendingCreditBytes -= bytes;
				await this.#desktop.grantOutputCredit({ sessionId: entry.sessionId, bytes });
			}
		} catch (error) {
			this.#controlFailure(entry, error);
		} finally {
			entry.grantingCredit = false;
		}
	}

	async #drainResize(entry: TerminalEntry): Promise<void> {
		if (entry.resizing || !entry.sessionId) {
			return;
		}

		entry.resizing = true;
		try {
			while (
				entry.pendingResize &&
				!entry.settled &&
				!entry.closing &&
				!entry.forgotten &&
				!entry.controlFailed
			) {
				const dimensions = entry.pendingResize;
				entry.pendingResize = null;
				await this.#desktop.resize({
					sessionId: entry.sessionId,
					columns: dimensions.columns,
					rows: dimensions.rows,
				});
			}
		} catch (error) {
			this.#controlFailure(entry, error);
		} finally {
			entry.resizing = false;
		}
	}

	#controlFailure(entry: TerminalEntry, error: unknown): void {
		if (entry.forgotten || entry.controlFailed) {
			return;
		}
		entry.controlFailed = true;
		entry.pendingCreditBytes = 0;
		entry.pendingResize = null;
		this.#publishError(entry, error);
	}

	#failEntry(entry: TerminalEntry): void {
		if (entry.aborted || entry.forgotten) {
			return;
		}
		entry.aborted = true;
		entry.controlFailed = true;
		entry.pendingCreditBytes = 0;
		entry.pendingResize = null;
		this.#publishError(entry, this.#desktop.createError("terminal_output_unavailable"));
		const sessionId = entry.sessionId ?? entry.openedSessionId;
		if (sessionId) {
			const attempt = this.#desktop
				.close({ sessionId })
				.then(() => {
					if (!entry.settled && !entry.forgotten) {
						entry.settled = true;
						this.#publishLifecycle(entry, { type: "closed", sessionId });
					}
				})
				.finally(() => {
					if (!entry.settled) {
						entry.closing = null;
					}
				});
			entry.closing = attempt;
			void attempt.catch(() => undefined);
		}
	}

	#publishLifecycle(entry: TerminalEntry, lifecycle: HumanTerminalLifecycle): void {
		if (entry.activated) {
			entry.observer.onLifecycle(lifecycle);
		} else if (entry.pendingLifecycle.length < maximumPendingLifecycleEvents) {
			entry.pendingLifecycle.push(lifecycle);
		} else {
			this.#failEntry(entry);
		}
	}

	#publishError(entry: TerminalEntry, error: unknown): void {
		if (entry.activated) {
			entry.observer.onError(error);
		} else {
			entry.pendingError = error;
		}
	}

	async #settleAbortedEntry(
		entry: TerminalEntry,
		...sessionIds: Array<string | null>
	): Promise<void> {
		for (const sessionId of sessionIds.filter(isSessionId)) {
			entry.cleanupSessionIds.add(sessionId);
		}
		if (entry.closing) {
			await entry.closing;
		}
		if (entry.settled) {
			return;
		}

		const uniqueSessionIds = [...entry.cleanupSessionIds];
		await Promise.all(
			uniqueSessionIds.map(async (sessionId) => {
				try {
					await this.#desktop.close({ sessionId });
				} catch (error) {
					if (!this.#desktop.hasErrorCode(error, "terminal_session_closed")) {
						throw error;
					}
				}
			}),
		);
		entry.settled = true;
		entry.cleanupSessionIds.clear();
	}
}
