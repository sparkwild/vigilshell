// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentSessionView, TerminalSessionView, WorkspaceNotification } from '@contracts';
import type { WorkspaceApi } from '../../../shared/workspace-api';
import type { TerminalController } from '../terminal/terminal-controller';
import { WorkspaceStore } from './workspace-store';
import { terminalLabel } from '../terminal/terminal-tab-model';

const stores: WorkspaceStore[] = [];
afterEach(() => {
  stores.splice(0).forEach((store) => store.dispose());
  vi.restoreAllMocks();
});
function setup() {
  let notify!: (note: WorkspaceNotification) => void;
  Object.assign(window, {
    vigilshell: {
      onWorkspaceEvent: (listener: typeof notify) => {
        notify = listener;
        return () => {};
      },
    },
  });
  const close = vi.fn(async () => ({ ok: true, value: { kind: 'done' } }));
  const cancelTransfer = vi.fn();
  const store = new WorkspaceStore({
    closeTerminal: close,
    cancelTransfer,
    getConnection: vi.fn(async () => ({ ok: true, value: { kind: 'done' } })),
  } as unknown as WorkspaceApi);
  stores.push(store);
  store.backend('runtime', false);
  const session = (
    id: string,
    connectionId = 'server',
    phase: TerminalSessionView['phase'] = 'running',
    agentControl?: TerminalSessionView['agentControl'],
  ) => {
    const value: TerminalSessionView = {
      id,
      agentControl,
      connectionId,
      groupId: connectionId,
      connectionRevision: 1,
      operationId: `op-${id}`,
      name: 'server',
      host: 'localhost',
      username: 'fixture',
      port: 22,
      phase,
      error: null,
      exitCode: null,
      terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    };
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'runtime',
        owner: 'owner',
        event: { kind: 'session', session: value },
      },
    });
    return value;
  };
  return { store, close, cancelTransfer, session };
}
it('keeps insertion order through state events and ordinals through removal and rename', async () => {
  const { store, session } = setup();
  session('a');
  session('b');
  const c = session('c');
  session('a', 'server', 'closed');
  expect(store.getSnapshot().sessions.map((s) => s.id)).toEqual(['a', 'b', 'c']);
  await store.closeTerminal('b');
  expect(store.terminalOrdinal('c')).toBe(3);
  expect(terminalLabel(c, 'Renamed', store.terminalOrdinal('c'))).toBe('Renamed · 3');
  session('d');
  expect(store.terminalOrdinal('d')).toBe(4);
  for (const id of ['a', 'c', 'd']) session(id, 'server', 'closed');
  await store.closeTerminals(['a', 'c', 'd']);
  session('e');
  expect(store.terminalOrdinal('e')).toBe(1);
});
it('selects the most recently visited surviving terminal, even from another connection', async () => {
  const { store, session } = setup();
  session('a');
  session('b', 'other');
  session('c');
  store.tab('b');
  store.tab('a');
  store.tab('c');
  await store.closeTerminal('c');
  expect(store.getSnapshot().activeTab).toBe('a');
  expect(store.getSnapshot().selected?.id).toBe('server');
});
it('falls back to the right neighbor when no remaining terminal was visited', async () => {
  const { store, session } = setup();
  session('a');
  session('b');
  session('c');
  await store.closeTerminal('a');
  expect(store.getSnapshot().activeTab).toBe('b');
});
it('closing an inactive tab preserves selection and does not change visit recency', async () => {
  const { store, session } = setup();
  session('a');
  session('b');
  session('c');
  store.tab('c');
  await store.closeTerminal('b');
  expect(store.getSnapshot().activeTab).toBe('c');
});
it('does not override a switch made while the active terminal is closing', async () => {
  const { store, session, close } = setup();
  session('a');
  session('b');
  session('c');
  let finish!: () => void;
  close.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = () => resolve({ ok: true, value: { kind: 'done' } });
      }),
  );
  const closing = store.closeTerminal('a');
  store.tab('c');
  finish();
  await closing;
  expect(store.getSnapshot().activeTab).toBe('c');
});
it('keeps a tab and its selection when close fails', async () => {
  const { store, session, close } = setup();
  session('a');
  close.mockResolvedValueOnce({ ok: false, code: 'disconnected' } as never);
  await expect(store.closeTerminal('a')).rejects.toThrow();
  expect(store.getSnapshot().activeTab).toBe('a');
  expect(store.getSnapshot().sessions).toHaveLength(1);
});
it('closing ended tabs preserves transfers, skips busy sessions, and disposes only removed displays', async () => {
  const { store, session, close, cancelTransfer } = setup();
  session('running');
  session('closed', 'server', 'closed');
  session('failed', 'server', 'failed');
  session('cancelled', 'server', 'cancelled');
  const dispose = vi.fn();
  store.terminals.set('closed', { dispose } as unknown as TerminalController);
  const tasks = store.getSnapshot().transfers;
  const guard = vi.fn();
  store.setTerminalCloseGuard(guard);
  vi.spyOn(store.feedback, 'isBusy').mockImplementation((owner) => owner === 'session:failed');
  const result = await store.closeTerminals(['closed', 'failed', 'cancelled']);
  expect(result.failed).toEqual([{ id: 'failed', code: 'busy' }]);
  expect(store.getSnapshot().sessions.map((s) => s.id)).toEqual(['running', 'failed']);
  expect(store.getSnapshot().activeTab).toBe('running');
  expect(dispose).toHaveBeenCalledTimes(1);
  expect(store.terminals.has('closed')).toBe(false);
  expect(store.getSnapshot().transfers).toBe(tasks);
  expect(close).not.toHaveBeenCalled();
  expect(cancelTransfer).not.toHaveBeenCalled();
  expect(guard).not.toHaveBeenCalled();
  session('closed', 'server', 'closed');
  expect(store.getSnapshot().sessions.map((s) => s.id)).toEqual(['running', 'failed']);
});
it('ignores late running snapshots after termination and selects a surviving tab on close', async () => {
  const { store, session } = setup();
  session('a');
  session('b', 'server', 'closed');
  session('c', 'server', 'closed');
  store.tab('c');
  session('b', 'server', 'running');
  await store.closeTerminals(['b', 'c']);
  expect(store.getSnapshot().sessions.map((s) => s.id)).toEqual(['a']);
  expect(store.getSnapshot().activeTab).toBe('a');
});
it('closing the last ended tab returns to the overview and is repeatable', async () => {
  const { store, session } = setup();
  session('a', 'server', 'closed');
  await store.closeTerminal('a');
  await store.closeTerminal('a');
  expect(store.getSnapshot().activeTab).toBe('overview');
  expect(store.getSnapshot().sessions).toEqual([]);
});

