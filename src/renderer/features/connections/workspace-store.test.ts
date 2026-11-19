// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import { WorkspaceStore } from './workspace-store';
import type { WorkspaceApi } from '../../../shared/workspace-api';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

it('restarts both group and connection pages after conflict and publishes only the complete catalogue', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  let attempt = 0;
  const listCatalogue = vi.fn(
    async (params: { kind: string; offset: number; version: number | null }) => {
      if (params.kind === 'groups') attempt++;
      if (attempt === 1 && params.kind === 'connections' && params.offset === 50)
        return { ok: false, code: 'conflict' };
      return {
        ok: true,
        value: {
          kind: 'catalogue',
          page: {
            version: attempt,
            layoutRevision: attempt,
            groups:
              params.kind === 'groups' ? [{ id: attempt === 1 ? 'stale-group' : 'group' }] : [],
            connections:
              params.kind === 'connections'
                ? Array.from({ length: params.offset === 0 ? 50 : 13 }, (_, i) => ({
                    id: attempt === 1 ? `stale-${i}` : `connection-${params.offset + i}`,
                  }))
                : [],
            next: params.kind === 'connections' && params.offset === 0 ? 50 : null,
          },
        },
      };
    },
  );
  const store = new WorkspaceStore({ listCatalogue } as unknown as WorkspaceApi);
  const published: string[] = [];
  const unsubscribe = store.subscribe(() => {
    const state = store.getSnapshot();
    published.push(...state.groups.map((g) => g.id), ...state.connections.map((c) => c.id));
  });
  try {
    await store.initialize();
    expect(store.getSnapshot()).toMatchObject({ ready: true, version: 2, layoutRevision: 2 });
    expect(store.getSnapshot().groups.map((g) => g.id)).toEqual(['group']);
    expect(store.getSnapshot().connections.map((c) => c.id)).toEqual(
      Array.from({ length: 63 }, (_, i) => `connection-${i}`),
    );
    expect(published.some((id) => id.startsWith('stale'))).toBe(false);
    expect(listCatalogue.mock.calls.map(([params]) => params)).toEqual([
      { kind: 'groups', query: '', offset: 0, version: null },
      { kind: 'connections', query: '', offset: 0, version: 1 },
      { kind: 'connections', query: '', offset: 50, version: 1 },
      { kind: 'groups', query: '', offset: 0, version: null },
      { kind: 'connections', query: '', offset: 0, version: 2 },
      { kind: 'connections', query: '', offset: 50, version: 2 },
    ]);
  } finally {
    unsubscribe();
    store.dispose();
  }
});

it('rejects a nonadvancing catalogue page without publishing partial groups or connections', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const listCatalogue = vi.fn(async () => ({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        version: 1,
        layoutRevision: 1,
        groups: [{ id: 'partial-group' }],
        connections: [],
        next: 0,
      },
    },
  }));
  const store = new WorkspaceStore({ listCatalogue } as unknown as WorkspaceApi);
  try {
    await store.initialize();
    expect(store.getSnapshot()).toMatchObject({
      ready: false,
      error: 'protocol_mismatch',
      groups: [],
      connections: [],
    });
    expect(listCatalogue).toHaveBeenCalledTimes(1);
  } finally {
    store.dispose();
  }
});

