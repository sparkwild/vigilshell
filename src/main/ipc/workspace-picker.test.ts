import { beforeEach, expect, it, vi } from 'vitest';
const fixture = vi.hoisted(() => ({
  handlers: new Map<
    string,
    (event: unknown, params: { direction: 'upload' | 'download' }) => Promise<unknown>
  >(),
  pick: vi.fn(),
  warning: vi.fn().mockResolvedValue({ response: 0 }),
  stat: vi.fn(),
  access: vi.fn(),
}));
vi.mock('electron', () => ({
  clipboard: {},
  dialog: { showOpenDialog: fixture.pick, showMessageBox: fixture.warning },
  ipcMain: {
    handle: vi.fn((name, fn) => fixture.handlers.set(name, fn)),
    on: vi.fn(),
  },
  MessageChannelMain: vi.fn(),
}));
vi.mock('node:fs/promises', () => ({
  stat: fixture.stat,
  access: fixture.access,
}));
import { defaultPreferences } from '../../shared/preferences';
import type { DesktopSnapshot, ConnectionAccessState, WorkspaceReply } from '@contracts';
import type { UiStateService } from '../platform/ui-state';
import { WorkspaceIpc } from './workspace';
import type { BackendClient } from '../backend/client';
beforeEach(() => {
  fixture.handlers.clear();
  fixture.pick.mockReset();
});
it('remembers upload and download folders separately, ignoring cancelled dialogs', async () => {
  const host = {
    on: vi.fn(),
    instanceId: 'runtime',
    workspaceCall: vi.fn(() => ({ result: Promise.resolve({ kind: 'done' }) })),
  } as unknown as BackendClient;
  const ipc = new WorkspaceIpc(
    host,
    () => undefined,
    () => true,
  );
  vi.spyOn(ipc as unknown as { ensure(): Promise<void> }, 'ensure').mockResolvedValue();
  const pick = fixture.handlers.get('workspace:pickFiles')!;
  fixture.pick.mockResolvedValueOnce({
    canceled: false,
    filePaths: ['/tmp/upload/a.txt'],
  });
  await pick({}, { direction: 'upload' });
  fixture.pick.mockResolvedValueOnce({
    canceled: false,
    filePaths: ['/tmp/download'],
  });
  await pick({}, { direction: 'download' });
  expect(fixture.pick.mock.calls[1]![0].defaultPath).toBeUndefined();
  fixture.pick.mockResolvedValueOnce({ canceled: true, filePaths: [] });
  await pick({}, { direction: 'upload' });
  expect(fixture.pick.mock.calls[2]![0].defaultPath).toBe('/tmp/upload');
  fixture.pick.mockResolvedValueOnce({ canceled: true, filePaths: [] });
  await pick({}, { direction: 'download' });
  expect(fixture.pick.mock.calls[3]![0].defaultPath).toBe('/tmp/download');
  fixture.pick.mockResolvedValueOnce({ canceled: true, filePaths: [] });
  await pick({}, { direction: 'upload' });
  expect(fixture.pick.mock.calls[4]![0].defaultPath).toBe('/tmp/upload');
});

