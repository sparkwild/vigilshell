import { afterEach, expect, it, vi } from 'vitest';
import type { WorkspaceOperation, WorkspaceReply } from '@contracts';
import { OperationStore } from './operations';
import { WorkspaceFailure } from './workspace-failure';
const job = (
  id = 'backend-a',
  phase: WorkspaceOperation['phase'] = 'working',
  kind = 'connect',
): WorkspaceOperation => ({
  id,
  phase,
  kind,
  stage: kind,
  error: null,
  draft: null,
  proxy: null,
  connection: null,
  challenge: null,
  sessionId: null,
});
const reply = (operation = job()): WorkspaceReply => ({
  kind: 'operation',
  operation,
});
const done: WorkspaceReply = { kind: 'done' };
const request = (owner: string, execute: () => Promise<WorkspaceReply>) => ({
  owner,
  action: 'ws_connect' as const,
  label: owner,
  execute,
});
function setup() {
  const get = vi.fn(async () => reply(job('backend-a', 'success'))),
    cancel = vi.fn(async () => done),
    reconcile = vi.fn(async (): Promise<WorkspaceReply> => ({
      kind: 'reconciled',
      result: done,
      error: null,
    }));
  let runtime = 'runtime';
  const store = new OperationStore({
    get,
    cancel,
    reconcile,
    runtimeId: () => runtime,
  });
  return {
    store,
    get,
    cancel,
    reconcile,
    restart: () => {
      runtime = 'new-runtime';
    },
  };
}
afterEach(() => vi.useRealTimers());
it('executes immediately but suppresses fast feedback and removes it immediately on completion', async () => {
  vi.useFakeTimers();
  const { store } = setup();
  const execute = vi.fn(async () => done);
  const work = store.run(request('a', execute));
  expect(execute).toHaveBeenCalledTimes(1);
  expect(store.forOwner('a')?.visible).toBe(false);
  await work;
  await vi.advanceTimersByTimeAsync(500);
  expect(store.getSnapshot().operations).toEqual([]);
});
it('starts the 300ms threshold before the first backend response', async () => {
  vi.useFakeTimers();
  const { store } = setup();
  let finish!: (r: WorkspaceReply) => void;
  const work = store.run(
    request(
      'a',
      () =>
        new Promise((r) => {
          finish = r;
        }),
    ),
  );
  await vi.advanceTimersByTimeAsync(299);
  expect(store.forOwner('a')?.visible).toBe(false);
  await vi.advanceTimersByTimeAsync(1);
  expect(store.forOwner('a')?.visible).toBe(true);
  finish(done);
  await work;
  expect(store.isBusy('a')).toBe(false);
});
it('locks shared resources before execute while unrelated targets remain concurrent', async () => {
  const { store } = setup();
  let finish!: (r: WorkspaceReply) => void;
  const work = store.run({
    ...request(
      'editor:a',
      () =>
        new Promise((r) => {
          finish = r;
        }),
    ),
    resources: ['connection:a'],
  });
  const execute = vi.fn(async () => done);
  await expect(store.run(request('connection:a', execute))).rejects.toMatchObject({ code: 'busy' });
  expect(execute).not.toHaveBeenCalled();
  await store.run(request('connection:b', async () => done));
  expect(store.isBusy('connection:a')).toBe(true);
  finish(done);
  await work;
});
it('keeps failures belonging to other owners or actions until dismissed or resolved', async () => {
  const { store } = setup();
  await expect(
    store.run(
      request('a', async () => {
        throw new WorkspaceFailure('connection_refused');
      }),
    ),
  ).rejects.toThrow();
  await store.run(request('b', async () => done));
  await store.run({ ...request('a', async () => done), action: 'ws_test' });
  expect(store.forOwner('a')?.error?.code).toBe('connection_refused');
  await store.run(request('a', async () => done));
  expect(store.getSnapshot().operations).toEqual([]);
});
it('waits for confirmed cancellation and addresses only the actual operation id', async () => {
  vi.useFakeTimers();
  const { store, get, cancel } = setup();
  get.mockResolvedValue(reply(job()));
  const work = store.run(request('a', async () => reply()));
  const caught = work.catch((e) => e.code);
  await vi.advanceTimersByTimeAsync(300);
  await store.cancel(store.forOwner('a')!.id);
  expect(cancel).toHaveBeenCalledWith('backend-a');
  expect(store.forOwner('a')?.phase).toBe('cancelling');
  get.mockResolvedValue(reply({ ...job('backend-a', 'cancelled'), error: 'cancelled' }));
  await vi.advanceTimersByTimeAsync(120);
  expect(await caught).toBe('cancelled');
  expect(store.getSnapshot().operations).toEqual([]);
});
it('a too-late cancellation keeps the operation running with its own warning', async () => {
  vi.useFakeTimers();
  const { store, get, cancel } = setup();
  get.mockResolvedValue(reply(job()));
  cancel.mockRejectedValue(new WorkspaceFailure('too_late'));
  const work = store.run(request('a', async () => reply()));
  await vi.advanceTimersByTimeAsync(300);
  await store.cancel(store.forOwner('a')!.id);
  expect(store.forOwner('a')).toMatchObject({
    phase: 'running',
    cancelError: { code: 'too_late' },
  });
  get.mockResolvedValue(reply(job('backend-a', 'success')));
  await vi.advanceTimersByTimeAsync(120);
  await work;
});
it('does not offer or send cancellation for non-cancellable saves', async () => {
  vi.useFakeTimers();
  const { store, get, cancel } = setup();
  get.mockResolvedValue(reply(job('save', 'working', 'save')));
  const work = store.run(request('a', async () => reply(job('save', 'working', 'save'))));
  await vi.advanceTimersByTimeAsync(300);
  await store.cancel(store.forOwner('a')!.id);
  expect(cancel).not.toHaveBeenCalled();
  get.mockResolvedValue(reply(job('save', 'success', 'save')));
  await vi.advanceTimersByTimeAsync(120);
  await work;
});
it('reconciles an initial unknown write without replaying it, retaining the lock on query failure', async () => {
  const { store, reconcile } = setup();
  const execute = vi.fn(async () => {
    throw new WorkspaceFailure('result_unknown', 'original-request', 'runtime');
  });
  const onResult = vi.fn();
  await expect(store.run({ ...request('a', execute), onResult })).rejects.toThrow();
  const id = store.forOwner('a')!.id;
  reconcile.mockRejectedValueOnce(new WorkspaceFailure('timeout'));
  await expect(store.reconcile(id)).rejects.toMatchObject({
    code: 'result_unknown',
  });
  expect(store.isBusy('a')).toBe(true);
  await store.reconcile(id);
  expect(reconcile).toHaveBeenLastCalledWith('original-request', 'runtime');
  expect(execute).toHaveBeenCalledTimes(1);
  expect(onResult).toHaveBeenCalledTimes(1);
  expect(store.isBusy('a')).toBe(false);
});
it('preserves a known backend job on a polling failure and never queries it in a new runtime', async () => {
  vi.useFakeTimers();
  const { store, get, restart } = setup();
  get.mockRejectedValue(new WorkspaceFailure('runtime_unavailable'));
  const execute = vi.fn(async () => reply());
  const caught = store.run(request('a', execute)).catch((e) => e.code);
  await vi.advanceTimersByTimeAsync(120);
  expect(await caught).toBe('result_unknown');
  expect(store.forOwner('a')?.job?.id).toBe('backend-a');
  restart();
  get.mockClear();
  await expect(store.reconcile(store.forOwner('a')!.id)).rejects.toMatchObject({
    code: 'result_unknown',
  });
  expect(get).not.toHaveBeenCalled();
  expect(store.isBusy('a')).toBe(true);
});
it('rejects a mismatched operation returned by reconciliation', async () => {
  vi.useFakeTimers();
  const { store, get } = setup();
  get.mockRejectedValueOnce(new WorkspaceFailure('timeout'));
  const caught = store.run(request('a', async () => reply())).catch(() => {});
  await vi.advanceTimersByTimeAsync(120);
  await caught;
  get.mockResolvedValue(reply(job('other-operation', 'success')));
  await expect(store.reconcile(store.forOwner('a')!.id)).rejects.toMatchObject({
    code: 'result_unknown',
  });
  expect(store.isBusy('a')).toBe(true);
});
it('does not report native cancellation as failure and tracks multiple visible instances', async () => {
  const { store } = setup();
  await expect(
    store.run(
      request('a', async () => {
        throw new WorkspaceFailure('cancelled');
      }),
    ),
  ).rejects.toThrow();
  expect(store.getSnapshot().operations).toEqual([]);
  store.setAnchor('fixed', 'a', true);
  store.setAnchor('preview', 'a', true);
  store.setAnchor('fixed', 'a', false);
  expect(store.getSnapshot().anchors.has('a')).toBe(true);
  store.setAnchor('preview', 'a', false);
  expect(store.getSnapshot().anchors.has('a')).toBe(false);
});