it('reconciles a timed-out mutation once without replay and preserves the group draft on conflict', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const updateLayout = vi.fn().mockResolvedValue({
    ok: false,
    code: 'timeout',
    requestId: 'request',
    runtimeId: 'runtime',
  });
  const reconcile = vi.fn().mockResolvedValue({
    ok: true,
    value: { kind: 'reconciled', result: null, error: 'conflict' },
  });
  const listCatalogue = vi.fn().mockResolvedValue({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        version: 4,
        layoutRevision: 2,
        groups: [],
        connections: [],
        next: null,
      },
    },
  });
  const store = new WorkspaceStore({
    updateLayout,
    reconcile,
    listCatalogue,
  } as unknown as WorkspaceApi);
  store.setGroupEditing({
    id: 'group',
    surfaceId: 'preview',
    draft: '未保存',
    error: null,
  });
  await expect(
    store.commitLayout({
      revision: 1,
      groupIds: [],
      buckets: [{ groupId: null, connectionIds: [] }],
    }),
  ).rejects.toThrow('conflict');
  expect(updateLayout).toHaveBeenCalledTimes(1);
  expect(reconcile).toHaveBeenCalledWith({
    requestId: 'request',
    runtimeId: 'runtime',
  });
  expect(store.getSnapshot().groupEditing?.draft).toBe('未保存');
  expect(store.getSnapshot().layoutRevision).toBe(2);
  expect(store.getSnapshot().groupBusy).toBe(false);
  store.dispose();
});

it('keeps the ready catalogue and interactivity on background read failure', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const listCatalogue = vi.fn(async () => ({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        version: 4,
        layoutRevision: 2,
        groups: [],
        connections: [],
        next: null,
      },
    },
  }));
  const store = new WorkspaceStore({ listCatalogue } as unknown as WorkspaceApi);
  await store.initialize();
  expect(store.getSnapshot().ready).toBe(true);
  const connections = store.getSnapshot().connections;
  listCatalogue.mockResolvedValue({ ok: false, code: 'timeout' } as never);
  await expect(store.refresh()).rejects.toMatchObject({ code: 'timeout' });
  expect(store.getSnapshot().connections).toBe(connections);
  expect(store.getSnapshot()).toMatchObject({
    ready: true,
    loading: false,
    error: null,
  });
  expect(store.feedback.forOwner('navigation:connections')?.phase).toBe('failed');
  store.dispose();
});

it.each(
  (['cancel', 'background', 'cancelTransfers'] as const).flatMap((choice) =>
    (['single', 'batch'] as const).map((mode) => ({ choice, mode })),
  ),
)(
  '$mode last-terminal close obeys the explicit $choice transfer decision',
  async ({ choice, mode }) => {
    let notify!: (note: import('@contracts').WorkspaceNotification) => void;
    Object.assign(window, {
      vigilshell: {
        onWorkspaceEvent: (listener: typeof notify) => {
          notify = listener;
          return () => {};
        },
      },
    });
    const order: string[] = [];
    let phase = 'running';
    const tasks = () => [
      {
        id: 'transfer',
        connectionId: 'connection',
        groupId: 'group',
        batchId: 'batch',
        remotePath: '/fixture',
        remoteIdentity: 'fixture@localhost:22',
        attempt: 1,
        revision: phase === 'running' ? 1 : 2,
        conflictVersion: 0,
        canRetry: false,
        name: 'fixture',
        direction: 'upload',
        phase,
        bytes: 0,
        total: 100,
        error: null,
      },
    ];
    const closeTerminal = vi.fn(async () => {
      order.push('close');
      return { ok: true, value: { kind: 'done' } };
    });
    const cancelTransfer = vi.fn(async () => {
      order.push('cancel');
      phase = 'cancelled';
      return { ok: true, value: { kind: 'done' } };
    });
    const store = new WorkspaceStore({
      listCatalogue: vi.fn(async () => ({
        ok: true,
        value: {
          kind: 'catalogue',
          page: {
            version: 1,
            layoutRevision: 1,
            groups: [],
            connections: [],
            next: null,
          },
        },
      })),
      listTransfers: vi.fn(async () => ({
        ok: true,
        value: {
          kind: 'transferPage',
          requestId: 'request',
          next: null,
          tasks: tasks(),
        },
      })),
      closeTerminal,
      cancelTransfer,
    } as unknown as WorkspaceApi);
    store.backend('runtime', false);
    await store.initialize();
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'runtime',
        owner: 'owner',
        event: {
          kind: 'session',
          session: {
            id: 'session',
            groupId: 'group',
            connectionId: 'connection',
            connectionRevision: 1,
            operationId: 'op',
            name: 'fixture',
            host: 'localhost',
            username: 'fixture',
            port: 22,
            phase: 'running',
            error: null,
            exitCode: null,
            terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
          },
        },
      },
    });
    await store.call('listTransfers', {});
    const guard = vi.fn().mockResolvedValue(choice);
    store.setTerminalCloseGuard(guard);
    if (mode === 'batch') await store.closeTerminals(['session']);
    else await store.closeTerminal('session');
    expect(guard).toHaveBeenCalledTimes(1);
    if (choice === 'cancel') {
      expect(order).toEqual([]);
      expect(store.getSnapshot().sessions).toHaveLength(1);
    } else {
      expect(order).toEqual(choice === 'background' ? ['close'] : ['cancel', 'close']);
      expect(store.getSnapshot().sessions).toHaveLength(0);
    }
    store.dispose();
  },
);

