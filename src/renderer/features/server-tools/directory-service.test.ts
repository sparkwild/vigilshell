// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import type { WorkspaceStore } from '../connections/workspace-store';
import type { TerminalSessionView } from '@contracts';
import { DirectoryService } from './directory-service';
const remembered = vi.hoisted(() => ({
  load: vi.fn(async () => {}),
  data: {
    kind: 'directory',
    connectionRevision: 1,
    path: '/last',
    scroll: 50,
    expanded: ['/'],
  },
}));
vi.mock('../../shared/platform/ui-state-store', () => ({
  loadUiState: remembered.load,
  readUiState: () => ({ data: remembered.data }),
  updateUiState: vi.fn(async () => {}),
}));
const session = {
  id: 'session',
  groupId: 'group',
  connectionId: 'connection',
  phase: 'running',
} as TerminalSessionView;
function fixture(call: ReturnType<typeof vi.fn>) {
  return new DirectoryService({
    call,
    getSnapshot: () => ({ runtimeId: 'runtime', sessions: [session] }),
  } as unknown as WorkspaceStore);
}
const wait = () => new Promise((resolve) => setTimeout(resolve, 0));
it('deduplicates reads without cancelling another subscriber', async () => {
  let reply!: (v: unknown) => void;
  const call = vi.fn((method: string) =>
    method === 'listFiles'
      ? new Promise((resolve) => {
          reply = resolve;
        })
      : Promise.resolve({ kind: 'done' }),
  );
  const service = fixture(call),
    a = new AbortController(),
    b = new AbortController();
  const first = service.read(session, '/', a.signal).catch((e) => e.name),
    second = service.read(session, '/', b.signal);
  a.abort();
  expect(await first).toBe('AbortError');
  expect(call.mock.calls.filter(([name]) => name === 'cancelFileList')).toHaveLength(0);
  reply({ kind: 'directory', page: { path: '/', entries: [], next: null } });
  expect(await second).toEqual({ path: '/', entries: [] });
  await wait();
  expect(call.mock.calls.filter(([name]) => name === 'listFiles')).toHaveLength(1);
  expect(call).toHaveBeenCalledWith('releaseFileList', expect.anything());
});
it('collects every page before publishing a directory', async () => {
  const call = vi.fn(async (method: string, p: { cursor?: string }) =>
    method === 'listFiles'
      ? {
          kind: 'directory',
          page: {
            path: '/',
            entries: [{ name: p.cursor ? 'second' : 'first' }],
            next: p.cursor ? null : '1',
          },
        }
      : { kind: 'done' },
  );
  const result = await fixture(call).read(session, '/', new AbortController().signal);
  expect(result.entries.map((e) => e.name)).toEqual(['first', 'second']);
});
it('bounds concurrency and prioritizes queued navigation', async () => {
  const pending: Array<() => void> = [];
  const call = vi.fn((method: string, p: { path: string }) =>
    method === 'listFiles'
      ? new Promise((resolve) =>
          pending.push(() =>
            resolve({
              kind: 'directory',
              page: { path: p.path, entries: [], next: null },
            }),
          ),
        )
      : Promise.resolve({ kind: 'done' }),
  );
  const service = fixture(call);
  const jobs = Array.from({ length: 5 }, (_, n) =>
    service.read(session, `/${n}`, new AbortController().signal, 1),
  );
  const table = service.read(session, '/table', new AbortController().signal, 0);
  expect(pending).toHaveLength(4);
  pending[0]!();
  await wait();
  expect(call.mock.calls.filter(([m]) => m === 'listFiles').at(-1)?.[1].path).toBe('/table');
  for (let n = 1; n < 6; n++) {
    pending[n]?.();
    await wait();
  }
  await Promise.all([...jobs, table]);
});

it('opens the initial folder by default even when a last folder was saved', async () => {
  const service = fixture(vi.fn());
  remembered.load.mockClear();
  await service.restore({ ...session, connectionRevision: 1 });
  expect(service.preference(session.connectionId).path).toBe('.');
  expect(remembered.load).not.toHaveBeenCalled();
});

it('applies the opening policy only on a new runtime group and checks configuration revision', async () => {
  const service = fixture(vi.fn());
  const first = { ...session, groupId: 'first', connectionRevision: 1 };
  await service.restore(first, 'last');
  expect(service.preference(first.connectionId).path).toBe('/last');
  service.preference(first.connectionId).path = '/current';
  await service.restore(first, 'initial');
  expect(service.preference(first.connectionId).path).toBe('/current');
  await service.restore({ ...first, groupId: 'second' }, 'initial');
  expect(service.preference(first.connectionId).path).toBe('.');
  await service.restore({ ...first, groupId: 'third' }, 'last');
  expect(service.preference(first.connectionId).path).toBe('/last');
  await service.restore({ ...first, groupId: 'fourth', connectionRevision: 2 }, 'last');
  expect(service.preference(first.connectionId).path).toBe('.');
});

it('does not let a delayed old-group restore overwrite a newly selected initial folder', async () => {
  let complete!: () => void;
  remembered.load.mockImplementationOnce(
    () =>
      new Promise<void>((resolve) => {
        complete = resolve;
      }),
  );
  const service = fixture(vi.fn());
  const first = service.restore({ ...session, groupId: 'old', connectionRevision: 1 }, 'last');
  await service.restore({ ...session, groupId: 'new', connectionRevision: 1 }, 'initial');
  complete();
  await first;
  expect(service.preference(session.connectionId).path).toBe('.');
});
