// @vitest-environment node
import { execFileSync } from 'node:child_process';
import { expect, it, vi } from 'vitest';
import { McpController, type McpPorts } from './controller';
import { AgentScreens } from './agent-screens';
import { mcpHttpConfiguration } from '../../shared/mcp-instructions';
import type {
  AgentConfiguration,
  AgentContext,
  AgentMutation,
  AgentOperation,
  AgentReply,
} from '@contracts';
function fixture(approved: boolean) {
  const contexts: AgentContext[] = [];
  const configuration: AgentConfiguration = {
    settings: {
      revision: 0,
      enabled: false,
      port: 0,
      showTray: true,
      retentionDays: 30,
      storageMib: 256,
    },
    entries: [],
    policies: [],
  };
  let change: AgentMutation | undefined;
  const request = vi.fn(async (op: AgentOperation): Promise<AgentReply> => {
    switch (op.operation) {
      case 'configuration':
        return { kind: 'configuration', configuration: structuredClone(configuration) };
      case 'contexts':
        return { kind: 'contexts', contexts: structuredClone(contexts) };
      case 'observations':
        return { kind: 'observations', observations: [] };
      case 'prepare':
        change = op.change;
        return { kind: 'challenge', id: 'once', revision: configuration.settings.revision };
      case 'commit': {
        if (change?.kind === 'provision') {
          configuration.settings = {
            ...configuration.settings,
            enabled: true,
            port: change.port,
            revision: configuration.settings.revision + 1,
          };
          configuration.entries = [change.secret];
        }
        if (change?.kind === 'access') {
          const secret = change.secret;
          configuration.entries = configuration.entries.map((e) =>
            e.entry.id === secret.entry.id ? secret : e,
          );
          configuration.settings.revision++;
        }
        if (change?.kind === 'settingsPatch' && change.patch.group === 'service') {
          configuration.settings = {
            ...configuration.settings,
            enabled: change.patch.enabled,
            port: change.patch.port,
            revision: configuration.settings.revision + 1,
          };
        }
        if (change?.kind === 'settingsPatch' && change.patch.group === 'activity') {
          configuration.settings.retentionDays = change.patch.retentionDays;
          configuration.settings.storageMib = change.patch.storageMib;
          configuration.settings.revision++;
        }
        if (change?.kind === 'settingsPatch' && change.patch.group === 'tray') {
          configuration.settings.showTray = change.patch.showTray;
          configuration.settings.revision++;
        }
        if (change?.kind === 'deleteAccess' || change?.kind === 'revoke') {
          const id = change.id;
          const count = configuration.entries.length;
          configuration.entries = configuration.entries.filter((e) => e.entry.id !== id);
          if (configuration.entries.length !== count) configuration.settings.revision++;
        }
        return { kind: 'done' };
      }
      default:
        return { kind: 'done' };
    }
  });
  const ports: McpPorts = {
    directory: vi.fn(async () => null),
    request,
    detect: vi.fn(async () => []),
    locale: () => 'en',
    scrollback: () => 10000,
    authorize: vi.fn(async () => approved),
    seal: (v) => `protected:${v}`,
    unseal: (v) => v.slice(10),
    copy: vi.fn(),
    changed: vi.fn(),
    tray: vi.fn(),
    notify: vi.fn(),
    cancelAuthorization: vi.fn(),
  };
  return { controller: new McpController(ports), ports, request, configuration, contexts };
}

const pendingContext = (): AgentContext => ({
  id: 'new-session',
  connectionId: 'connection',
  connectionName: 'Server',
  connectionRevision: 1,
  entryId: 'entry',
  entryName: 'Cursor',
  createdAt: 1,
  cwd: '/',
  environment: {},
  phase: 'waitingUser',
  revision: 0,
});