it('closing an editor removes its previous test failure without affecting another owner', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const store = new WorkspaceStore({} as WorkspaceApi);
  const { WorkspaceFailure } = await import('./workspace-failure');
  await store.newConnection();
  const owner = 'editor:' + store.getSnapshot().editor!.token;
  store.feedback.report(
    { owner, action: 'ws_test', label: 'fixture' },
    new WorkspaceFailure('authentication_failed'),
  );
  store.feedback.report(
    { owner: 'connection:other', action: 'ws_connect', label: 'other' },
    new WorkspaceFailure('connection_refused'),
  );
  store.closeEditor();
  expect(store.getSnapshot().editor).toBeNull();
  expect(store.feedback.forOwner(owner)).toBeUndefined();
  expect(store.feedback.forOwner('connection:other')?.error?.code).toBe('connection_refused');
  store.dispose();
});

it.each([true, false])(
  'late startup read recovery restores ready and clears only its feedback when unlocked=%s',
  async (unlocked) => {
    let notify!: (note: import('@contracts').WorkspaceNotification) => void;
    Object.assign(window, {
      vigilshell: {
        onWorkspaceEvent: (listener: typeof notify) => {
          notify = listener;
          return () => {};
        },
      },
    });
    const listCatalogue = vi.fn().mockResolvedValue({ ok: false, code: 'timeout' });
    const mutation = vi.fn().mockResolvedValue({ ok: false, code: 'timeout' });
    const store = new WorkspaceStore({
      listCatalogue,
      deleteConnection: mutation,
    } as unknown as WorkspaceApi);
    store.backend('runtime', false);
    await store.initialize();
    await expect(
      store.run({
        owner: 'connection:fixture',
        action: 'ws_delete',
        label: 'fixture',
        execute: () =>
          store.call('deleteConnection', {
            id: '00000000-0000-4000-8000-000000000001',
            version: 1,
            disconnectSessions: false,
            sessionIds: [],
          }),
      }),
    ).rejects.toMatchObject({ code: 'timeout' });
    expect(store.getSnapshot()).toMatchObject({ ready: false, error: 'timeout' });
    listCatalogue.mockResolvedValue(
      unlocked
        ? {
            ok: true,
            value: {
              kind: 'catalogue',
              page: { version: 1, layoutRevision: 1, groups: [], connections: [], next: null },
            },
          }
        : { ok: false, code: 'credential_denied' },
    );
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'old-runtime',
        owner: 'owner',
        event: { kind: 'catalogue' },
      },
    });
    expect(listCatalogue).toHaveBeenCalledTimes(1);
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'runtime',
        owner: 'owner',
        event: { kind: 'catalogue' },
      },
    });
    await vi.waitFor(() => {
      expect(store.getSnapshot()).toMatchObject({
        ready: unlocked,
        loading: false,
        error: unlocked ? null : 'credential_denied',
      });
      expect(store.feedback.forOwner('navigation:connections')?.error?.code).toBe(
        unlocked ? undefined : 'credential_denied',
      );
    });
    expect(store.feedback.forOwner('connection:fixture')?.error?.code).toBe('timeout');
    expect(mutation).toHaveBeenCalledOnce();
    store.dispose();
  },
);

