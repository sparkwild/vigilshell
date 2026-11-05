import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RuntimeRequest, WorkspaceCommand } from '@contracts';
import { BackendClient } from './client';

const fixtures: {
  client: BackendClient;
  directory: string;
  pids: Set<number>;
}[] = [];

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ESRCH') return false;
    throw error;
  }
}

async function setup(mode: string, timeoutMs = 500) {
  const directory = await mkdtemp(join(tmpdir(), 'vigilshell-backend-client-test-'));
  const source = await readFile(
    fileURLToPath(new URL('../../../tests/fixtures/runtime.mjs', import.meta.url)),
    'utf8',
  );
  const executable = join(directory, 'fake-runtime.mjs');
  await writeFile(executable, `#!${process.execPath}\n${source}`, {
    mode: 0o700,
  });
  await writeFile(join(directory, 'scenario.json'), JSON.stringify({ mode }));
  const client = new BackendClient(executable, directory, timeoutMs);
  const fixture = { client, directory, pids: new Set<number>() };
  fixtures.push(fixture);
  const unavailable = vi.fn();
  client.on('unavailable', unavailable);
  return {
    client,
    unavailable,
    async pid() {
      const pid = Number(await readFile(join(directory, 'pid'), 'utf8'));
      fixture.pids.add(pid);
      return pid;
    },
    async requests(): Promise<
      (RuntimeRequest | (WorkspaceCommand & { id: string; owner: string }))[]
    > {
      return (await readFile(join(directory, 'requests.jsonl'), 'utf8'))
        .trim()
        .split('\n')
        .map(
          (line) =>
            JSON.parse(line) as RuntimeRequest | (WorkspaceCommand & { id: string; owner: string }),
        );
    },
  };
}

afterEach(async () => {
  for (const fixture of fixtures.splice(0)) {
    await fixture.client.stop(50);
    for (const pid of fixture.pids) {
      if (isAlive(pid)) process.kill(pid, 'SIGKILL');
    }
    await rm(fixture.directory, { recursive: true, force: true });
  }
});

describe('BackendClient private process protocol', () => {
  it.each([
    ['malformed-json', /protocol_mismatch/],
    ['invalid-schema', /protocol_mismatch/],
    ['wrong-request', /protocol_mismatch/],
    ['unsupported-version', /protocol_mismatch/],
    ['oversized-frame', /too large/],
    ['oversized-partial', /too large/],
  ])('rejects %s and ends the invalid runtime', async (mode, message) => {
    const { client, unavailable, pid } = await setup(mode as string);
    await expect(client.start()).rejects.toThrow(message);
    const childPid = await pid();
    await expect.poll(() => isAlive(childPid)).toBe(false);
    expect(unavailable).toHaveBeenCalledTimes(1);
    await expect(client.request({ method: 'preferences.get', params: {} })).rejects.toThrow(
      'runtime_unavailable',
    );
  });

  it('buffers a UTF-8 response split inside a multibyte character', async () => {
    const { client, pid, unavailable } = await setup('split-utf8');
    expect(await client.start()).toMatchObject({
      runtimeId: 'runtime-初始',
      kind: 'ready',
      contractVersion: 10,
    });
    await pid();
    expect(await client.request({ method: 'preferences.get', params: {} })).toMatchObject({
      kind: 'preferences',
    });
    expect(unavailable).not.toHaveBeenCalled();
  });

  it('disconnects when a preferences notification uses a different runtime identity', async () => {
    const { client, pid, unavailable } = await setup('changed-instance');
    await client.start();
    const childPid = await pid();
    await expect(
      client.request({
        method: 'preferences.update',
        params: {
          patch: { themeMode: 'dark' },
          base: { themeMode: 'system' },
          expectedRevision: 0,
        },
      }),
    ).rejects.toThrow('protocol_mismatch');
    await expect.poll(() => isAlive(childPid)).toBe(false);
    expect(unavailable).toHaveBeenCalledTimes(1);
  });

  it('a timed-out write leaves the backend alive and can be read without replay', async () => {
    const { client, pid, requests, unavailable } = await setup('timeout-after-commit');
    await client.start();
    const firstPid = await pid();
    await expect(
      client.request({
        method: 'preferences.update',
        params: {
          patch: { themeMode: 'dark' },
          base: { themeMode: 'system' },
          expectedRevision: 0,
        },
      }),
    ).rejects.toThrow('timeout');
    expect(isAlive(firstPid)).toBe(true);
    expect(unavailable).not.toHaveBeenCalled();
    expect(await client.request({ method: 'preferences.get', params: {} })).toMatchObject({
      kind: 'preferences',
      preferences: { themeMode: 'dark', revision: 1 },
    });
    expect((await requests()).filter((r) => r.method === 'preferences.update')).toHaveLength(1);
  });

  it('sends shutdown and waits for a normal child exit', async () => {
    const { client, pid, requests, unavailable } = await setup('normal');
    await client.start();
    const childPid = await pid();
    await client.stop();
    expect(isAlive(childPid)).toBe(false);
    expect((await requests()).map((request) => request.method)).toEqual([
      'runtime.hello',
      'runtime.shutdown',
    ]);
    expect(unavailable).not.toHaveBeenCalled();
  });

  it('terminates a child that ignores the shutdown grace period', async () => {
    const { client, pid, unavailable } = await setup('ignore-shutdown');
    await client.start();
    const childPid = await pid();
    await client.stop(50);
    expect(isAlive(childPid)).toBe(false);
    expect(unavailable).not.toHaveBeenCalled();
  });
});