it('preserves MCP output, screen queries and task cancellation after desktop replay removal', async () => {
  const f = fixture(false);
  const screen = vi
    .spyOn(AgentScreens.prototype, 'text')
    .mockResolvedValue('actual terminal screen');
  f.request.mockImplementation(async (operation) => {
    if (operation.operation === 'invoke' && operation.call.tool === 'tasks_query')
      return {
        kind: 'output',
        output: {
          chunks: [{ sequence: 1, stream: 'stdout', text: 'original output' }],
          nextCursor: 1,
          incomplete: false,
        },
      };
    return { kind: 'done' };
  });
  try {
    const query = { tool: 'tasks_query', action: 'output', task_id: 'task', cursor: 0 } as const;
    expect(await f.controller.invoke('digest', { ...query, view: 'stream' })).toMatchObject({
      kind: 'output',
      output: { chunks: [{ text: 'original output' }] },
    });
    expect(screen).not.toHaveBeenCalled();
    expect(await f.controller.invoke('digest', { ...query, view: 'screen' })).toMatchObject({
      kind: 'output',
      output: { screen: 'actual terminal screen', chunks: [{ text: 'original output' }] },
    });
    expect(screen).toHaveBeenCalledExactlyOnceWith('task');
    const cancel = { tool: 'task_cancel', task_id: 'task', request_key: 'cancel' } as const;
    expect(await f.controller.invoke('digest', cancel)).toEqual({ kind: 'done' });
    expect(f.request).toHaveBeenCalledWith({ operation: 'invoke', digest: 'digest', call: cancel });
  } finally {
    screen.mockRestore();
    await f.controller.stop();
  }
});

it.each([
  ['failed', 'authorization_expired'],
  ['failed', 'connection_failed'],
  ['disconnected', 'runtime_unavailable'],
  ['closed', null],
] as const)(
  'keeps a new %s session in the floating notice after %s, refresh and hiding',
  async (phase, error) => {
    const f = fixture(false);
    const context = pendingContext();
    f.contexts.push(context);
    try {
      await f.controller.refresh();
      const ended = { ...context, phase, error, revision: 1 };
      f.controller.event({ kind: 'context', context: ended });
      const expected = { ok: true, snapshot: { views: [{ context: ended }] } };
      expect(await f.controller.ui({ action: 'snapshot' })).toMatchObject(expected);
      await f.controller.refresh();
      expect(await f.controller.ui({ action: 'snapshot' })).toMatchObject(expected);
      await f.controller.ui({ action: 'hideSessionView', sessionId: context.id });
      await f.controller.refresh();
      expect(await f.controller.ui({ action: 'snapshot' })).toMatchObject({
        ok: true,
        snapshot: { views: [] },
      });
    } finally {
      await f.controller.stop();
    }
  },
);

it('keeps an initially failed runtime context in the floating notice', async () => {
  const f = fixture(false);
  f.contexts.push({
    ...pendingContext(),
    phase: 'failed',
    error: 'authorization_expired',
    revision: 1,
  });
  try {
    await f.controller.refresh();
    expect(await f.controller.ui({ action: 'snapshot' })).toMatchObject({
      ok: true,
      snapshot: { views: [{ context: { phase: 'failed', error: 'authorization_expired' } }] },
    });
  } finally {
    await f.controller.stop();
  }
});