it('closes the confirmed active and ended sessions, retaining failures and later sessions', async () => {
  const { store, session, close } = setup();
  session('failed-close');
  session('success');
  session('ended', 'server', 'closed');
  close.mockResolvedValueOnce({ ok: false, code: 'disconnected' } as never);
  const pending = store.closeTerminals(['failed-close', 'success', 'ended']);
  session('new');
  const result = await pending;
  expect(result.failed).toEqual([{ id: 'failed-close', code: 'disconnected' }]);
  expect(store.getSnapshot().sessions.map((s) => s.id)).toEqual(['failed-close', 'new']);
  expect(close).toHaveBeenCalledTimes(2);
});
it('stops the batch after a canceled close and shares an in-flight batch', async () => {
  const { store, session } = setup();
  session('a');
  session('b');
  let release!: () => void;
  const close = vi.spyOn(store, 'closeTerminal').mockImplementationOnce(
    () =>
      new Promise<void>((resolve) => {
        release = resolve;
      }),
  );
  const first = store.closeTerminals(['a', 'b']);
  expect(store.closeTerminals(['b'])).toBe(first);
  release();
  expect((await first).cancelled).toBe(true);
  expect(close).toHaveBeenCalledTimes(1);
  expect(store.getSnapshot().sessions).toHaveLength(2);
});

it('owns information visibility per session and forgets it when the session is removed', async () => {
  const { store, session } = setup();
  session('a');
  session('b');
  store.hideTerminalInfo('a');
  store.hideTerminalInfo('missing');
  session('a', 'server', 'failed');
  session('c');
  expect(store.getSnapshot().hiddenTerminalInfo.size).toBe(0);
  store.hideTerminalInfo('a');
  session('a', 'server', 'failed');
  expect(store.getSnapshot().hiddenTerminalInfo).toEqual(new Set(['a']));
  await store.closeTerminal('a');
  expect(store.getSnapshot().hiddenTerminalInfo.size).toBe(0);
  session('new');
  expect(store.getSnapshot().hiddenTerminalInfo.size).toBe(0);
  const fresh = setup().store;
  expect(fresh.getSnapshot().hiddenTerminalInfo.size).toBe(0);
});

it('releases feedback timers on session removal and store disposal', async () => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
  try {
    const { store, session } = setup();
    session('a');
    store.startTerminalInfoCountdown('a');
    expect(vi.getTimerCount()).toBe(1);
    session('a', 'server', 'closed');
    await store.closeTerminal('a');
    expect(vi.getTimerCount()).toBe(0);
    session('b');
    store.startTerminalInfoCountdown('b');
    expect(vi.getTimerCount()).toBe(1);
    store.dispose();
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    vi.useRealTimers();
  }
});