it('grants the native saved download directory without opening a picker or changing last-used location', async () => {
  const host = {
    on: vi.fn(),
    instanceId: 'runtime',
    workspaceCall: vi.fn(() => ({ result: Promise.resolve({ kind: 'done' }) })),
  } as unknown as BackendClient;
  const ui = {
    get: vi.fn(async () => ({
      data: { kind: 'pickers', upload: '/uploads', download: '/previous' },
    })),
    set: vi.fn(),
  };
  const ipc = new WorkspaceIpc(
    host,
    () => undefined,
    () => true,
    {
      state: () =>
        ({
          preferences: {
            ...defaultPreferences,
            downloadMode: 'directory',
            downloadDirectory: '/native-chosen',
          },
        }) as DesktopSnapshot,
      ui: ui as unknown as UiStateService,
    },
  );
  vi.spyOn(ipc as unknown as { ensure(): Promise<void> }, 'ensure').mockResolvedValue();
  fixture.stat.mockResolvedValue({ isDirectory: () => true });
  fixture.access.mockResolvedValue(undefined);
  await fixture.handlers.get('workspace:pickFiles')!({}, { direction: 'download' });
  expect(fixture.pick).not.toHaveBeenCalled();
  expect(host.workspaceCall).toHaveBeenLastCalledWith(expect.any(String), {
    method: 'files.grant',
    params: { direction: 'download', paths: ['/native-chosen'] },
  });
  expect(ui.set).not.toHaveBeenCalled();
});
it('warns and asks again when the configured download directory is unavailable', async () => {
  const host = {
    on: vi.fn(),
    instanceId: 'runtime',
    workspaceCall: vi.fn(() => ({ result: Promise.resolve({ kind: 'done' }) })),
  } as unknown as BackendClient;
  const ui = {
    get: vi.fn(async () => ({
      data: { kind: 'pickers', upload: '', download: '/previous' },
    })),
    set: vi.fn(),
  };
  const ipc = new WorkspaceIpc(
    host,
    () => undefined,
    () => true,
    {
      state: () =>
        ({
          preferences: {
            ...defaultPreferences,
            downloadMode: 'directory',
            downloadDirectory: '/missing',
          },
        }) as DesktopSnapshot,
      ui: ui as unknown as UiStateService,
    },
  );
  vi.spyOn(ipc as unknown as { ensure(): Promise<void> }, 'ensure').mockResolvedValue();
  fixture.stat.mockRejectedValue(Error('ENOENT'));
  fixture.pick.mockResolvedValue({ canceled: true, filePaths: [] });
  await fixture.handlers.get('workspace:pickFiles')!({}, { direction: 'download' });
  expect(fixture.warning).toHaveBeenCalled();
  expect(fixture.pick).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: '/previous' }));
  expect(host.workspaceCall).not.toHaveBeenCalled();
  expect(ui.set).not.toHaveBeenCalled();
});

it('initial navigation does not detach an owner that never negotiated', async () => {
  const host = {
    on: vi.fn(),
    instanceId: 'runtime',
    workspaceCall: vi.fn(),
  } as unknown as BackendClient;
  const ipc = new WorkspaceIpc(
    host,
    () => undefined,
    () => true,
  );
  await ipc.navigation();
  expect(host.workspaceCall).not.toHaveBeenCalled();
});
it('navigation during negotiation releases only the owner whose late handshake succeeded', async () => {
  let finish!: (reply: WorkspaceReply) => void;
  const handshake = new Promise<WorkspaceReply>((resolve) => {
    finish = resolve;
  });
  const call = vi.fn((_owner: string, request: { method: string }) => ({
    result:
      request.method === 'workspace.negotiate' ? handshake : Promise.resolve({ kind: 'done' }),
  }));
  const host = {
    on: vi.fn(),
    instanceId: 'runtime',
    workspaceCall: call,
  } as unknown as BackendClient;
  const ipc = new WorkspaceIpc(
    host,
    () => undefined,
    () => true,
  );
  const attach = (ipc as unknown as { ensure(): Promise<void> }).ensure();
  const failed = expect(attach).rejects.toThrow('runtime_unavailable');
  await ipc.navigation();
  expect(call).toHaveBeenCalledTimes(1);
  finish({
    kind: 'ready',
    version: 16,
    connectionAccess: { phase: 'locked', revision: 0, code: null },
  });
  await failed;
  expect(call.mock.calls.map(([, c]) => c.method)).toEqual([
    'workspace.negotiate',
    'workspace.detach',
  ]);
  expect(call.mock.calls[0]![0]).toBe(call.mock.calls[1]![0]);
});

