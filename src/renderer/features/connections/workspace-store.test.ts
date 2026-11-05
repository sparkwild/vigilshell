// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import { WorkspaceStore } from './workspace-store';
import type { WorkspaceApi } from '../../../shared/workspace-api';

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
