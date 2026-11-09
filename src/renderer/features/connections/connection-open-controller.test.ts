// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import type { ConnectionSummary, TerminalSessionView, WorkspaceNotification } from '@contracts';
import type { WorkspaceApi } from '../../../shared/workspace-api';
import { WorkspaceStore } from './workspace-store';
import { ConnectionOpenController } from './connection-open-controller';

const connection: ConnectionSummary = {
  id: 'connection',
  name: 'Fixture',
  host: '127.0.0.1',
  environment: 'none',
  groupId: null,
  groupSortOrder: 0,
  createdAt: null,
  auth: 'password',
  credentialSaved: true,
  credentialPending: false,
  deleting: false,
  version: 1,
  revision: 1,
};
export function session(
  id: string,
  phase: TerminalSessionView['phase'] = 'running',
  connectionId = 'connection',
): TerminalSessionView {
  return {
    id,
    phase,
    connectionId,
    groupId: `group-${connectionId}`,
    operationId: `operation-${id}`,
    connectionRevision: 1,
    name: 'Fixture',
    host: '127.0.0.1',
    port: 22,
    username: 'fixture',
    error: null,
    exitCode: null,
    terminal: {
      encoding: 'utf-8',
      backspace: 'del',
      delete: 'vt220',
    },
  };
}
const cleanups: Array<() => void> = [];
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
});
async function setup(behavior: () => string = () => 'ask') {
  let notify!: (note: WorkspaceNotification) => void;
  Object.assign(window, {
    vigilshell: {
      onWorkspaceEvent: (listener: typeof notify) => {
        notify = listener;
        return () => {};
      },
    },
  });
  let connections = [connection];
  const store = new WorkspaceStore({
    listCatalogue: vi.fn(async () => ({
      ok: true,
      value: {
        kind: 'catalogue',
        page: {
          version: 1,
          layoutRevision: 1,
          groups: [],
          connections,
          next: null,
        },
      },
    })),
  } as unknown as WorkspaceApi);
  vi.spyOn(store, 'select').mockResolvedValue(undefined);
  store.backend('runtime', false);
  await store.initialize();
  const emit = (value: TerminalSessionView) =>
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'runtime',
        owner: 'owner',
        event: { kind: 'session', session: value },
      },
    });
  const ended = vi.fn();
  const opener = new ConnectionOpenController(store, ended, behavior);
  const disconnect = opener.connect();
  cleanups.push(() => {
    disconnect();
    store.dispose();
  });
  return {
    store,
    opener,
    emit,
    ended,
    removeConnection: async () => {
      connections = [];
      await store.refresh();
    },
  };
}
it('deduplicates repeated choices and cancel makes no connection or tab change', async () => {
  const { store, opener, emit } = await setup();
  emit(session('a'));
  const connect = vi.spyOn(store, 'connect').mockResolvedValue('new');
  const previousTab = store.getSnapshot().activeTab;
  const first = opener.openConnection('connection');
  expect(opener.openConnection('connection')).toBe(first);
  opener.choose('cancel');
  expect(await first).toBeNull();
  expect(store.getSnapshot().activeTab).toBe(previousTab);
  expect(connect).not.toHaveBeenCalled();
});
it('reuses the current tab, then actual visit order, with creation-order fallback unaffected by session updates', async () => {
  const { store, opener, emit } = await setup();
  emit(session('outside', 'running', 'other'));
  emit(session('old'));
  emit(session('new'));
  emit(session('old'));
  expect(store.existingTerminal('connection', 'running')?.id).toBe('new');
  store.tab('old');
  store.tab('outside');
  const pending = opener.openConnection('connection');
  opener.choose('reuse');
  expect(await pending).toBe('old');
  expect(store.getSnapshot().activeTab).toBe('old');
  store.tab('new');
  expect(store.existingTerminal('connection', 'running')?.id).toBe('new');
});
it.each(['connecting', 'trust', 'authenticating', 'opening'] as const)(
  'prioritizes %s over running sessions without a choice',
  async (phase) => {
    const { store, opener, emit } = await setup();
    emit(session('running'));
    emit(session('pending', phase));
    await expect(opener.openConnection('connection')).resolves.toBe('pending');
    expect(opener.getSnapshot()).toBeNull();
    expect(store.getSnapshot().activeTab).toBe('pending');
  },
);
it('passes empty/ended collections and pre-session in-flight work to existing connect deduplication', async () => {
  const { store, opener, emit } = await setup();
  const connect = vi.spyOn(store, 'connect').mockResolvedValue('new');
  await opener.openConnection('connection');
  for (const phase of ['closed', 'failed', 'cancelled'] as const) emit(session(phase, phase));
  await opener.openConnection('connection');
  emit(session('running'));
  vi.spyOn(store, 'isOpening').mockReturnValue(true);
  await opener.openConnection('connection');
  expect(connect).toHaveBeenCalledTimes(3);
  expect(opener.getSnapshot()).toBeNull();
});
it('checks latest sessions and retains the explicit choice when all reusable targets end', async () => {
  const { store, opener, emit, ended } = await setup();
  emit(session('a'));
  emit(session('b'));
  store.tab('a');
  let pending = opener.openConnection('connection');
  emit(session('a', 'closed'));
  opener.choose('reuse');
  expect(await pending).toBe('b');
  pending = opener.openConnection('connection');
  emit(session('b', 'failed'));
  expect(opener.getSnapshot()?.count).toBe(0);
  opener.choose('cancel');
  expect(await pending).toBeNull();
  expect(ended).toHaveBeenCalledTimes(1);
  expect(opener.getSnapshot()).toBeNull();
});
it.each(['source', 'runtime', 'connection'])('cancels when %s disappears', async (reason) => {
  const { store, opener, emit, removeConnection, ended } = await setup();
  emit(session('a'));
  const source = new AbortController();
  const pending = opener.openConnection('connection', source.signal);
  if (reason === 'source') source.abort();
  else if (reason === 'runtime') store.backend('restarted', false);
  else await removeConnection();
  expect(await pending).toBeNull();
  expect(ended).not.toHaveBeenCalled();
});
it('new creates exactly one session and uses the latest capacity and resource lock', async () => {
  const { store, opener, emit } = await setup();
  emit(session('a'));
  const connect = vi.spyOn(store, 'connect').mockResolvedValue('created');
  const pending = opener.openConnection('connection');
  for (const id of ['b', 'c', 'd']) emit(session(id));
  expect(opener.getSnapshot()?.newBlocked).toBe('connection_limit');
  opener.choose('new');
  expect(connect).not.toHaveBeenCalled();
  emit(session('d', 'closed'));
  const busy = vi.spyOn(store.feedback, 'isBusy').mockReturnValue(true);
  opener.choose('new');
  expect(connect).not.toHaveBeenCalled();
  busy.mockReturnValue(false);
  opener.choose('new');
  opener.choose('new');
  expect(await pending).toBe('created');
  expect(connect).toHaveBeenCalledExactlyOnceWith('connection', true);
});
it('total capacity still allows switching and runtime restart drops visit history', async () => {
  const { store, opener, emit } = await setup();
  emit(session('old'));
  emit(session('new'));
  store.tab('old');
  store.tab('overview');
  for (let n = 0; n < 30; n++) emit(session(`other-${n}`, 'running', `other-${n}`));
  const pending = opener.openConnection('connection');
  expect(opener.getSnapshot()?.newBlocked).toBe('total_limit');
  opener.choose('reuse');
  expect(await pending).toBe('old');
  store.backend('next', false);
  expect(store.existingTerminal('connection', 'running')).toBeUndefined();
});