it('keeps runtime interruption in the floating notice after recovery', async () => {
  const f = fixture(false);
  f.contexts.push(pendingContext());
  try {
    await f.controller.refresh();
    await f.controller.unavailable();
    await f.controller.refresh();
    expect(await f.controller.ui({ action: 'snapshot' })).toMatchObject({
      ok: true,
      snapshot: { views: [{ context: { id: 'new-session', phase: 'disconnected' } }] },
    });
    expect(f.ports.authorize).not.toHaveBeenCalled();
  } finally {
    await f.controller.stop();
  }
});
it('access deletion removes only the revoked entry without enabling service or requesting identity', async () => {
  const f = fixture(false);
  f.configuration.entries = [
    {
      entry: { id: 'old', name: 'Old access', client: null, enabled: false, generation: 2 },
      digest: '',
      sealed: '',
    },
    {
      entry: { id: 'other', name: 'Other access', client: null, enabled: true, generation: 1 },
      digest: 'a'.repeat(64),
      sealed: 'protected:other',
    },
  ];
  try {
    const action = { action: 'deleteAccess', id: 'old', generation: 2 } as const;
    const result = await f.controller.ui(action);
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.snapshot?.entries.map((e) => e.id)).toEqual(['other']);
    expect(f.request).toHaveBeenCalledWith({
      operation: 'prepare',
      change: { kind: 'deleteAccess', id: 'old', generation: 2 },
    });
    expect(f.configuration.settings.enabled).toBe(false);
    expect(f.configuration.settings.port).toBe(0);
    const revision = f.configuration.settings.revision;
    expect((await f.controller.ui(action)).ok).toBe(true);
    expect(f.configuration.settings.revision).toBe(revision);
    expect(f.ports.authorize).not.toHaveBeenCalled();
    expect(f.ports.copy).not.toHaveBeenCalled();
  } finally {
    await f.controller.stop();
  }
});
it('access deletion rejects active entries before preparing a mutation', async () => {
  const f = fixture(false);
  f.configuration.entries = [
    {
      entry: { id: 'active', name: 'Active', client: null, enabled: true, generation: 1 },
      digest: 'a'.repeat(64),
      sealed: 'protected:active',
    },
  ];
  try {
    expect(await f.controller.ui({ action: 'deleteAccess', id: 'active', generation: 1 })).toEqual({
      ok: false,
      code: 'access_not_revoked',
    });
    expect(f.configuration.entries).toHaveLength(1);
    expect(f.request.mock.calls.some(([request]) => request.operation === 'prepare')).toBe(false);
  } finally {
    await f.controller.stop();
  }
});
it('access deletion keeps a previous setup request from recreating the deleted credential', async () => {
  const f = fixture(false);
  const setup = {
    action: 'prepareAccess',
    name: 'Old access',
    client: 'generic',
    delivery: 'prompt',
    id: null,
    requestKey: 'original-setup',
  } as const;
  try {
    expect((await f.controller.ui(setup)).ok).toBe(true);
    const secret = f.configuration.entries[0]!;
    secret.entry.enabled = false;
    secret.entry.generation++;
    secret.digest = '';
    secret.sealed = '';
    f.configuration.settings.revision++;
    expect(
      (
        await f.controller.ui({
          action: 'deleteAccess',
          id: secret.entry.id,
          generation: secret.entry.generation,
        })
      ).ok,
    ).toBe(true);
    expect(await f.controller.ui(setup)).toEqual({ ok: false, code: 'access_revoked' });
    expect(f.configuration.entries).toHaveLength(0);
    expect(f.ports.copy).toHaveBeenCalledTimes(1);
    expect(f.ports.authorize).not.toHaveBeenCalled();
  } finally {
    await f.controller.stop();
  }
});
it('keeps native approval for server connection decisions and aborts cancellation', async () => {
  const f = fixture(false);
  const result = await f.controller.ui({ action: 'approveConnection', contextId: 'pending' });
  expect(result).toEqual({ ok: false, code: 'authorization_cancelled' });
  expect(f.request.mock.calls.some(([o]) => o.operation === 'commit')).toBe(false);
  expect(f.request.mock.calls.some(([o]) => o.operation === 'abort')).toBe(true);
  expect(f.ports.copy).not.toHaveBeenCalled();
  await f.controller.stop();
});
it('does not create a default credential when only enabling the service', async () => {
  const f = fixture(true);
  const r = await f.controller.ui({
    action: 'updateSettings',
    patch: { group: 'service', enabled: true, port: 0, baseEnabled: false, basePort: 0 },
  });
  expect(r.ok).toBe(true);
  if (r.ok) expect(r.snapshot?.entries).toHaveLength(0);
  await f.controller.stop();
});
it('keeps secrets out of snapshots and delivers preparation once on retry', async () => {
  const f = fixture(true);
  const action = {
    action: 'prepareAccess',
    client: 'generic',
    name: 'Agent',
    id: null,
    delivery: 'prompt',
    requestKey: 'once',
  } as const;
  const result = await f.controller.ui(action);
  expect(result.ok).toBe(true);
  expect(JSON.stringify(result)).not.toMatch(/protected:|digest/);
  expect(f.ports.authorize).not.toHaveBeenCalled();
  expect(f.ports.copy).toHaveBeenCalledTimes(1);
  await f.controller.ui(action);
  expect(f.ports.copy).toHaveBeenCalledTimes(1);
  expect(f.ports.authorize).not.toHaveBeenCalled();
  await f.controller.stop();
});
it('exports generic HTTP data without an independent backend command', () => {
  const config = JSON.parse(mcpHttpConfiguration('http://127.0.0.1:51602/mcp', 'test-token'));
  expect(config.mcpServers.vigilshell).toEqual({
    type: 'http',
    url: 'http://127.0.0.1:51602/mcp',
    headers: { Authorization: 'Bearer test-token' },
  });
  expect(JSON.stringify(config)).not.toMatch(/command|stdio|vigilshell-backend/);
});
it('copies a complete prompt without native authentication and never returns its token to the renderer', async () => {
  const f = fixture(false);
  const result = await f.controller.ui({
    action: 'prepareAccess',
    client: 'generic',
    name: 'Agent',
    id: null,
    delivery: 'prompt',
    requestKey: 'prompt',
  });
  expect(result.ok).toBe(true);
  expect(f.ports.authorize).not.toHaveBeenCalled();
  const token = f.configuration.entries[0]!.sealed.slice(10);
  const copied = vi.mocked(f.ports.copy).mock.calls[0]![0];
  expect(copied).toContain(token);
  expect(copied).toContain('"vigilshell":');
  expect(copied).not.toContain(f.configuration.entries[0]!.entry.id);
  expect(copied).toContain('Streamable HTTP');
  expect(copied).toContain('Do not run remote server commands');
  expect(copied).toContain('Preserve unrelated configuration');
  expect(JSON.stringify(result)).not.toContain(token);
  expect(f.configuration.policies).toEqual([]);
  await f.controller.stop();
});
it('retries a failed clipboard delivery using the same credential', async () => {
  const f = fixture(false);
  vi.mocked(f.ports.copy).mockImplementationOnce(() => {
    throw new Error('clipboard_unavailable');
  });
  const action = {
    action: 'prepareAccess',
    client: 'generic',
    name: 'Agent',
    id: null,
    delivery: 'prompt',
    requestKey: 'retry',
  } as const;
  expect((await f.controller.ui(action)).ok).toBe(false);
  const id = f.configuration.entries[0]!.entry.id;
  expect((await f.controller.ui(action)).ok).toBe(true);
  expect(f.configuration.entries[0]!.entry.id).toBe(id);
  expect(
    f.request.mock.calls.filter(
      ([r]) => r.operation === 'prepare' && r.change.kind === 'provision',
    ),
  ).toHaveLength(1);
  await f.controller.stop();
});
it('reuses an existing entry and enables a stopped service when copying instructions', async () => {
  const f = fixture(false);
  await f.controller.ui({
    action: 'prepareAccess',
    client: 'generic',
    name: 'Agent',
    id: null,
    delivery: 'prompt',
    requestKey: 'existing',
  });
  const entry = f.configuration.entries[0]!;
  await f.controller.ui({
    action: 'updateSettings',
    patch: {
      group: 'service',
      enabled: false,
      port: f.configuration.settings.port,
      baseEnabled: true,
      basePort: f.configuration.settings.port,
    },
  });
  expect(
    (
      await f.controller.ui({
        action: 'deliverAccess',
        id: entry.entry.id,
        client: 'generic',
        delivery: 'prompt',
      })
    ).ok,
  ).toBe(true);
  expect(f.configuration.entries).toHaveLength(1);
  expect(f.configuration.entries[0]).toEqual(entry);
  expect(f.configuration.settings.enabled).toBe(true);
  expect(f.ports.authorize).not.toHaveBeenCalled();
  await f.controller.stop();
});
it('loads the externalized terminal dependency using native ESM', () => {
  expect(
    execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        "import headless from '@xterm/headless'; const terminal = new headless.Terminal({ cols: 80, rows: 24 }); console.log(terminal.cols); terminal.dispose();",
      ],
      { encoding: 'utf8' },
    ).trim(),
  ).toBe('80');
});

