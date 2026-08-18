import type { Terminal } from "@xterm/xterm";
import { describe, expect, it, vi } from "vitest";
import type {
	HumanTerminalLifecycle,
	HumanTerminalOutputMessage,
	HumanTerminalSession,
	OpenHumanTerminalRequest,
} from "$lib/desktop/contracts";
import {
	HumanTerminalController,
	type HumanTerminalDesktopPort,
	type HumanTerminalObserver,
} from "./human-terminal-controller";

const sessionId = "11111111-1111-4111-8111-111111111111";
const otherSessionId = "22222222-2222-4222-8222-222222222222";
const request: OpenHumanTerminalRequest = {
	profileId: "fixture-ready",
	columns: 120,
	rows: 36,
};
const session: HumanTerminalSession = {
	sessionId,
	serverId: "server-ready",
	profileId: request.profileId,
};

type Deferred<T> = {
	promise: Promise<T>;
	resolve: (value: T) => void;
	reject: (error: unknown) => void;
};

function deferred<T>(): Deferred<T> {
	let resolve!: (value: T) => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<T>((resolvePromise, rejectPromise) => {
		resolve = resolvePromise;
		reject = rejectPromise;
	});
	return { promise, resolve, reject };
}

function createDesktopHarness() {
	const openAttempts: Array<{
		settlement: Deferred<HumanTerminalSession>;
		outputReceiver: (message: HumanTerminalOutputMessage) => void;
	}> = [];
	const open = vi.fn<HumanTerminalDesktopPort["open"]>((_request, onOutput) => {
		const settlement = deferred<HumanTerminalSession>();
		openAttempts.push({ settlement, outputReceiver: onOutput });
		return { output: {}, settlement: settlement.promise };
	});
	const sendInput = vi.fn<HumanTerminalDesktopPort["sendInput"]>(async () => undefined);
	const resize = vi.fn<HumanTerminalDesktopPort["resize"]>(async () => undefined);
	const grantOutputCredit = vi.fn<HumanTerminalDesktopPort["grantOutputCredit"]>(
		async () => undefined,
	);
	const close = vi.fn<HumanTerminalDesktopPort["close"]>(async () => ({ status: "closed" }));
	const port: HumanTerminalDesktopPort = {
		createError: (code) => Object.assign(new Error(code), { code }),
		hasErrorCode: (error, code) =>
			typeof error === "object" &&
			error !== null &&
			"code" in error &&
			error.code === code,
		open,
		sendInput,
		resize,
		grantOutputCredit,
		close,
	};

	return {
		port,
		open,
		sendInput,
		resize,
		grantOutputCredit,
		close,
		resolveOpen(value: HumanTerminalSession = session, attemptIndex = 0) {
			const attempt = openAttempts[attemptIndex];
			if (!attempt) {
				throw new Error("terminal open attempt is unavailable");
			}
			attempt.settlement.resolve(value);
		},
		emit(message: unknown, attemptIndex = 0) {
			const attempt = openAttempts[attemptIndex];
			if (!attempt) {
				throw new Error("terminal output receiver is unavailable");
			}
			attempt.outputReceiver(message as HumanTerminalOutputMessage);
		},
	};
}

function createObserver(): HumanTerminalObserver & {
	onLifecycle: ReturnType<typeof vi.fn<(event: HumanTerminalLifecycle) => void>>;
	onError: ReturnType<typeof vi.fn<(error: unknown) => void>>;
} {
	return {
		onLifecycle: vi.fn<(event: HumanTerminalLifecycle) => void>(),
		onError: vi.fn<(error: unknown) => void>(),
	};
}

async function establish(
	controller: HumanTerminalController,
	harness: ReturnType<typeof createDesktopHarness>,
	observer: HumanTerminalObserver,
): Promise<void> {
	const opening = controller.open(request, observer);
	harness.emit({ type: "opened", sessionId });
	harness.resolveOpen();
	await opening;
	controller.activate(sessionId);
}