it('shares in-flight creation before a session appears, including explicit new-tab work', async () => {
  const { store, opener, emit } = await setup();
  emit(session('already-running'));
  let complete!: (reply: import('@contracts').WorkspaceReply) => void;
  const run = vi.spyOn(store, 'run').mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  const fresh = store.connect('connection', true);
  const normal = opener.openConnection('connection');
  expect(opener.getSnapshot()).toBeNull();
  expect(store.isOpening('connection')).toBe(true);
  expect(run).toHaveBeenCalledTimes(1);
  complete({ kind: 'done' });
  await Promise.all([fresh, normal]);
  expect(store.isOpening('connection')).toBe(false);
});
it('tracks the automatic fallback when closing a tab and removes closed-tab visit records', async () => {
  const { store, emit } = await setup();
  emit(session('outside', 'running', 'other'));
  emit(session('a'));
  emit(session('b'));
  emit(session('closed', 'closed'));
  store.tab('a');
  store.tab('closed');
  await store.closeTerminal('closed');
  expect(store.getSnapshot().activeTab).toBe('a');
  store.tab('outside');
  expect(store.existingTerminal('connection', 'running')?.id).toBe('a');
});

it('a rejected duplicate new-tab action does not replace the original in-flight request', async () => {
  const { store, opener } = await setup();
  let complete!: (reply: import('@contracts').WorkspaceReply) => void;
  const run = vi
    .spyOn(store, 'run')
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    )
    .mockRejectedValueOnce(new Error('busy'));
  const first = store.connect('connection', true);
  await store.connect('connection', true);
  expect(store.isOpening('connection')).toBe(true);
  const normal = opener.openConnection('connection');
  expect(run).toHaveBeenCalledTimes(2);
  complete({ kind: 'done' });
  await Promise.all([first, normal]);
  expect(store.isOpening('connection')).toBe(false);
});

it('uses the configured repeat-open behavior without bypassing establishing sessions or capacity', async () => {
  let behavior = 'reuse';
  const { store, opener, emit } = await setup(() => behavior);
  emit(session('first'));
  const connect = vi.spyOn(store, 'connect').mockResolvedValue('first');
  await opener.openConnection(connection.id);
  expect(connect).toHaveBeenLastCalledWith(connection.id);
  expect(opener.getSnapshot()).toBeNull();
  behavior = 'new';
  await opener.openConnection(connection.id);
  expect(connect).toHaveBeenLastCalledWith(connection.id, true);
  emit(session('establishing', 'connecting'));
  await opener.openConnection(connection.id);
  expect(connect).toHaveBeenLastCalledWith(connection.id);
  emit(session('establishing', 'running'));
  emit(session('third'));
  emit(session('fourth'));
  const count = connect.mock.calls.length;
  const pending = opener.openConnection(connection.id);
  expect(opener.getSnapshot()?.newBlocked).toBe('connection_limit');
  expect(connect).toHaveBeenCalledTimes(count);
  opener.cancel();
  await pending;
});