it('does not commit a successful native result after the backend generation changed', async () => {
  const f = fixture(true);
  let approve!: (value: boolean) => void;
  f.ports.authorize = vi.fn(
    () =>
      new Promise<boolean>((resolve) => {
        approve = resolve;
      }),
  );
  const pending = f.controller.ui({ action: 'approveConnection', contextId: 'session' });
  await vi.waitFor(() => expect(f.ports.authorize).toHaveBeenCalled());
  await f.controller.unavailable();
  approve(true);
  expect(await pending).toMatchObject({ ok: false, code: 'authorization_interrupted' });
  expect(f.ports.cancelAuthorization).toHaveBeenCalled();
  expect(f.request.mock.calls.some(([op]) => op.operation === 'commit')).toBe(false);
  await f.controller.stop();
});

it('script approval uses one native identity challenge, cancellation never commits', async () => {
  for (const approved of [false, true]) {
    const f = fixture(approved);
    const result = await f.controller.ui({ action: 'approveScript', taskId: 'script-task' });
    expect(f.ports.authorize).toHaveBeenCalledWith(
      expect.objectContaining({ action: 'approveScript' }),
      { kind: 'approveScript', task_id: 'script-task' },
    );
    expect(f.request.mock.calls.find(([o]) => o.operation === 'prepare')?.[0]).toMatchObject({
      operation: 'prepare',
      change: { kind: 'approveScript', task_id: 'script-task' },
    });
    expect(f.request.mock.calls.some(([o]) => o.operation === 'commit')).toBe(approved);
    expect(result.ok).toBe(approved);
    await f.controller.stop();
  }
});