it('opens new conflicts automatically, merges a batch, queues other batches and does not reopen dismissed versions', () => {
  let notify!: (note: import('@contracts').WorkspaceNotification) => void;
  Object.assign(window, {
    vigilshell: {
      onWorkspaceEvent: (listener: typeof notify) => {
        notify = listener;
        return () => {};
      },
    },
  });
  const store = new WorkspaceStore({} as WorkspaceApi);
  store.backend('runtime', false);
  function send(
    id: string,
    batchId: string,
    version = 1,
    revision = 1,
    runtimeId = 'runtime',
    phase = 'awaitingDecision',
  ) {
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId,
        owner: 'owner',
        event: {
          kind: 'transfer',
          task: {
            id,
            batchId,
            connectionId: 'connection',
            groupId: 'group',
            phase,
            attempt: 1,
            conflictVersion: version,
            revision,
            createdAt: 1,
            batchSize: 2,
            name: id,
            direction: 'download',
            bytes: 0,
            total: null,
            error: null,
            remotePath: '/fixture',
            remoteIdentity: 'fixture',
            canRetry: false,
            conflict: null,
          } as import('@contracts').TransferView,
        },
      },
    });
  }
  try {
    send('a', 'batch');
    expect(store.getSnapshot().transferConflictTarget).toMatchObject({
      taskId: 'a',
      batchId: 'batch',
      runtimeId: 'runtime',
    });
    send('b', 'batch');
    send('c', 'another');
    expect(store.getSnapshot().transferConflictTarget?.batchId).toBe('batch');
    store.closeTransferConflicts();
    expect(store.getSnapshot().transferConflictTarget?.batchId).toBe('another');
    store.closeTransferConflicts();
    expect(store.getSnapshot().transferConflictTarget).toBeNull();
    send('a', 'batch', 1, 2);
    send('b', 'batch', 1, 2);
    send('c', 'another', 1, 2);
    expect(store.getSnapshot().transferConflictTarget).toBeNull();
    send('a', 'batch', 2, 3);
    expect(store.getSnapshot().transferConflictTarget?.taskId).toBe('a');
    store.closeTransferConflicts();
    send('a', 'batch', 1, 1);
    send('stale', 'old', 1, 1, 'retired-runtime');
    expect(store.getSnapshot().transferConflictTarget).toBeNull();
    store.showTransferConflicts('b');
    expect(store.getSnapshot().transferConflictTarget?.taskId).toBe('b');
    store.closeTransferConflicts();
    send('d', 'done', 1, 1, 'runtime', 'complete');
    expect(store.getSnapshot().transferConflictTarget).toBeNull();
  } finally {
    store.dispose();
  }
});

it('does not reveal transfers or open a connection when its terminal tab has closed', () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const store = new WorkspaceStore({} as WorkspaceApi);
  try {
    store.showTransfersInSidebar('background');
    expect(store.getSnapshot()).toMatchObject({
      toolsTab: 'overview',
      toolsReveal: 0,
      sessions: [],
      transferWindows: [],
    });
  } finally {
    store.dispose();
  }
});