it.each(['current', 'owner-replaced', 'runtime-replaced'] as const)(
  'late negotiation recovery respects %s ownership and never replays a blocked mutation',
  async (state) => {
    const { EventEmitter } = await import('node:events');
    const { BackendError } = await import('../backend/client');
    const send = vi.fn();
    const call = vi.fn((_owner: string, command: { method: string }) => ({
      result:
        command.method === 'workspace.negotiate'
          ? Promise.reject(new BackendError('timeout'))
          : Promise.resolve({ kind: 'done' }),
    }));
    const host = Object.assign(new EventEmitter(), { instanceId: 'runtime', workspaceCall: call });
    const ipc = new WorkspaceIpc(
      host as unknown as BackendClient,
      () => ({ webContents: { send } }) as unknown as import('electron').BrowserWindow,
      () => true,
    );
    const handler = fixture.handlers.get('workspace:deleteConnection')!;
    expect(
      await handler({}, {
        id: '00000000-0000-4000-8000-000000000001',
        version: 1,
        disconnectSessions: false,
        sessionIds: [],
      } as never),
    ).toMatchObject({
      ok: false,
      code: 'timeout',
    });
    const owner = call.mock.calls[0]![0];
    if (state === 'owner-replaced') await ipc.navigation();
    if (state === 'runtime-replaced') {
      host.instanceId = 'replacement';
      host.emit('unavailable');
      send.mockClear();
    }
    host.emit('negotiationRecovered', 'runtime', 'foreign-owner', {
      phase: 'unlocking',
      revision: 1,
      code: null,
    });
    expect(call).toHaveBeenCalledTimes(1);
    host.emit('negotiationRecovered', 'runtime', owner, {
      phase: 'unlocking',
      revision: 1,
      code: null,
    });
    await vi.waitFor(() => {
      if (state === 'current') expect(send).toHaveBeenCalledOnce();
      else
        expect(call.mock.calls.map(([, c]) => c.method)).toEqual(
          state === 'owner-replaced'
            ? ['workspace.negotiate', 'workspace.detach']
            : ['workspace.negotiate'],
        );
    });
    host.emit('negotiationRecovered', 'runtime', owner, {
      phase: 'unlocking',
      revision: 1,
      code: null,
    });
    await Promise.resolve();
    expect(call.mock.calls.map(([, c]) => c.method)).toEqual([
      'workspace.negotiate',
      ...(state === 'current'
        ? ['workspace.context', 'connections.initialize']
        : state === 'owner-replaced'
          ? ['workspace.detach']
          : []),
    ]);
    if (state === 'current')
      expect(send).toHaveBeenCalledWith('workspace:connection-access-changed', {
        runtimeId: 'runtime',
        state: { phase: 'unlocking', revision: 1, code: null },
      });
    else expect(send).not.toHaveBeenCalled();
  },
);

it('caches explicit access snapshots and rejects stale owner/runtime/revision updates', async () => {
  const { EventEmitter } = await import('node:events');
  const send = vi.fn();
  const locked: ConnectionAccessState = { phase: 'locked', revision: 0, code: null };
  const call = vi.fn((_owner: string, c: { method: string }) => ({
    result: Promise.resolve(
      c.method === 'workspace.negotiate'
        ? { kind: 'ready', version: 16, connectionAccess: locked }
        : c.method === 'connections.initialize'
          ? { kind: 'connectionAccess', state: { phase: 'unlocking', revision: 1, code: null } }
          : { kind: 'done' },
    ),
  }));
  const host = Object.assign(new EventEmitter(), { instanceId: 'runtime', workspaceCall: call });
  new WorkspaceIpc(
    host as unknown as BackendClient,
    () => ({ webContents: { send } }) as unknown as import('electron').BrowserWindow,
    () => true,
  );
  const get = fixture.handlers.get('workspace:connection-access')!;
  await get({}, {} as never);
  await Promise.resolve();
  expect(call.mock.calls.map(([, c]) => c.method)).toEqual([
    'workspace.negotiate',
    'workspace.context',
    'connections.initialize',
  ]);
  const owner = call.mock.calls[0]![0];
  const emit = (runtimeId: string, owner: string, state: ConnectionAccessState) =>
    host.emit('workspace', {
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: { runtimeId, owner, event: { kind: 'connectionAccess', state } },
    });
  emit('runtime', owner, { phase: 'ready', revision: 2, code: null });
  emit('runtime', owner, { phase: 'unlocking', revision: 1, code: null });
  emit('old', owner, { phase: 'blocked', revision: 3, code: 'credential_denied' });
  emit('runtime', 'other', { phase: 'blocked', revision: 3, code: 'credential_denied' });
  expect(await get({}, {} as never)).toEqual({
    runtimeId: 'runtime',
    state: { phase: 'ready', revision: 2, code: null },
  });
  expect(call).toHaveBeenCalledTimes(3);
});