it('ignores old or duplicate task events so settled work cannot become active again', async () => {
  const f = fixture(true);
  const task = {
    id: 'ordered-task',
    contextId: 'ctx',
    phase: 'waitingReview',
    revision: 1,
  } as import('@contracts').AgentTask;
  f.controller.event({ kind: 'task', task });
  expect(f.controller.activeCount).toBe(1);
  f.controller.event({ kind: 'task', task: { ...task, phase: 'succeeded', revision: 3 } });
  f.controller.event({ kind: 'task', task: { ...task, phase: 'running', revision: 2 } });
  f.controller.event({ kind: 'task', task: { ...task, phase: 'succeeded', revision: 3 } });
  expect(f.controller.activeCount).toBe(0);
  expect(f.ports.notify).toHaveBeenCalledTimes(2);
  await f.controller.stop();
});
it('connection setup distinguishes clipboard failure and retries without provisioning twice', async () => {
  const f = fixture(false);
  vi.mocked(f.ports.copy).mockImplementationOnce(() => {
    throw Error('clipboard failed');
  });
  const action = {
    action: 'prepareAccess',
    name: 'Fixture',
    client: 'generic',
    delivery: 'prompt',
    id: null,
    requestKey: 'copy-retry',
  } as const;
  try {
    expect(await f.controller.ui(action)).toEqual({ ok: false, code: 'access_copy_failed' });
    expect(f.configuration.entries).toHaveLength(1);
    const id = f.configuration.entries[0]!.entry.id;
    expect((await f.controller.ui(action)).ok).toBe(true);
    expect(f.configuration.entries.map((e) => e.entry.id)).toEqual([id]);
    expect(
      f.request.mock.calls.filter(
        ([op]) => op.operation === 'prepare' && op.change.kind === 'provision',
      ),
    ).toHaveLength(1);
    expect(f.ports.copy).toHaveBeenCalledTimes(2);
  } finally {
    await f.controller.stop();
  }
});