it('keeps large Agent exchanges bounded without applying their budget to ordinary RPC', async () => {
  const { client, unavailable, pid } = await setup('large-agent');
  await client.start();
  await pid();
  const events = vi.fn();
  client.on('agent', events);
  const call = (length: number) =>
    client.request({
      owner: 'fixture',
      method: 'agent.request',
      params: {
        request: {
          operation: 'invoke',
          digest: 'fixture',
          call: {
            tool: 'terminal_manage',
            action: 'write',
            task_id: 'fixture',
            request_key: 'once',
            text: 'x'.repeat(length),
          },
        },
      },
    });
  const result = await call(70000);
  expect(result).toMatchObject({
    kind: 'workspace',
    result: { kind: 'agent', result: { kind: 'output', output: { incomplete: false } } },
  });
  await expect.poll(() => events.mock.calls.length).toBe(1);
  expect(events.mock.calls[0]![0].context.environment.fixture).toHaveLength(70000);
  await expect(call(1024 * 1024)).rejects.toThrow('invalid_request');
  await expect(
    client.request({
      method: 'preferences.update',
      params: {
        expectedRevision: 0,
        patch: { downloadDirectory: 'x'.repeat(70000) },
        base: { downloadDirectory: '' },
      },
    }),
  ).rejects.toThrow('invalid_request');
  expect(unavailable).not.toHaveBeenCalled();
});

it('uses ordinary RPC deadlines while key availability travels as explicit state', async () => {
  const { client, requests } = await setup('unlock-delayed', 500);
  await client.start();
  const owner = crypto.randomUUID();
  const timers = vi.spyOn(globalThis, 'setTimeout');
  await expect(
    client.workspaceCall(owner, { method: 'workspace.negotiate', params: { version: 16 } }).result,
  ).resolves.toMatchObject({ kind: 'ready', connectionAccess: { phase: 'unlocking' } });
  await expect(client.request({ method: 'preferences.get', params: {} })).resolves.toMatchObject({
    kind: 'preferences',
  });
  const changed = vi.fn();
  client.on('workspace', changed);
  await expect(
    client.workspaceCall(owner, { method: 'connections.unlock', params: {} }).result,
  ).resolves.toMatchObject({ kind: 'connectionAccess', state: { phase: 'unlocking' } });
  await expect.poll(() => changed.mock.calls.length).toBe(1);
  expect(changed.mock.calls[0]![0].params.event).toMatchObject({
    kind: 'connectionAccess',
    state: { phase: 'ready', revision: 2 },
  });
  expect(timers.mock.calls.some(([, ms]) => ms === 120_000)).toBe(false);
  expect((await requests()).filter((r) => r.method === 'connections.unlock')).toHaveLength(1);
  timers.mockRestore();
});
it('retains generic late handshake ownership without treating it as a key unlock', async () => {
  const { client, requests } = await setup('negotiation-delayed');
  await client.start();
  const owner = crypto.randomUUID();
  const recovered = vi.fn();
  client.on('negotiationRecovered', recovered);
  await expect(
    client.workspaceCall(owner, { method: 'workspace.negotiate', params: { version: 16 } }).result,
  ).rejects.toThrow('timeout');
  await expect.poll(() => recovered.mock.calls.length).toBe(1);
  expect(recovered.mock.calls[0]).toEqual([
    client.instanceId,
    owner,
    { phase: 'unlocking', revision: 1, code: null },
  ]);
  expect((await requests()).filter((r) => r.method === 'workspace.negotiate')).toHaveLength(1);
});