it('isolates pending key authorization from communication and resumes only catalogue reads', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const listCatalogue = vi.fn().mockResolvedValue({
    ok: true,
    value: {
      kind: 'catalogue',
      page: { version: 1, layoutRevision: 1, groups: [], connections: [], next: null },
    },
  });
  const save = vi.fn();
  const store = new WorkspaceStore({
    listCatalogue,
    createConnection: save,
  } as unknown as WorkspaceApi);
  store.backend('runtime', true);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'unlocking', revision: 1, code: null },
  });
  await store.refresh();
  expect(listCatalogue.mock.calls.every(([params]) => params.kind === 'groups')).toBe(true);
  expect(store.getSnapshot()).toMatchObject({
    transportReady: true,
    ready: false,
    loading: true,
    error: null,
  });
  store.acceptConnectionAccess({
    runtimeId: 'old-runtime',
    state: { phase: 'ready', revision: 3, code: null },
  });
  expect(listCatalogue.mock.calls.every(([params]) => params.kind === 'groups')).toBe(true);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'ready', revision: 2, code: null },
  });
  await vi.waitFor(() => expect(store.getSnapshot().ready).toBe(true));
  expect(listCatalogue).toHaveBeenCalledTimes(3);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'ready', revision: 2, code: null },
  });
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'unlocking', revision: 1, code: null },
  });
  expect(listCatalogue).toHaveBeenCalledTimes(3);
  expect(save).not.toHaveBeenCalled();
  store.dispose();
});
it('keeps unlock denial local and starts exactly one explicit retry without replaying writes', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const listCatalogue = vi.fn().mockResolvedValue({
    ok: true,
    value: {
      kind: 'catalogue',
      page: { version: 1, layoutRevision: 1, groups: [], connections: [], next: null },
    },
  });
  const unlockConnections = vi.fn().mockResolvedValue({
    ok: true,
    value: { kind: 'connectionAccess', state: { phase: 'unlocking', revision: 2, code: null } },
  });
  const store = new WorkspaceStore({ listCatalogue, unlockConnections } as unknown as WorkspaceApi);
  store.backend('runtime', true);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'blocked', revision: 1, code: 'credential_denied' },
  });
  await store.refresh();
  expect(store.getSnapshot()).toMatchObject({
    transportReady: true,
    ready: false,
    loading: false,
    error: 'credential_denied',
  });
  expect(listCatalogue.mock.calls.every(([params]) => params.kind === 'groups')).toBe(true);
  await store.retryConnections();
  expect(unlockConnections).toHaveBeenCalledOnce();
  expect(listCatalogue.mock.calls.every(([params]) => params.kind === 'groups')).toBe(true);
  expect(store.getSnapshot()).toMatchObject({ transportReady: true, loading: true, error: null });
  store.dispose();
});

it('does not let an old catalogue completion override a newer blocked key state', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  let finish!: (value: unknown) => void;
  const listCatalogue = vi
    .fn()
    .mockResolvedValue({
      ok: true,
      value: {
        kind: 'catalogue',
        page: { version: 1, layoutRevision: 1, groups: [], connections: [], next: null },
      },
    })
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
  const store = new WorkspaceStore({ listCatalogue } as unknown as WorkspaceApi);
  store.backend('runtime', true);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'ready', revision: 1, code: null },
  });
  await vi.waitFor(() => expect(listCatalogue).toHaveBeenCalledOnce());
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'blocked', revision: 2, code: 'configuration_corrupt' },
  });
  finish({
    ok: true,
    value: {
      kind: 'catalogue',
      page: { version: 1, layoutRevision: 1, groups: [], connections: [], next: null },
    },
  });
  await Promise.resolve();
  await Promise.resolve();
  expect(store.getSnapshot()).toMatchObject({
    transportReady: true,
    ready: false,
    loading: false,
    error: 'configuration_corrupt',
  });
  expect(listCatalogue).toHaveBeenCalledTimes(2);
  store.dispose();
});

it('loads plaintext groups during unlock without requesting or inventing encrypted connections', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const listCatalogue = vi.fn().mockResolvedValue({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        version: 1,
        layoutRevision: 1,
        groups: [{ id: 'group', name: 'Visible group', version: 1, sortOrder: 0 }],
        connections: [],
        next: null,
      },
    },
  });
  const store = new WorkspaceStore({ listCatalogue } as unknown as WorkspaceApi);
  store.backend('runtime', true);
  store.acceptConnectionAccess({
    runtimeId: 'runtime',
    state: { phase: 'unlocking', revision: 1, code: null },
  });
  await vi.waitFor(() => expect(store.getSnapshot().groupsReady).toBe(true));
  expect(store.getSnapshot()).toMatchObject({
    ready: false,
    transportReady: true,
    loading: true,
    groups: [{ name: 'Visible group' }],
    connections: [],
  });
  expect(listCatalogue.mock.calls.map(([params]) => params.kind)).toEqual(['groups']);
  store.dispose();
});