it('access management revokes with one mutation and never recreates credentials on retry', async () => {
  const f = fixture(false);
  const setup = {
    action: 'prepareAccess',
    name: 'Generic',
    client: 'generic',
    delivery: 'prompt',
    id: null,
    requestKey: 'revoke-setup',
  } as const;
  try {
    expect((await f.controller.ui(setup)).ok).toBe(true);
    const id = f.configuration.entries[0]!.entry.id;
    f.request.mockClear();
    expect((await f.controller.ui({ action: 'revokeAccess', id })).ok).toBe(true);
    expect(f.request.mock.calls.filter(([r]) => r.operation === 'prepare')).toEqual([
      [{ operation: 'prepare', change: { kind: 'revoke', id } }],
    ]);
    expect(f.configuration.entries).toHaveLength(0);
    expect(await f.controller.ui(setup)).toEqual({ ok: false, code: 'access_revoked' });
    expect(await f.controller.ui({ action: 'rotateAccess', id })).toEqual({
      ok: false,
      code: 'access_revoked',
    });
    expect(f.configuration.entries).toHaveLength(0);
  } finally {
    await f.controller.stop();
  }
});

it('access management renames generic entries only and rotates the existing credential without changing identity', async () => {
  const f = fixture(false);
  f.configuration.entries = [
    {
      entry: { id: 'generic', name: 'Original', client: null, enabled: true, generation: 1 },
      digest: 'a'.repeat(64),
      sealed: 'protected:old',
    },
    {
      entry: { id: 'local', name: 'Cursor', client: 'cursor', enabled: true, generation: 1 },
      digest: 'b'.repeat(64),
      sealed: 'protected:local',
    },
    {
      entry: { id: 'legacy', name: 'Revoked', client: null, enabled: false, generation: 2 },
      digest: '',
      sealed: '',
    },
  ];
  try {
    expect(
      (await f.controller.ui({ action: 'renameAccess', id: 'generic', name: 'Renamed' })).ok,
    ).toBe(true);
    expect(f.configuration.entries[0]!.entry.name).toBe('Renamed');
    expect(f.configuration.entries[0]!.digest).toBe('a'.repeat(64));
    expect(await f.controller.ui({ action: 'renameAccess', id: 'local', name: 'Changed' })).toEqual(
      { ok: false, code: 'invalid_access_entry' },
    );
    expect(await f.controller.ui({ action: 'rotateAccess', id: 'legacy' })).toEqual({
      ok: false,
      code: 'access_revoked',
    });
    expect((await f.controller.ui({ action: 'rotateAccess', id: 'generic' })).ok).toBe(true);
    expect(f.configuration.entries[0]!.entry).toMatchObject({
      id: 'generic',
      name: 'Renamed',
      generation: 2,
    });
    expect(f.configuration.entries[0]!.digest).not.toBe('a'.repeat(64));
    expect(f.configuration.entries).toHaveLength(3);
    expect(f.ports.copy).not.toHaveBeenCalled();
  } finally {
    await f.controller.stop();
  }
});

it('policy settings and closing access do not prompt for identity, enabling does', async () => {
  const f = fixture(true);
  const target = { connectionId: 'one', connectionRevision: 1, expectedRevision: 0 };
  expect(
    (
      await f.controller.ui({
        action: 'updatePolicySettings',
        ...target,
        patch: { field: 'verifyConnect', value: false },
      })
    ).ok,
  ).toBe(true);
  expect((await f.controller.ui({ action: 'setPolicyAccess', ...target, enabled: false })).ok).toBe(
    true,
  );
  expect(f.ports.authorize).not.toHaveBeenCalled();
  expect((await f.controller.ui({ action: 'setPolicyAccess', ...target, enabled: true })).ok).toBe(
    true,
  );
  expect(f.ports.authorize).toHaveBeenCalledTimes(1);
  await f.controller.stop();
});
it('cancelled policy activation never commits the prepared access change', async () => {
  const f = fixture(false);
  expect(
    (
      await f.controller.ui({
        action: 'setPolicyAccess',
        connectionId: 'one',
        connectionRevision: 1,
        expectedRevision: 0,
        enabled: true,
      })
    ).ok,
  ).toBe(false);
  expect(f.request.mock.calls.some(([r]) => r.operation === 'commit')).toBe(false);
  expect(f.request.mock.calls.some(([r]) => r.operation === 'abort')).toBe(true);
  await f.controller.stop();
});

