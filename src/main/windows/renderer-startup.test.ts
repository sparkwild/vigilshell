import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { RendererStartup } from './renderer-startup';

function setup() {
  const replies: ((value: 'retry' | 'dismiss') => void)[] = [];
  const ports = {
    revealLoading: vi.fn(),
    reveal: vi.fn(),
    load: vi.fn().mockResolvedValue(undefined),
    report: vi.fn(),
    prompt: vi.fn<ConstructorParameters<typeof RendererStartup>[0]['prompt']>(
      () => new Promise<'retry' | 'dismiss'>((resolve) => replies.push(resolve)),
    ),
  };
  const lifecycle = new RendererStartup(ports, 1000);
  lifecycle.begin();
  return { lifecycle, ports, replies };
}
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

it('reveals the painted splash before readiness without consuming pending navigation', () => {
  const { lifecycle, ports } = setup();
  lifecycle.requestShow();
  expect(ports.revealLoading).not.toHaveBeenCalled();
  lifecycle.windowReady();
  lifecycle.requestShow();
  expect(ports.revealLoading).toHaveBeenCalledTimes(2);
  expect(ports.reveal).not.toHaveBeenCalled();
  lifecycle.rendererReady();
  lifecycle.hidden();
  for (let i = 0; i < 3; i++) lifecycle.requestShow();
  expect(ports.reveal).toHaveBeenCalledTimes(4);
  vi.advanceTimersByTime(2000);
  expect(ports.prompt).not.toHaveBeenCalled();
});

it('presents a bounded startup failure without requiring a renderer and deduplicates prompts', async () => {
  const { lifecycle, ports, replies } = setup();
  vi.advanceTimersByTime(1000);
  expect(ports.report).toHaveBeenCalledWith({ code: 'renderer_startup_timeout', detail: '' });
  lifecycle.requestShow();
  lifecycle.requestShow();
  expect(ports.prompt).toHaveBeenCalledTimes(1);
  replies[0]!('dismiss');
  await Promise.resolve();
  lifecycle.requestShow();
  expect(ports.prompt).toHaveBeenCalledTimes(2);
});

it('reloads only the renderer on explicit retry and permits successful recovery', async () => {
  const { lifecycle, ports, replies } = setup();
  lifecycle.windowReady();
  lifecycle.fail('renderer_script_failed', 'CSP');
  replies[0]!('retry');
  await Promise.resolve();
  expect(ports.load).toHaveBeenCalledTimes(1);
  lifecycle.begin();
  lifecycle.rendererReady();
  expect(ports.reveal).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(2000);
  expect(ports.prompt).toHaveBeenCalledTimes(1);
});

it('cancels stale native prompts when a load recovers or is disposed', async () => {
  const { lifecycle, ports, replies } = setup();
  lifecycle.fail('renderer_script_failed');
  const signal = ports.prompt.mock.calls[0]![1];
  lifecycle.rendererReady();
  expect(signal.aborted).toBe(true);
  replies[0]!('retry');
  await Promise.resolve();
  expect(ports.load).not.toHaveBeenCalled();
  lifecycle.begin();
  lifecycle.fail('renderer_load_failed');
  lifecycle.dispose();
  expect(ports.prompt.mock.calls[1]![1].aborted).toBe(true);
  replies[1]!('retry');
  await Promise.resolve();
  lifecycle.requestShow();
  expect(ports.load).not.toHaveBeenCalled();
});

it('keeps background failures quiet until the user requests the window', () => {
  const { lifecycle, ports } = setup();
  lifecycle.windowReady();
  lifecycle.rendererReady();
  lifecycle.hidden();
  lifecycle.crashed('crashed');
  expect(ports.prompt).not.toHaveBeenCalled();
  lifecycle.requestShow();
  expect(ports.prompt).toHaveBeenCalledTimes(1);
});

it('reports a failed reload and keeps its recovery entry available', async () => {
  const { lifecycle, ports, replies } = setup();
  ports.load.mockRejectedValue(new Error('offline'));
  lifecycle.fail('renderer_load_failed');
  replies[0]!('retry');
  await Promise.resolve();
  await Promise.resolve();
  expect(ports.report).toHaveBeenLastCalledWith({
    code: 'renderer_load_failed',
    detail: 'offline',
  });
  expect(ports.prompt).toHaveBeenCalledTimes(2);
});

it('cancels an open recovery prompt when hidden and ignores its late retry', async () => {
  const { lifecycle, ports, replies } = setup();
  lifecycle.fail('renderer_load_failed');
  const signal = ports.prompt.mock.calls[0]![1];
  lifecycle.hidden();
  expect(signal.aborted).toBe(true);
  lifecycle.requestShow();
  expect(ports.prompt).toHaveBeenCalledTimes(2);
  replies[0]!('retry');
  await Promise.resolve();
  expect(ports.load).not.toHaveBeenCalled();
  lifecycle.requestShow();
  expect(ports.prompt).toHaveBeenCalledTimes(2);
  lifecycle.dispose();
});

it('does not report an intentional prompt abort as a new recovery failure', async () => {
  const { lifecycle, ports } = setup();
  ports.prompt.mockImplementation(
    (_failure, signal) =>
      new Promise((_resolve, reject) => {
        signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
      }),
  );
  lifecycle.fail('renderer_load_failed');
  lifecycle.hidden();
  await Promise.resolve();
  expect(ports.report).toHaveBeenCalledOnce();
  expect(ports.report).toHaveBeenCalledWith({ code: 'renderer_load_failed', detail: '' });
  expect(ports.load).not.toHaveBeenCalled();
  lifecycle.dispose();
});