const detailReply = (id: string, name = id) => ({
  ok: true as const,
  value: {
    kind: 'connection' as const,
    connection: { id, config: { name } },
    trust: { fingerprint: name },
  },
});
const missingConnection = { ok: false as const, code: 'not_found' };

it.each([true, false])('preserves B when A is missing (A completes first: %s)', async (aFirst) => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const a = deferred<typeof missingConnection>();
  const b = deferred<ReturnType<typeof detailReply>>();
  const store = new WorkspaceStore({
    getConnection: ({ id }: { id: string }) => (id === 'a' ? a.promise : b.promise),
  } as unknown as WorkspaceApi);
  const first = store.select({ kind: 'connection', id: 'a' });
  const second = store.select({ kind: 'connection', id: 'b' });
  if (aFirst) {
    a.resolve(missingConnection);
    await first;
  }
  b.resolve(detailReply('b'));
  await second;
  a.resolve(missingConnection);
  await first;
  expect(store.getSnapshot()).toMatchObject({
    selected: { id: 'b' },
    details: { id: 'b' },
    trust: { fingerprint: 'b' },
  });
  store.dispose();
});

it('clears the active details and trust when the selected connection has been deleted', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const getConnection = vi
    .fn()
    .mockResolvedValueOnce(detailReply('a'))
    .mockResolvedValueOnce(missingConnection);
  const store = new WorkspaceStore({ getConnection } as unknown as WorkspaceApi);
  await store.select({ kind: 'connection', id: 'a' });
  expect(store.getSnapshot().trust).toMatchObject({ fingerprint: 'a' });
  await store.select({ kind: 'connection', id: 'a' });
  expect(store.getSnapshot()).toMatchObject({ selected: null, details: null, trust: null });
  store.dispose();
});

it.each([missingConnection, detailReply('a', 'old')])(
  'drops a previous runtime detail reply for the same id',
  async (late) => {
    Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
    const stale = deferred<typeof late>();
    const getConnection = vi
      .fn()
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce(detailReply('a', 'new'));
    const store = new WorkspaceStore({ getConnection } as unknown as WorkspaceApi);
    store.backend('old-runtime', false);
    const first = store.select({ kind: 'connection', id: 'a' });
    store.backend('new-runtime', false);
    await store.select({ kind: 'connection', id: 'a' });
    stale.resolve(late);
    await first;
    expect(getConnection).toHaveBeenCalledTimes(2);
    expect(store.getSnapshot()).toMatchObject({
      selected: { id: 'a' },
      details: { config: { name: 'new' } },
      trust: { fingerprint: 'new' },
    });
    store.dispose();
  },
);

it('re-reads selected details after a same-runtime move and uses the new group immediately', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const reply = (groupId: string) => {
    const detail = detailReply('a', groupId);
    return {
      ...detail,
      value: { ...detail.value, connection: { ...detail.value.connection, groupId, version: 1 } },
    };
  };
  const old = deferred<ReturnType<typeof reply>>();
  const current = deferred<ReturnType<typeof reply>>();
  const getConnection = vi
    .fn()
    .mockReturnValueOnce(old.promise)
    .mockReturnValueOnce(current.promise);
  const listCatalogue = vi.fn(async ({ kind }: { kind: string }) => ({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        version: 2,
        layoutRevision: 2,
        groups: [],
        connections: kind === 'connections' ? [{ id: 'a', groupId: 'new-group', version: 1 }] : [],
        next: null,
      },
    },
  }));
  const store = new WorkspaceStore({ getConnection, listCatalogue } as unknown as WorkspaceApi);
  try {
    const selected = store.select({ kind: 'connection', id: 'a' });
    const refreshed = store.refresh();
    await vi.waitFor(() => expect(store.getSnapshot().layoutRevision).toBe(2));
    await store.newConnectionForSelection();
    expect(store.getSnapshot().editor?.groupId).toBe('new-group');
    old.resolve(reply('old-group'));
    await selected;
    await vi.waitFor(() => expect(getConnection).toHaveBeenCalledTimes(2));
    expect(store.getSnapshot().details).toBeNull();
    expect(store.getSnapshot().trust).toBeNull();
    current.resolve(reply('new-group'));
    await refreshed;
    expect(store.getSnapshot().details?.groupId).toBe('new-group');
    expect(store.getSnapshot().trust?.fingerprint).toBe('new-group');
  } finally {
    store.dispose();
  }
});

