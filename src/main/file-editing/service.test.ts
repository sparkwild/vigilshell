import { mkdtemp, rm, readFile, writeFile, rename, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { afterEach, expect, it, vi } from 'vitest';
import type { FileEditRemote, RemoteFileVersion } from '@contracts';
import { FileEditingService, type EditorPorts } from './service';
const hash = (s: string) => createHash('sha256').update(s).digest('hex');
const cleanups: Array<() => Promise<void>> = [];
afterEach(async () => {
  for (const f of cleanups.splice(0)) await f();
});
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'vigil-editor-'));
  let remote: RemoteFileVersion = {
    hash: hash('original'),
    size: 8,
    modified: 1,
    permissions: 0o100640,
    uid: 1000,
    gid: 1000,
  };
  let local = '';
  let id = '';
  const snapshot = (): FileEditRemote => ({
    id,
    connectionId: 'connection',
    groupId: 'group',
    path: '/fixture/file.txt',
    version: remote,
  });
  const upload = vi.fn(async (file: string, expected: RemoteFileVersion) => {
    if (expected.hash !== remote.hash) throw Error('conflict');
    const content = await readFile(file, 'utf8');
    remote = {
      ...remote,
      hash: hash(content),
      size: Buffer.byteLength(content),
      modified: remote.modified + 1,
    };
    return snapshot();
  });
  const launch = vi.fn().mockResolvedValue(undefined),
    chooseApplication = vi.fn().mockResolvedValue('/Applications/Test.app');
  const begin = vi.fn(async (command: Parameters<EditorPorts['begin']>[0]) => {
    id = command.id;
    local = command.local;
    await writeFile(local, 'original');
    return snapshot();
  });
  const reconcile = vi
    .fn<EditorPorts['reconcile']>()
    .mockResolvedValue({ kind: 'reconciled', result: null, error: null });
  const ports: EditorPorts = {
    runtime: () => 'runtime',
    session: async () => ({ connectionId: 'connection', groupId: 'group' }),
    application: () => '/Applications/Test.app',
    chooseApplication,
    launch,
    begin,
    call: (command) => ({
      id: 'request',
      runtimeId: 'runtime',
      result:
        command.action === 'upload'
          ? upload(command.local, command.expected)
          : Promise.resolve(snapshot()),
    }),
    reconcile,
    chooseExport: async () => null,
  };
  const service = new FileEditingService(root, ports);
  await service.restore();
  cleanups.push(async () => {
    await service.stop();
    await rm(root, { recursive: true, force: true });
  });
  await service.open('session', '/fixture/file.txt', false);
  return {
    service,
    root,
    upload,
    begin,
    launch,
    chooseApplication,
    reconcile,
    ports,
    local: () => local,
    id: () => id,
    remote: () => remote,
  };
}
it('reuses an editing lease for another terminal in the same group before another download', async () => {
  const f = await fixture();
  await f.service.open('second-session', '/fixture/file.txt', false);
  expect(f.begin).toHaveBeenCalledOnce();
  expect(f.launch).toHaveBeenCalledTimes(2);
  expect(f.service.list()).toHaveLength(1);
});
it('observes atomic replacement saves and uploads a stable byte-for-byte snapshot', async () => {
  const f = await fixture();
  await writeFile(f.local() + '.new', 'new\r\ncontent');
  await rename(f.local() + '.new', f.local());
  await vi.waitFor(() => expect(f.upload).toHaveBeenCalledOnce(), { timeout: 4000 });
  await vi.waitFor(() => expect(f.service.list()[0]?.phase).toBe('synced'));
  expect(f.remote().hash).toBe(hash('new\r\ncontent'));
});
it('serializes saves while upload is running and subsequently sends the latest snapshot', async () => {
  const f = await fixture();
  let finish!: () => void;
  const original = f.upload.getMockImplementation()!;
  f.upload.mockImplementationOnce(async (file, expected) => {
    const result = await original(file, expected);
    await new Promise<void>((r) => {
      finish = r;
    });
    return result;
  });
  await writeFile(f.local(), 'first');
  const pending = f.service.retry(f.id());
  await vi.waitFor(() => expect(f.upload).toHaveBeenCalledOnce());
  await writeFile(f.local(), 'second');
  expect(f.upload).toHaveBeenCalledOnce();
  finish();
  await pending;
  await vi.waitFor(() => expect(f.upload).toHaveBeenCalledTimes(2), { timeout: 4000 });
  await vi.waitFor(() => expect(f.remote().hash).toBe(hash('second')));
});
it('never automatically repeats an unknown upload and turns a definite failed receipt into an actionable failure', async () => {
  const f = await fixture();
  f.upload.mockRejectedValueOnce(Error('timeout'));
  await writeFile(f.local(), 'changed');
  await expect(f.service.retry(f.id())).rejects.toThrow('timeout');
  expect(f.service.list()[0]?.phase).toBe('unknown');
  await expect(f.service.retry(f.id())).rejects.toThrow('result_unknown');
  expect(f.upload).toHaveBeenCalledOnce();
  f.reconcile.mockResolvedValue({
    kind: 'reconciled',
    result: null,
    error: 'files_permission_denied',
  });
  await expect(f.service.retry(f.id())).rejects.toThrow('files_permission_denied');
  expect(f.service.list()[0]?.phase).toBe('failed');
  expect(f.upload).toHaveBeenCalledOnce();
  await f.service.retry(f.id());
  expect(f.upload).toHaveBeenCalledTimes(2);
});
it('explicit overwrite submits even when the local copy has not changed', async () => {
  const f = await fixture();
  await f.service.resolve(f.id(), 'overwrite', null);
  expect(f.upload).toHaveBeenCalledOnce();
});
it('retains edits and the upload receipt when disconnect races with completion', async () => {
  const f = await fixture();
  let finish!: () => void;
  const original = f.upload.getMockImplementation()!;
  f.upload.mockImplementationOnce(async (file, expected) => {
    const value = await original(file, expected);
    await new Promise<void>((r) => {
      finish = r;
    });
    return value;
  });
  await writeFile(f.local(), 'dirty');
  const pending = f.service.retry(f.id());
  await vi.waitFor(() => expect(f.upload).toHaveBeenCalledOnce());
  await f.service.disconnect();
  finish();
  await pending;
  expect(f.service.list()[0]?.phase).toBe('disconnected');
  expect(await readFile(f.local(), 'utf8')).toBe('dirty');
  const record = JSON.parse(await readFile(join(f.root, f.id(), 'record.json'), 'utf8'));
  expect(record.phase).toBe('disconnected');
});
it('restores local recovery material without starting a connection or upload', async () => {
  const f = await fixture();
  await writeFile(f.local(), 'unsynced');
  await f.service.stop();
  const restored = new FileEditingService(f.root, f.ports);
  cleanups.unshift(() => restored.stop());
  await restored.restore();
  expect(restored.list()[0]?.phase).toBe('disconnected');
  expect(f.upload).not.toHaveBeenCalled();
  expect(await readdir(join(f.root, f.id()))).toContain('record.json');
  expect(await readFile(f.local(), 'utf8')).toBe('unsynced');
});