it('does not replay a failure after its local feedback leaves the screen', async () => {
  const { store } = setup();
  store.setAnchor('dialog', 'editor:a', true);
  await expect(
    store.run(
      request('editor:a', async () => {
        throw new WorkspaceFailure('authentication_failed');
      }),
    ),
  ).rejects.toThrow();
  expect(store.forOwner('editor:a')?.presentedLocally).toBe(true);
  store.setAnchor('dialog', 'editor:a', false);
  expect(store.forOwner('editor:a')?.presentedLocally).toBe(true);
  await expect(
    store.run(
      request('editor:a', async () => {
        throw new WorkspaceFailure('connection_refused');
      }),
    ),
  ).rejects.toThrow();
  expect(store.forOwner('editor:a')?.presentedLocally).not.toBe(true);
});

it('recognizes a session failure page even when the operation belongs to the connection', async () => {
  const { store } = setup();
  await expect(
    store.run(
      request('connection:a', async () =>
        reply({
          ...job('connect', 'failed'),
          sessionId: 'session-a',
          error: 'authentication_failed',
        }),
      ),
    ),
  ).rejects.toThrow();
  expect(store.forOwner('connection:a')?.presentedLocally).not.toBe(true);
  store.setAnchor('failure-page', 'session:session-a', true);
  expect(store.forOwner('connection:a')?.presentedLocally).toBe(true);
  store.setAnchor('failure-page', 'session:session-a', false);
  expect(store.forOwner('connection:a')?.presentedLocally).toBe(true);
});

it('preserves running and unknown operations when an editor releases completed failures', async () => {
  const { store } = setup();
  store.report(
    { owner: 'editor:a', action: 'ws_test', label: 'fixture' },
    new WorkspaceFailure('authentication_failed'),
  );
  store.report(
    { owner: 'editor:a', action: 'ws_save', label: 'fixture' },
    new WorkspaceFailure('result_unknown'),
  );
  store.report(
    { owner: 'editor:b', action: 'ws_test', label: 'other' },
    new WorkspaceFailure('connection_refused'),
  );
  store.dismissOwnerFailures('editor:a');
  expect(store.getSnapshot().operations.map((op) => [op.owner, op.phase])).toEqual([
    ['editor:a', 'unknown'],
    ['editor:b', 'failed'],
  ]);
  expect(store.isBusy('editor:a')).toBe(true);
  let finish!: (value: WorkspaceReply) => void;
  const work = store.run(
    request(
      'editor:c',
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    ),
  );
  store.dismissOwnerFailures('editor:c');
  expect(store.isBusy('editor:c')).toBe(true);
  finish(done);
  await work;
});