it('ignores a missing response from before the same connection was selected again', async () => {
  Object.assign(window, { vigilshell: { onWorkspaceEvent: () => () => {} } });
  const old = deferred<typeof missingConnection>();
  const getConnection = vi
    .fn()
    .mockReturnValueOnce(old.promise)
    .mockResolvedValueOnce(detailReply('b'))
    .mockResolvedValueOnce(detailReply('a', 'reselected'));
  const store = new WorkspaceStore({ getConnection } as unknown as WorkspaceApi);
  try {
    const first = store.select({ kind: 'connection', id: 'a' });
    await store.select({ kind: 'connection', id: 'b' });
    const last = store.select({ kind: 'connection', id: 'a' });
    old.resolve(missingConnection);
    await Promise.all([first, last]);
    expect(getConnection).toHaveBeenCalledTimes(3);
    expect(store.getSnapshot()).toMatchObject({
      selected: { id: 'a' },
      details: { id: 'a' },
      trust: { fingerprint: 'reselected' },
    });
  } finally {
    store.dispose();
  }
});

function groupFixture() {
  let listener: Parameters<NonNullable<typeof window.vigilshell.onWorkspaceEvent>>[0];
  let runtime = 'old-runtime';
  const a = { id: 'a', name: 'Group A', sortOrder: 0, version: 1 };
  const b = { id: 'b', name: 'Group B', sortOrder: 1, version: 1 };
  const groups = [a, b];
  Object.assign(window, {
    vigilshell: {
      onWorkspaceEvent: (receive: typeof listener) => {
        listener = receive;
        return () => {};
      },
    },
  });
  const listCatalogue = vi.fn(async ({ kind }: { kind: string }) => ({
    ok: true,
    value: {
      kind: 'catalogue',
      page: {
        groups: kind === 'groups' ? [...groups] : [],
        connections:
          kind === 'connections' ? [{ id: 'healthy', groupId: 'b', groupSortOrder: 0 }] : [],
        unavailableConnections:
          kind === 'connections' ? [{ id: 'unreadable', groupId: 'a', groupSortOrder: 0 }] : [],
        version: 1,
        layoutRevision: 1,
        next: null,
      },
    },
  }));
  const getConnection = vi.fn(async () => detailReply('healthy'));
  const reconcile = vi.fn();
  const updateGroup = vi.fn(async ({ id, name }: { id: string; name: string }) => {
    const group = { ...groups.find((g) => g.id === id)!, name, version: 2 };
    groups[group.sortOrder] = group;
    listener({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: { runtimeId: runtime, owner: 'owner', event: { kind: 'groupRenamed', group } },
    });
    return { ok: true, value: { kind: 'group', group } };
  });
  const store = new WorkspaceStore({
    listCatalogue,
    getConnection,
    reconcile,
    updateGroup,
  } as unknown as WorkspaceApi);
  const restart = async (next: string) => {
    runtime = next;
    store.backend(runtime, false);
    await store.initialize();
  };
  const edit = (id: string, draft: string) =>
    store.setGroupEditing({ id, draft, surfaceId: 'sidebar', error: null });
  return { store, a, b, listCatalogue, getConnection, updateGroup, reconcile, restart, edit };
}