it('a temporary application choice does not replace the default on later normal opens', async () => {
  const f = await fixture();
  f.chooseApplication.mockResolvedValueOnce('/Applications/Other.app');
  await f.service.open('session', '/fixture/file.txt', true);
  expect(f.launch).toHaveBeenLastCalledWith('/Applications/Other.app', f.local());
  await f.service.open('second-session', '/fixture/file.txt', false);
  expect(f.launch).toHaveBeenLastCalledWith('/Applications/Test.app', f.local());
  expect(f.begin).toHaveBeenCalledOnce();
  expect(f.chooseApplication).toHaveBeenCalledExactlyOnceWith(false);
});
it('emits distinct failed sync attempts without replaying restored records or planned shutdown', async () => {
  const f = await fixture();
  const notices = vi.fn();
  f.service.on('notification', notices);
  await writeFile(f.local(), 'new content');
  f.upload.mockRejectedValue(Error('conflict'));
  await expect(f.service.retry(f.id())).rejects.toThrow('conflict');
  const first = notices.mock.calls.find(([n]) => n.phase === 'conflict')![0];
  expect(first).toMatchObject({
    id: f.id(),
    phase: 'conflict',
    unsynced: true,
    runtimeId: 'runtime',
  });
  await expect(f.service.retry(f.id())).rejects.toThrow('conflict');
  const conflicts = notices.mock.calls.filter(([n]) => n.phase === 'conflict').map(([n]) => n);
  expect(conflicts).toHaveLength(2);
  expect(conflicts[1].attempt).toBeGreaterThan(conflicts[0].attempt);
  notices.mockClear();
  await f.service.stop();
  expect(notices).not.toHaveBeenCalled();
  const restored = new FileEditingService(f.root, f.ports);
  restored.on('notification', notices);
  await restored.restore();
  expect(notices).not.toHaveBeenCalled();
  await restored.stop();
});
it('distinguishes unsynced disconnection from a clean editing session', async () => {
  const f = await fixture();
  const notices = vi.fn();
  f.service.on('notification', notices);
  await f.service.disconnect();
  expect(notices).toHaveBeenCalledWith(
    expect.objectContaining({ phase: 'disconnected', unsynced: false }),
  );
  const g = await fixture();
  const dirty = vi.fn();
  g.service.on('notification', dirty);
  await writeFile(g.local(), 'unsynced');
  await g.service.disconnect();
  expect(dirty).toHaveBeenCalledWith(
    expect.objectContaining({ phase: 'disconnected', unsynced: true, runtimeId: 'runtime' }),
  );
});
it('does not allocate a new notification attempt for an unchanged working copy', async () => {
  const f = await fixture();
  const notices = vi.fn();
  f.service.on('notification', notices);
  await f.service.retry(f.id());
  await f.service.retry(f.id());
  expect(notices.mock.calls.filter(([notice]) => notice.phase === 'failed')).toHaveLength(0);
  await writeFile(f.local(), 'changed');
  f.upload.mockRejectedValue(Error('conflict'));
  await expect(f.service.retry(f.id())).rejects.toThrow('conflict');
  expect(notices.mock.calls.find(([notice]) => notice.phase === 'conflict')?.[0].attempt).toBe(1);
});
