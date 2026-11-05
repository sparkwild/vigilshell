import { describe, expect, it, vi } from 'vitest';
import type { AgentEvent, AgentSnapshot } from '@contracts';
import type { McpApi, McpUiResult } from '../../../contracts/mcp';
import { McpSnapshotStore } from './snapshot-store';

describe('shared MCP snapshots', () => {
  it('shares subscriptions and ignores a read that predates the committed mutation', async () => {
    let receive!: (event: AgentEvent) => void;
    let resolve!: (value: McpUiResult) => void;
    const unsubscribe = vi.fn();
    const api = {
      request: vi.fn(
        () =>
          new Promise<McpUiResult>((done) => {
            resolve = done;
          }),
      ),
      onChanged: vi.fn((listener) => {
        receive = listener;
        return unsubscribe;
      }),
      onNavigate: vi.fn(),
    } as unknown as McpApi;
    const store = new McpSnapshotStore(api);
    const first = store.subscribe(vi.fn()),
      second = store.subscribe(vi.fn());
    expect(api.onChanged).toHaveBeenCalledTimes(1);
    const committed = { endpoint: 'new' } as AgentSnapshot;
    store.accept(committed);
    resolve({ ok: true, snapshot: { endpoint: 'old' } as AgentSnapshot });
    await Promise.resolve();
    expect(store.getSnapshot().snapshot).toBe(committed);
    receive({ kind: 'viewsChanged', selected_view: 'session' });
    expect(store.getSnapshot().selectedView).toBe('session');
    first();
    expect(unsubscribe).not.toHaveBeenCalled();
    second();
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});

it('recovers a failed read after configuration readiness without issuing mutations', async () => {
  let receive!: (event: AgentEvent) => void;
  const snapshot = { endpoint: 'ready' } as AgentSnapshot;
  const api = {
    request: vi
      .fn()
      .mockResolvedValueOnce({ ok: false, code: 'timeout' })
      .mockResolvedValue({ ok: true, snapshot }),
    onChanged: (fn: typeof receive) => {
      receive = fn;
      return () => {};
    },
    onNavigate: vi.fn(),
  } as unknown as McpApi;
  const store = new McpSnapshotStore(api),
    unsubscribe = store.subscribe(() => {});
  await Promise.resolve();
  expect(store.getSnapshot().error).toBe('timeout');
  receive({ kind: 'configuration' });
  expect(store.getSnapshot().error).toBeNull();
  await Promise.resolve();
  expect(store.getSnapshot().snapshot).toBe(snapshot);
  expect(
    vi.mocked(api.request).mock.calls.every(([request]) => request.action === 'snapshot'),
  ).toBe(true);
  unsubscribe();
});