describe("HumanTerminalController", () => {
	it("binds settlement to the opened lifecycle and credits early output after xterm writes it", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		const opening = controller.open(request, observer);
		harness.emit({ type: "opened", sessionId });
		harness.emit(Uint8Array.from([65, 66, 67]).buffer);
		harness.resolveOpen();

		await expect(opening).resolves.toEqual(session);
		controller.activate(sessionId);
		expect(observer.onLifecycle).toHaveBeenCalledWith({ type: "opened", sessionId });

		let writeCompletion: (() => void) | undefined;
		const write = vi.fn((_data: string | Uint8Array, callback?: () => void) => {
			writeCompletion = callback;
		});
		controller.attachTerminal(sessionId, { write } as unknown as Terminal);
		expect(write).toHaveBeenCalledTimes(1);
		expect(harness.grantOutputCredit).not.toHaveBeenCalled();

		writeCompletion?.();
		await vi.waitFor(() => {
			expect(harness.grantOutputCredit).toHaveBeenCalledWith({ sessionId, bytes: 3 });
		});
	});

	it("closes both identities and rejects an opened lifecycle that does not match settlement", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const opening = controller.open(request, createObserver());
		harness.emit({ type: "opened", sessionId });
		harness.resolveOpen({ ...session, sessionId: otherSessionId });

		await expect(opening).rejects.toMatchObject({ code: "terminal_output_unavailable" });
		expect(harness.close).toHaveBeenCalledTimes(2);
		expect(harness.close.mock.calls.map(([value]) => value.sessionId)).toEqual(
			expect.arrayContaining([sessionId, otherSessionId]),
		);
	});

	it("retries aborted-session cleanup instead of swallowing a close failure", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const opening = controller.open(request, createObserver());
		harness.emit({ type: "opened", sessionId });
		harness.close.mockRejectedValueOnce(
			Object.assign(new Error("timeout"), { code: "terminal_close_timeout" }),
		);
		harness.resolveOpen({ ...session, sessionId: otherSessionId });

		await expect(opening).rejects.toMatchObject({ code: "terminal_close_timeout" });
		expect(
			harness.close.mock.calls.filter(([value]) => value.sessionId === otherSessionId),
		).toHaveLength(2);
	});

	it("serializes input into bounded chunks", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const data = new Uint8Array(2 * 64 * 1024 + 3);

		controller.sendInput(sessionId, data);

		await vi.waitFor(() => expect(harness.sendInput).toHaveBeenCalledTimes(3));
		expect(harness.sendInput.mock.calls.map(([value]) => value.data.length)).toEqual([
			64 * 1024,
			64 * 1024,
			3,
		]);
	});

	it("fails closed before dispatch when pending input exceeds its bound", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		await establish(controller, harness, observer);

		controller.sendInput(sessionId, new Uint8Array(5 * 64 * 1024));

		expect(observer.onError).toHaveBeenCalledTimes(1);
		expect(observer.onError).toHaveBeenCalledWith(
			expect.objectContaining({ code: "terminal_control_queue_full" }),
		);
		await Promise.resolve();
		expect(harness.sendInput).not.toHaveBeenCalled();
		controller.sendInput(sessionId, Uint8Array.from([65]));
		await Promise.resolve();
		expect(harness.sendInput).not.toHaveBeenCalled();
	});

	it("coalesces resize requests while one resize is in flight", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const firstResize = deferred<void>();
		harness.resize.mockImplementationOnce(() => firstResize.promise);

		controller.resize(sessionId, 100, 30);
		controller.resize(sessionId, 110, 31);
		controller.resize(sessionId, 120, 32);
		expect(harness.resize).toHaveBeenCalledTimes(1);
		firstResize.resolve(undefined);

		await vi.waitFor(() => expect(harness.resize).toHaveBeenCalledTimes(2));
		expect(harness.resize).toHaveBeenLastCalledWith({
			sessionId,
			columns: 120,
			rows: 32,
		});
	});

	it("freezes all controls after a resize request fails", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		await establish(controller, harness, observer);
		harness.resize.mockRejectedValueOnce(
			Object.assign(new Error("unavailable"), { code: "terminal_unavailable" }),
		);

		controller.resize(sessionId, 100, 30);
		await vi.waitFor(() => expect(observer.onError).toHaveBeenCalledTimes(1));
		controller.sendInput(sessionId, Uint8Array.from([65]));
		controller.resize(sessionId, 120, 32);
		harness.emit(Uint8Array.from([66]).buffer);
		controller.attachTerminal(sessionId, { write: vi.fn((_data, done) => done?.()) } as unknown as Terminal);
		await Promise.resolve();

		expect(harness.resize).toHaveBeenCalledTimes(1);
		expect(harness.sendInput).not.toHaveBeenCalled();
		expect(harness.grantOutputCredit).not.toHaveBeenCalled();
	});

	it.each([
		["malformed", { type: "exit_status", sessionId, status: -1 }],
		["mismatched", { type: "eof", sessionId: otherSessionId }],
	])("fails closed for %s lifecycle output", async (_label, invalidLifecycle) => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		await establish(controller, harness, observer);
		observer.onLifecycle.mockClear();

		harness.emit(invalidLifecycle);

		expect(observer.onError).toHaveBeenCalledWith(
			expect.objectContaining({ code: "terminal_output_unavailable" }),
		);
		await vi.waitFor(() => expect(harness.close).toHaveBeenCalledWith({ sessionId }));
	});

	it("freezes controls after output failure even when best-effort close fails", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		await establish(controller, harness, observer);
		harness.close.mockRejectedValueOnce(
			Object.assign(new Error("unavailable"), { code: "unexpected_failure" }),
		);

		harness.emit({ type: "eof", sessionId: otherSessionId });
		await vi.waitFor(() => expect(harness.close).toHaveBeenCalledWith({ sessionId }));
		controller.sendInput(sessionId, Uint8Array.from([65]));
		controller.resize(sessionId, 80, 24);
		await Promise.resolve();

		expect(observer.onError).toHaveBeenCalledTimes(1);
		expect(harness.sendInput).not.toHaveBeenCalled();
		expect(harness.resize).not.toHaveBeenCalled();
	});

	it("surfaces automatic close failure and allows the Human to retry", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const automaticClose = deferred<{ status: "closed" }>();
		harness.close.mockImplementationOnce(() => automaticClose.promise);
		harness.emit({ type: "eof", sessionId: otherSessionId });

		const humanClose = controller.close(sessionId);
		automaticClose.reject(Object.assign(new Error("unavailable"), { code: "unexpected_failure" }));
		await expect(humanClose).rejects.toMatchObject({ code: "unexpected_failure" });

		await expect(controller.close(sessionId)).resolves.toBeUndefined();
		expect(harness.close).toHaveBeenCalledTimes(2);
	});

	it("gates concurrent close and ignores output after terminal settlement", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const observer = createObserver();
		await establish(controller, harness, observer);
		observer.onLifecycle.mockClear();
		const closeSettlement = deferred<{ status: "closed" }>();
		harness.close.mockImplementationOnce(() => closeSettlement.promise);
		const write = vi.fn();
		controller.attachTerminal(sessionId, { write } as unknown as Terminal);

		const firstClose = controller.close(sessionId);
		const secondClose = controller.close(sessionId);
		controller.sendInput(sessionId, Uint8Array.from([65]));
		controller.resize(sessionId, 80, 24);
		expect(firstClose).toBe(secondClose);
		expect(harness.close).toHaveBeenCalledTimes(1);
		expect(harness.sendInput).not.toHaveBeenCalled();
		expect(harness.resize).not.toHaveBeenCalled();

		harness.emit({ type: "closed", sessionId });
		harness.emit(Uint8Array.from([66]).buffer);
		harness.emit({ type: "eof", sessionId });
		expect(observer.onLifecycle).toHaveBeenCalledTimes(1);
		expect(observer.onLifecycle).toHaveBeenCalledWith({ type: "closed", sessionId });
		expect(write).not.toHaveBeenCalled();

		closeSettlement.resolve({ status: "closed" });
		await expect(firstClose).resolves.toBeUndefined();
		await expect(secondClose).resolves.toBeUndefined();
	});

	it("allows close retry when lifecycle settles before the close request fails", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const firstSettlement = deferred<{ status: "closed" }>();
		harness.close.mockImplementationOnce(() => firstSettlement.promise);

		const firstClose = controller.close(sessionId);
		harness.emit({ type: "closed", sessionId });
		firstSettlement.reject(Object.assign(new Error("unavailable"), { code: "unexpected_failure" }));
		await expect(firstClose).rejects.toMatchObject({ code: "unexpected_failure" });

		await expect(controller.close(sessionId)).resolves.toBeUndefined();
		expect(harness.close).toHaveBeenCalledTimes(2);
	});

	it("restores output credit when a pending close fails after xterm writes", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		let writeCompletion: (() => void) | undefined;
		controller.attachTerminal(
			sessionId,
			{
				write: vi.fn((_data: string | Uint8Array, callback?: () => void) => {
					writeCompletion = callback;
				}),
			} as unknown as Terminal,
		);
		harness.emit(Uint8Array.from([65, 66, 67]).buffer);
		const closeSettlement = deferred<{ status: "closed" }>();
		harness.close.mockImplementationOnce(() => closeSettlement.promise);

		const closing = controller.close(sessionId);
		writeCompletion?.();
		expect(harness.grantOutputCredit).not.toHaveBeenCalled();
		closeSettlement.reject(Object.assign(new Error("unavailable"), { code: "unexpected_failure" }));

		await expect(closing).rejects.toMatchObject({ code: "unexpected_failure" });
		await vi.waitFor(() => {
			expect(harness.grantOutputCredit).toHaveBeenCalledWith({ sessionId, bytes: 3 });
		});
	});

	it("resumes a coalesced resize when a pending close fails", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const firstResize = deferred<void>();
		harness.resize.mockImplementationOnce(() => firstResize.promise);
		controller.resize(sessionId, 100, 30);
		controller.resize(sessionId, 120, 32);
		const closeSettlement = deferred<{ status: "closed" }>();
		harness.close.mockImplementationOnce(() => closeSettlement.promise);

		const closing = controller.close(sessionId);
		firstResize.resolve(undefined);
		await firstResize.promise;
		closeSettlement.reject(Object.assign(new Error("unavailable"), { code: "unexpected_failure" }));

		await expect(closing).rejects.toMatchObject({ code: "unexpected_failure" });
		await vi.waitFor(() => expect(harness.resize).toHaveBeenCalledTimes(2));
		expect(harness.resize).toHaveBeenLastCalledWith({
			sessionId,
			columns: 120,
			rows: 32,
		});
	});

	it("keeps fresh reconnect identity independent from a settled session", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const firstObserver = createObserver();
		await establish(controller, harness, firstObserver);
		harness.emit({ type: "disconnected", sessionId });

		const secondObserver = createObserver();
		const reconnecting = controller.open(request, secondObserver);
		harness.emit({ type: "opened", sessionId: otherSessionId }, 1);
		harness.resolveOpen({ ...session, sessionId: otherSessionId }, 1);
		await expect(reconnecting).resolves.toMatchObject({ sessionId: otherSessionId });
		controller.activate(otherSessionId);
		secondObserver.onLifecycle.mockClear();

		harness.emit({ type: "eof", sessionId }, 0);
		expect(secondObserver.onLifecycle).not.toHaveBeenCalled();
		controller.sendInput(otherSessionId, Uint8Array.from([65]));
		await vi.waitFor(() => {
			expect(harness.sendInput).toHaveBeenCalledWith({ sessionId: otherSessionId, data: [65] });
		});
	});

	it("rejects a backend session identity that is already registered", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		await establish(controller, harness, createObserver());
		const duplicate = controller.open(request, createObserver());
		harness.emit({ type: "opened", sessionId }, 1);
		harness.resolveOpen(session, 1);

		await expect(duplicate).rejects.toMatchObject({ code: "terminal_output_unavailable" });
		expect(harness.close).toHaveBeenCalledWith({ sessionId });
	});

	it("closes and rejects an opening aborted before the opened lifecycle", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const opening = controller.open(request, createObserver());

		controller.closeAllBestEffort();
		harness.emit({ type: "opened", sessionId });
		harness.resolveOpen();

		await expect(opening).rejects.toMatchObject({ code: "terminal_output_unavailable" });
		expect(harness.close).toHaveBeenCalledWith({ sessionId });
	});

	it("closes a late opened session even while its aborted settlement remains pending", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		void controller.open(request, createObserver());
		controller.closeAllBestEffort();

		harness.emit({ type: "opened", sessionId });

		await vi.waitFor(() => expect(harness.close).toHaveBeenCalledWith({ sessionId }));
	});

	it("closes and rejects an opening aborted after the opened lifecycle", async () => {
		const harness = createDesktopHarness();
		const controller = new HumanTerminalController(harness.port);
		const opening = controller.open(request, createObserver());
		harness.emit({ type: "opened", sessionId });

		controller.closeAllBestEffort();
		harness.resolveOpen();

		await expect(opening).rejects.toMatchObject({ code: "terminal_output_unavailable" });
		expect(harness.close).toHaveBeenCalledWith({ sessionId });
	});
});