it('locks ended streams until the final cause arrives and routes details to the sidebar without reopening the popup', () => {
  const { store, session } = setup();
  session('a');
  store.hideTerminalInfo('a');
  store.terminalStreamEnded('a');
  session('a');
  expect(store.getSnapshot().sessions[0]?.phase).toBe('closed');
  session('a', 'server', 'failed');
  expect(store.getSnapshot().hiddenTerminalInfo.has('a')).toBe(false);
  store.hideTerminalInfo('a');
  const reveal = store.getSnapshot().toolsReveal;
  store.showTerminalDiagnostics('a');
  expect(store.getSnapshot()).toMatchObject({
    activeTab: 'a',
    toolsTab: 'overview',
    diagnosticsSessionId: 'a',
    toolsReveal: reveal + 1,
  });
  expect(store.getSnapshot().hiddenTerminalInfo.has('a')).toBe(true);
  session('a', 'server', 'failed');
  expect(store.getSnapshot().hiddenTerminalInfo.has('a')).toBe(true);
});
it('reconnects only ended sessions and coalesces repeated actions until the request is settled', async () => {
  const { store, session } = setup();
  vi.spyOn(store, 'refresh').mockResolvedValue(undefined);
  await store.initialize();
  session('a');
  expect(store.canReconnectTerminal('a')).toBe(false);
  session('a', 'server', 'closed');
  expect(store.canReconnectTerminal('a')).toBe(true);
  let resolve!: (id: string | null) => void;
  const connect = vi.spyOn(store, 'connect').mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const first = store.reconnectTerminal('a');
  const second = store.reconnectTerminal('a');
  expect(connect).toHaveBeenCalledExactlyOnceWith('server', true, 'a');
  expect(store.getSnapshot().sessions[0]?.id).toBe('a');
  resolve(null);
  await Promise.all([first, second]);
  expect(store.canReconnectTerminal('a')).toBe(true);
  store.backend(null, false);
  expect(store.canReconnectTerminal('a')).toBe(false);
});

it('keeps tool scroll positions per connection and resets them for a new backend', () => {
  const { store } = setup();
  store.saveToolScrollPosition('one', 120);
  store.saveToolScrollPosition('two', 45);
  expect(store.toolScrollPosition('one')).toBe(120);
  expect(store.toolScrollPosition('two')).toBe(45);
  store.backend('next-runtime', false);
  expect(store.toolScrollPosition('one')).toBe(0);
  expect(store.toolScrollPosition('two')).toBe(0);
});

it('maps attached agents onto the existing tab and releases control before closing it', async () => {
  const { store, session, close } = setup();
  session('tab');
  session('other');
  const control = {
    entryId: 'entry',
    entryName: 'DeepSeek Harness',
    contextId: 'context',
    leaseId: 'lease',
    releasing: false,
  };
  session('tab', 'server', 'running', control);
  expect(store.agentTerminal('context')).toBe('tab');
  expect(store.tabs().map((tab) => tab.id)).toEqual(['tab', 'other']);
  const order: string[] = [];
  const request = vi.fn(async () => {
    order.push('release');
    session('tab');
    return { ok: true };
  });
  Object.assign(window.vigilshell, { mcp: { request } });
  close.mockImplementationOnce(async () => {
    order.push('close');
    return { ok: true, value: { kind: 'done' } };
  });
  await store.closeTerminal('tab');
  expect(request).toHaveBeenCalledWith({ action: 'closeSession', sessionId: 'context' });
  expect(order).toEqual(['release', 'close']);
  expect(store.tabs().map((tab) => tab.id)).toEqual(['other']);
});

it('uses the returned terminal mapping after snapshots arrive out of order and after release', () => {
  const { store, session } = setup();
  const view = {
    runningTasks: 0,
    context: {
      id: 'new-context',
      terminal: { id: 'new-tab' },
      connectionId: 'server',
      entryId: 'agent',
      phase: 'ready',
    },
  } as AgentSessionView;
  store.observeAgentViews([view], 'new-context');
  expect(store.agentTerminal('new-context')).toBe('new-tab');
  expect(store.tabs().some((tab) => tab.kind === 'agent')).toBe(false);
  session('new-tab');
  expect(store.getSnapshot().activeTab).toBe('new-tab');
  view.context.phase = 'closed';
  store.observeAgentViews([view], 'new-context');
  expect(store.tabs().map((tab) => tab.id)).toEqual(['new-tab']);
});