it('applies the rename notification and reply without reading any catalogue page or selected details', async () => {
  const f = groupFixture();
  await f.restart('old-runtime');
  await f.store.select({ kind: 'connection', id: 'healthy' });
  const layout = f.store.currentLayout();
  expect(layout.buckets).toContainEqual({ groupId: 'a', connectionIds: ['unreadable'] });
  f.listCatalogue.mockClear();
  f.getConnection.mockClear();
  f.edit('a', 'Renamed');
  await f.store.renameGroup(f.a, 'Renamed');
  expect(f.store.getSnapshot().groups[0]).toMatchObject({ name: 'Renamed', version: 2 });
  expect(f.store.getSnapshot().details).toMatchObject({ id: 'healthy' });
  expect(f.store.currentLayout()).toEqual(layout);
  expect(f.listCatalogue).not.toHaveBeenCalled();
  expect(f.getConnection).not.toHaveBeenCalled();
  f.store.dispose();
});

it.each(['applied', 'conflict'])(
  'reconciles a lost rename response as %s using the original request once',
  async (state) => {
    const f = groupFixture();
    await f.restart('old-runtime');
    f.edit('a', 'Renamed');
    f.updateGroup.mockResolvedValueOnce({
      ok: false,
      code: 'timeout',
      requestId: 'original',
      runtimeId: 'old-runtime',
    } as never);
    f.reconcile.mockResolvedValue({
      ok: true,
      value: {
        kind: 'reconciled',
        result:
          state === 'applied'
            ? { kind: 'group', group: { ...f.a, name: 'Renamed', version: 2 } }
            : null,
        error: state === 'conflict' ? 'conflict' : null,
      },
    });
    await f.store.renameGroup(f.a, 'Renamed');
    expect(f.updateGroup).toHaveBeenCalledTimes(1);
    expect(f.reconcile).toHaveBeenCalledExactlyOnceWith({
      requestId: 'original',
      runtimeId: 'old-runtime',
    });
    if (state === 'applied') {
      expect(f.store.getSnapshot().groups[0]?.name).toBe('Renamed');
      expect(f.store.getSnapshot().groupEditing).toBeNull();
    } else {
      expect(f.store.getSnapshot().groupEditing).toMatchObject({
        draft: 'Renamed',
        error: 'conflict',
      });
    }
    f.store.dispose();
  },
);

it('retains the unknown rename after restart without locking B or applying A’s late receipt', async () => {
  const f = groupFixture();
  await f.restart('old-runtime');
  f.edit('a', 'Uncertain');
  const original = deferred<unknown>();
  f.reconcile.mockReturnValueOnce(original.promise);
  f.updateGroup.mockResolvedValueOnce({
    ok: false,
    code: 'timeout',
    requestId: 'original',
    runtimeId: 'old-runtime',
  } as never);
  const a = f.store.renameGroup(f.a, 'Uncertain');
  await vi.waitFor(() => expect(f.reconcile).toHaveBeenCalledTimes(1));
  expect(f.store.feedback.isBusy('group:a')).toBe(true);
  await f.restart('new-runtime');
  f.edit('b', 'B draft');
  original.resolve({
    ok: true,
    value: {
      kind: 'reconciled',
      result: { kind: 'group', group: { ...f.a, name: 'Uncertain', version: 2 } },
      error: null,
    },
  });
  await a;
  expect(f.store.getSnapshot().groupEditing).toMatchObject({
    id: 'b',
    draft: 'B draft',
    error: null,
  });
  expect(f.store.feedback.forOwner('group:a')).toMatchObject({
    phase: 'unknown',
    error: { requestId: 'original', runtimeId: 'old-runtime' },
  });
  expect(f.store.feedback.isBusy('group:a')).toBe(false);
  await f.store.renameGroup(f.b, 'B draft');
  expect(f.updateGroup.mock.calls.map(([p]) => p.id)).toEqual(['a', 'b']);
  expect(f.store.getSnapshot().groups[1]).toMatchObject({ name: 'B draft', version: 2 });
  f.store.dispose();
});