it('activity summaries do not initialize configuration or request authorization', async () => {
  const f = fixture(false);
  f.request.mockResolvedValue({ kind: 'activitySummaries', items: [], more: false });
  expect(await f.controller.ui({ action: 'activitySummaries', filter: {} })).toMatchObject({
    ok: true,
    reply: { kind: 'activitySummaries' },
  });
  expect(f.request).toHaveBeenCalledExactlyOnceWith({ operation: 'activitySummaries', filter: {} });
  expect(f.ports.authorize).not.toHaveBeenCalled();
});

it('historical requests bypass configuration startup even before the MCP snapshot exists', async () => {
  const f = fixture(false);
  f.request.mockResolvedValue({ kind: 'recordings', sessions: [], has_more: false });
  expect(await f.controller.ui({ action: 'recordings', filter: { offset: 0 } })).toMatchObject({
    ok: true,
  });
  expect(f.request).toHaveBeenCalledExactlyOnceWith({
    operation: 'recordings',
    filter: { offset: 0 },
  });
  expect(f.ports.authorize).not.toHaveBeenCalled();
});

it('activity logos are a read-only query independent of MCP initialization', async () => {
  const f = fixture(false);
  f.request.mockResolvedValue({ kind: 'activitySources', sources: [] });
  expect(await f.controller.ui({ action: 'activitySources' })).toMatchObject({ ok: true });
  expect(f.request).toHaveBeenCalledExactlyOnceWith({ operation: 'activitySources' });
  expect(f.ports.authorize).not.toHaveBeenCalled();
});

it('saves activity and tray preferences without identity verification', async () => {
  const f = fixture(false);
  expect(
    (
      await f.controller.ui({
        action: 'updateSettings',
        patch: {
          group: 'activity',
          retentionDays: 45,
          storageMib: 128,
          baseRetentionDays: 30,
          baseStorageMib: 256,
        },
      })
    ).ok,
  ).toBe(true);
  expect(
    (
      await f.controller.ui({
        action: 'updateSettings',
        patch: {
          group: 'tray',
          showTray: false,
          baseShowTray: true,
        },
      })
    ).ok,
  ).toBe(true);
  expect(f.configuration.settings).toMatchObject({
    retentionDays: 45,
    storageMib: 128,
    showTray: false,
  });
  expect(f.ports.authorize).not.toHaveBeenCalled();
  expect(f.request.mock.calls.filter(([r]) => r.operation === 'prepare')).toHaveLength(2);
  expect(f.request.mock.calls.filter(([r]) => r.operation === 'commit')).toHaveLength(2);
  await f.controller.stop();
});
it('preserves settings and reports a failed commit without requesting identity', async () => {
  const f = fixture(false);
  const original = f.request.getMockImplementation()!;
  f.request.mockImplementation(async (op) => {
    if (op.operation === 'commit') throw Error('storage_unavailable');
    return original(op);
  });
  expect(
    await f.controller.ui({
      action: 'updateSettings',
      patch: {
        group: 'tray',
        showTray: false,
        baseShowTray: true,
      },
    }),
  ).toEqual({ ok: false, code: 'storage_unavailable' });
  expect(f.configuration.settings.showTray).toBe(true);
  expect(f.ports.authorize).not.toHaveBeenCalled();
  expect(f.request.mock.calls.some(([r]) => r.operation === 'abort')).toBe(true);
  await f.controller.stop();
});
