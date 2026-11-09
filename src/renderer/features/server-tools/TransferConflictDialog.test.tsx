// @vitest-environment jsdom
import { render, screen, cleanup, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import userEvent from '@testing-library/user-event';
import { TransferConflictDialog } from './TransferConflictDialog';
import { translate } from '../../../shared/i18n/messages';
const fixture = vi.hoisted(() => ({
  tasks: new Map(),
  target: { runtimeId: 'runtime', connectionId: 'connection', batchId: 'batch', taskId: 'a' },
  runtime: 'runtime',
  liveRuntime: 'runtime',
  call: vi.fn(async () => ({ kind: 'done' })),
  close: vi.fn(),
  refresh: vi.fn(async () => {}),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: {
      transportReady: true,
      ready: true,
      runtimeId: fixture.runtime,
      transfers: fixture.tasks,
      transferConflictTarget: fixture.target,
    },
    store: {
      getSnapshot: () => ({
        transportReady: true,
        ready: true,
        runtimeId: fixture.liveRuntime,
        transfers: fixture.tasks,
      }),
      feedback: { setAnchor: vi.fn() },
      call: fixture.call,
      refreshTransfers: fixture.refresh,
      closeTransferConflicts: fixture.close,
    },
  }),
  errorCode: (e: Error) => e.message,
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Record<string, string | number>) =>
      translate(key, 'zh-CN', values),
  }),
}));
const task = (id: string, batchId = 'batch', phase = 'awaitingDecision') => ({
  id,
  batchId,
  batchSize: 2,
  connectionId: 'connection',
  phase,
  createdAt: id === 'a' ? 1 : 2,
  name: id + '-directory',
  remoteIdentity: 'fixture',
  conflictVersion: 3,
  conflict: { sourcePath: `/source/${id}/nested.txt`, targetPath: `/target/${id}/nested.txt` },
});
beforeEach(() => {
  fixture.tasks = new Map([
    ['a', task('a')],
    ['b', task('b')],
    ['other', task('other', 'other')],
  ]);
  fixture.runtime = fixture.liveRuntime = 'runtime';
  vi.clearAllMocks();
  fixture.call.mockResolvedValue({ kind: 'done' });
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })) as unknown as typeof window.matchMedia;
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
afterEach(cleanup);
it('shows only the chosen batch, precise child paths, and single-file decisions by default', async () => {
  render(<TransferConflictDialog />);
  expect(screen.getByText('当前有 2 项任务等待处理')).toBeInTheDocument();
  expect(
    screen.queryByText('仅处理本次选择的传输批次。关闭窗口不会取消任务。'),
  ).not.toBeInTheDocument();
  expect(screen.getAllByRole('button', { name: '关闭' })).toHaveLength(1);
  expect(screen.queryByText('other-directory')).not.toBeInTheDocument();
  expect(screen.getByText('/source/a/nested.txt')).toBeInTheDocument();
  expect(screen.getByText('/target/a/nested.txt')).toBeInTheDocument();
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  await userEvent.click(screen.getByRole('button', { name: '覆盖' }));
  expect(fixture.call).toHaveBeenCalledExactlyOnceWith('resolveTransferConflict', {
    id: 'a',
    version: 3,
    decision: 'overwrite',
    batch: false,
  });
});
it('sends current-and-future batch scope only for overwrite or skip', async () => {
  render(<TransferConflictDialog />);
  await userEvent.click(screen.getByText('应用到本批次所有同名冲突'));
  expect(screen.getByRole('checkbox')).toBeChecked();
  await userEvent.click(screen.getByRole('button', { name: '跳过' }));
  expect(fixture.call).toHaveBeenCalledExactlyOnceWith('resolveTransferConflict', {
    id: 'a',
    version: 3,
    decision: 'skip',
    batch: true,
  });
});
it('cancel stops this batch regardless of the checkbox', async () => {
  render(<TransferConflictDialog />);
  await userEvent.click(screen.getByRole('checkbox'));
  await userEvent.click(screen.getByRole('button', { name: '取消' }));
  expect(fixture.call.mock.calls).toEqual([
    ['cancelTransfer', { id: 'a' }],
    ['cancelTransfer', { id: 'b' }],
  ]);
});
it('closing and escape never send a transfer mutation', async () => {
  render(<TransferConflictDialog />);
  await userEvent.click(screen.getAllByRole('button', { name: '关闭' })[0]!);
  expect(fixture.close).toHaveBeenCalled();
  await userEvent.keyboard('{Escape}');
  expect(fixture.call).not.toHaveBeenCalled();
});
it('advances to another pending task and does not replay a decision when conflict version changes', async () => {
  const view = render(<TransferConflictDialog />);
  await userEvent.click(screen.getByRole('checkbox'));
  fixture.tasks.set('a', { ...task('a'), conflictVersion: 4 });
  view.rerender(<TransferConflictDialog />);
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(fixture.call).not.toHaveBeenCalled();
  fixture.tasks.set('a', task('a', 'batch', 'running'));
  view.rerender(<TransferConflictDialog />);
  expect(screen.getByText('当前有 1 项任务等待处理')).toBeInTheDocument();
  expect(screen.getByText('/target/b/nested.txt')).toBeInTheDocument();
});
it('rejects a stale conflict version immediately before dispatch', async () => {
  render(<TransferConflictDialog />);
  fixture.tasks.set('a', { ...task('a'), conflictVersion: 4 });
  await userEvent.click(screen.getByRole('button', { name: '覆盖' }));
  expect(fixture.call).not.toHaveBeenCalled();
  expect(screen.getByText('此冲突已变化，请核对当前任务状态后再操作。')).toBeInTheDocument();
});
it('rejects a retired runtime and does not show an old batch after replacement', async () => {
  const view = render(<TransferConflictDialog />);
  fixture.liveRuntime = 'new-runtime';
  await userEvent.click(screen.getByRole('button', { name: '覆盖' }));
  expect(fixture.call).not.toHaveBeenCalled();
  fixture.runtime = 'new-runtime';
  view.rerender(<TransferConflictDialog />);
  expect(screen.getByText('该批传输已不可用')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '覆盖' })).not.toBeInTheDocument();
});
it('cancel only stops unfinished tasks in this batch', async () => {
  fixture.tasks.set('finished', task('finished', 'batch', 'complete'));
  fixture.tasks.set('running', task('running', 'batch', 'running'));
  render(<TransferConflictDialog />);
  await userEvent.click(screen.getByRole('button', { name: '取消' }));
  expect(fixture.call.mock.calls).toEqual([
    ['cancelTransfer', { id: 'a' }],
    ['cancelTransfer', { id: 'b' }],
    ['cancelTransfer', { id: 'running' }],
  ]);
});
it('shows mutation errors locally without automatic retry', async () => {
  fixture.call.mockRejectedValueOnce(new Error('files_permission_denied'));
  render(<TransferConflictDialog />);
  await userEvent.click(screen.getByRole('button', { name: '覆盖' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('没有访问此文件或目录的权限');
  expect(fixture.call).toHaveBeenCalledTimes(1);
});
it('closes when no conflicts remain without cancelling active tasks', () => {
  for (const id of ['a', 'b']) fixture.tasks.set(id, task(id, 'batch', 'running'));
  render(<TransferConflictDialog />);
  expect(screen.queryByText(/当前没有等待处理的冲突/)).not.toBeInTheDocument();
  expect(fixture.close).toHaveBeenCalled();
  expect(fixture.call).not.toHaveBeenCalled();
});

it('keeps batch cancellation busy and its error visible when the first task leaves the waiting list', async () => {
  let fail!: (error: Error) => void;
  fixture.call.mockImplementationOnce(
    () =>
      new Promise((_resolve, reject) => {
        fail = reject;
      }),
  );
  const view = render(<TransferConflictDialog />);
  await userEvent.click(screen.getByRole('button', { name: '取消' }));
  fixture.tasks.set('a', task('a', 'batch', 'cancelled'));
  view.rerender(<TransferConflictDialog />);
  expect(screen.getByRole('button', { name: '覆盖' })).toBeDisabled();
  fail(new Error('files_permission_denied'));
  expect(await screen.findByRole('alert')).toHaveTextContent('没有访问此文件或目录的权限');
  expect(fixture.call).toHaveBeenCalledTimes(1);
});
it('does not offer overwrite or skip without exact conflict paths', () => {
  fixture.tasks.set('a', { ...task('a'), conflict: null });
  render(<TransferConflictDialog />);
  expect(screen.getByRole('button', { name: '覆盖' })).toBeDisabled();
  expect(screen.getByRole('button', { name: '跳过' })).toBeDisabled();
  expect(screen.getByRole('button', { name: '取消' })).toBeEnabled();
});

it.each(['success', 'failure'])(
  'waits for the operation receipt before closing: %s',
  async (outcome) => {
    fixture.tasks.delete('b');
    let resolve!: (value: { kind: string }) => void;
    let reject!: (error: Error) => void;
    fixture.call.mockImplementationOnce(
      () =>
        new Promise<{ kind: string }>((yes, no) => {
          resolve = yes;
          reject = no;
        }),
    );
    const view = render(<TransferConflictDialog />);
    await userEvent.click(screen.getByRole('button', { name: '跳过' }));
    fixture.tasks.set('a', task('a', 'batch', 'running'));
    view.rerender(<TransferConflictDialog />);
    expect(fixture.close).not.toHaveBeenCalled();
    expect(screen.getByRole('status', { name: '正在处理' })).toBeInTheDocument();
    if (outcome === 'success') {
      resolve({ kind: 'done' });
      await waitFor(() => expect(fixture.close).toHaveBeenCalled());
    } else {
      reject(new Error('files_permission_denied'));
      expect(await screen.findByRole('alert')).toHaveTextContent('没有访问此文件或目录的权限');
      expect(fixture.close).not.toHaveBeenCalled();
    }
  },
);

it.each([
  [1, 1, false],
  [1, 4, true],
  [2, 1, true],
])(
  'only offers batch scope for multiple tasks or directory items: batch=%s, items=%s',
  async (batchSize, pending, visible) => {
    fixture.tasks.delete('b');
    fixture.tasks.set('a', {
      ...task('a'),
      batchSize,
      details: { stage: 'transferring', completed: 0, failed: 0, skipped: 0, pending },
    });
    render(<TransferConflictDialog />);
    if (visible) {
      expect(screen.getByRole('checkbox', { name: '应用到本批次所有同名冲突' })).not.toBeChecked();
    } else {
      expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
      await userEvent.click(screen.getByRole('button', { name: '覆盖' }));
      expect(fixture.call).toHaveBeenCalledExactlyOnceWith('resolveTransferConflict', {
        id: 'a',
        version: 3,
        decision: 'overwrite',
        batch: false,
      });
    }
  },
);
