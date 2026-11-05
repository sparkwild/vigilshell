// @vitest-environment jsdom
import { render, screen, cleanup, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import userEvent from '@testing-library/user-event';
import { TransferList } from './TransferPanel';
import { translate } from '../../../shared/i18n/messages';
const fixture = vi.hoisted(() => ({
  transfers: new Map(),
  samples: new Map(),
  showConflicts: vi.fn(),
  refresh: vi.fn(async () => {}),
  call: vi.fn(async () => ({ kind: 'done' })),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: {
      transportReady: true,
      ready: true,
      runtimeId: 'runtime',
      transfers: fixture.transfers,
    },
    store: {
      feedback: { setAnchor: vi.fn() },
      transferSamples: fixture.samples,
      refreshTransfers: fixture.refresh,
      showTransferConflicts: fixture.showConflicts,
      connectionLabel: (id: string) => `Connection ${id}`,
      call: fixture.call,
      report: vi.fn(),
    },
  }),
  errorCode: () => 'tools_unavailable',
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Record<string, string | number>) =>
      translate(key, 'zh-CN', values),
  }),
}));
beforeEach(() => {
  fixture.call.mockClear();
  fixture.showConflicts.mockClear();
  fixture.samples.clear();
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
afterEach(() => {
  cleanup();
  fixture.transfers.clear();
});
it('shows only the requested connection and exposes a conflict as pending work', () => {
  for (const id of ['a', 'b'])
    fixture.transfers.set(id, {
      id,
      connectionId: id,
      groupId: id,
      name: `${id}.txt`,
      phase: 'awaitingDecision',
      direction: 'upload',
      bytes: 0,
      total: null,
      remotePath: `/${id}.txt`,
      remoteIdentity: `user@${id}`,
      conflictVersion: 1,
    });
  render(<TransferList connectionId="a" />);
  expect(screen.getByText('a.txt')).toBeInTheDocument();
  expect(screen.queryByText('b.txt')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: '处理冲突' })).toBeEnabled();
  expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '覆盖' })).not.toBeInTheDocument();
  expect(screen.getByRole('progressbar')).not.toHaveAttribute('value');
});
it('does not offer a global task list when no connection is supplied', () => {
  fixture.transfers.set('a', { connectionId: 'a', phase: 'running' });
  render(<TransferList />);
  expect(screen.getByText('暂无传输任务')).toBeInTheDocument();
});
it('explains transfer failures without exposing the internal error code', () => {
  fixture.transfers.set('a', {
    id: 'a',
    connectionId: 'a',
    name: 'a.txt',
    phase: 'failed',
    direction: 'upload',
    bytes: 0,
    total: null,
    remotePath: '/a.txt',
    remoteIdentity: 'user@a',
    error: 'files_permission_denied',
  });
  const { container } = render(<TransferList connectionId="a" />);
  expect(container).toHaveTextContent('没有访问此文件或目录的权限');
  expect(container).not.toHaveTextContent('files_permission_denied');
});

it('filters a notification batch and focuses its first failure', () => {
  for (const id of ['a', 'b'])
    fixture.transfers.set(id, {
      id,
      connectionId: 'connection',
      batchId: id,
      name: id + '.txt',
      phase: 'failed',
      direction: 'upload',
      bytes: 0,
      total: 0,
      remotePath: '/' + id,
      remoteIdentity: 'fixture',
      attempt: 0,
      conflictVersion: 0,
      canRetry: true,
    });
  render(
    <TransferList
      connectionId="connection"
      target={{ runtimeId: 'runtime', connectionId: 'connection', batchId: 'a', phase: 'failed' }}
    />,
  );
  expect(screen.getByText('a.txt')).toBeInTheDocument();
  expect(screen.queryByText('b.txt')).not.toBeInTheDocument();
  expect(document.activeElement?.id).toBe('transfer-task-a');
});
it('renders an unavailable target after runtime replacement', () => {
  render(
    <TransferList
      connectionId="connection"
      target={{ runtimeId: 'old', connectionId: 'connection', batchId: 'a', phase: 'failed' }}
    />,
  );
  expect(screen.getByText('该批传输已不可用')).toBeInTheDocument();
});

function task(phase = 'running') {
  return {
    id: 'a',
    createdAt: 1000,
    connectionId: 'a',
    groupId: 'a',
    name: 'access.log',
    phase,
    direction: 'download',
    bytes: 25,
    total: 100,
    remotePath: '/logs/access.log',
    remoteIdentity: 'user@fixture',
    attempt: 1,
    revision: 1,
    conflictVersion: 0,
    canRetry: true,
    details: { stage: 'transferring', completed: 0, failed: 0, skipped: 0, pending: 1 },
  };
}
it('keeps details open through completion without exposing paths in the compact row', async () => {
  fixture.transfers.set('a', task());
  const view = render(<TransferList connectionId="a" />);
  expect(screen.queryByText('/logs/access.log')).not.toBeInTheDocument();
  expect(screen.queryByText(/完成 0/)).not.toBeInTheDocument();
  expect(screen.getByText('25%')).toBeVisible();
  await userEvent.hover(document.getElementById('transfer-task-a')!);
  await screen.findByRole('region', { name: '详情' });
  const dialog = screen.getByRole('region', { name: '详情' });
  expect(within(dialog).getByText('/logs/access.log')).toBeVisible();
  expect(fixture.call).not.toHaveBeenCalled();
  fixture.transfers.set('a', {
    ...task('complete'),
    bytes: 100,
    details: { ...task().details, stage: 'finished', completed: 1, pending: 0 },
  });
  view.rerender(<TransferList connectionId="a" />);
  expect(within(screen.getByRole('region', { name: '详情' })).getByText('已完成')).toBeVisible();
  await userEvent.unhover(document.getElementById('transfer-task-a')!);
  await userEvent.keyboard('{Escape}');
  expect(fixture.call).not.toHaveBeenCalled();
});
it('keeps cancellation and cancelled-task retry on the existing commands', async () => {
  fixture.transfers.set('a', task());
  const view = render(<TransferList connectionId="a" />);
  await userEvent.click(screen.getByRole('button', { name: '取消' }));
  expect(fixture.call).toHaveBeenCalledWith('cancelTransfer', { id: 'a' });
  fixture.transfers.set('a', task('cancelled'));
  view.rerender(<TransferList connectionId="a" />);
  expect(screen.queryByRole('button', { name: '已结束 (1)' })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '重试' }));
  expect(fixture.call).toHaveBeenCalledWith('retryTransfer', { id: 'a' });
});
it('keeps hover details concise without loading children and preserves conflict decisions', async () => {
  fixture.transfers.set('a', { ...task('awaitingDecision'), conflictVersion: 3 });
  const call = fixture.call as ReturnType<typeof vi.fn>;
  render(<TransferList connectionId="a" />);
  await userEvent.hover(document.getElementById('transfer-task-a')!);
  await screen.findByRole('region', { name: '详情' });
  const details = within(screen.getByRole('region', { name: '详情' }));
  expect(details.getByText('/logs/access.log')).toBeVisible();
  expect(details.queryByText(/完成 0/)).not.toBeInTheDocument();
  expect(details.queryByRole('button')).not.toBeInTheDocument();
  expect(call).not.toHaveBeenCalled();
  await userEvent.unhover(document.getElementById('transfer-task-a')!);
  await userEvent.keyboard('{Escape}');
  await userEvent.click(screen.getByRole('button', { name: '处理冲突' }));
  expect(fixture.showConflicts).toHaveBeenCalledWith('a');
  expect(call).not.toHaveBeenCalled();
});

it('pins active tasks and orders both active and ended tasks by creation time, not arrival or completion', () => {
  const entries = [
    { ...task('complete'), id: 'old', name: 'old', createdAt: 1000 },
    { ...task('queued'), id: 'new', name: 'new', createdAt: 4000 },
    { ...task('running'), id: 'middle', name: 'middle', createdAt: 2000 },
    { ...task('failed'), id: 'failed', name: 'failed', createdAt: 3000 },
  ];
  for (const value of entries) fixture.transfers.set(value.id, value);
  const view = render(<TransferList connectionId="a" />);
  const order = () =>
    [...view.container.querySelectorAll('[id^="transfer-task-"]')].map((node) => node.id);
  expect(order()).toEqual([
    'transfer-task-new',
    'transfer-task-middle',
    'transfer-task-failed',
    'transfer-task-old',
  ]);
  const card = document.getElementById('transfer-task-new');
  fixture.transfers.set('new', { ...entries[1], phase: 'complete' });
  view.rerender(<TransferList connectionId="a" />);
  expect(order()).toEqual([
    'transfer-task-middle',
    'transfer-task-new',
    'transfer-task-failed',
    'transfer-task-old',
  ]);
  fixture.transfers.set('middle', { ...entries[2], phase: 'complete' });
  view.rerender(<TransferList connectionId="a" />);
  expect(order()).toEqual([
    'transfer-task-new',
    'transfer-task-failed',
    'transfer-task-middle',
    'transfer-task-old',
  ]);
  expect(document.getElementById('transfer-task-new')).toBe(card);
  expect(screen.queryByText('进行中')).not.toBeInTheDocument();
});
it('keeps equal-time tasks stable across unordered snapshots', () => {
  fixture.transfers.set('b', { ...task('complete'), id: 'b' });
  fixture.transfers.set('a', { ...task('complete'), id: 'a' });
  const view = render(<TransferList connectionId="a" />);
  const order = () =>
    [...view.container.querySelectorAll('[id^="transfer-task-"]')].map((node) => node.id);
  expect(order()).toEqual(['transfer-task-a', 'transfer-task-b']);
  fixture.transfers = new Map([...fixture.transfers].reverse());
  view.rerender(<TransferList connectionId="a" />);
  expect(order()).toEqual(['transfer-task-a', 'transfer-task-b']);
});
it('opens details from keyboard focus without a separate details button', async () => {
  fixture.transfers.set('a', task());
  render(<TransferList connectionId="a" />);
  await userEvent.tab();
  expect(document.activeElement?.id).toBe('transfer-task-a');
  expect(await screen.findByRole('region', { name: '详情' })).toHaveTextContent('/logs/access.log');
  expect(screen.queryByRole('button', { name: '详情' })).not.toBeInTheDocument();
});

it.each(['complete', 'failed', 'cancelled', 'skipped'])(
  'keeps metric slots through sampling gaps and removes them on %s',
  (phase) => {
    fixture.transfers.set('a', task());
    const view = render(<TransferList connectionId="a" />);
    const placeholders = screen.getAllByText('—');
    expect(placeholders).toHaveLength(1);
    const now = performance.now();
    fixture.samples.set('a', {
      points: [
        { time: now - 1000, bytes: 0 },
        { time: now, bytes: 25 },
      ],
    });
    view.rerender(<TransferList connectionId="a" />);
    expect(placeholders[0]).toHaveTextContent('25 B/s');
    expect(screen.queryByText('约 3 秒')).not.toBeInTheDocument();
    fixture.samples.clear();
    view.rerender(<TransferList connectionId="a" />);
    expect(placeholders[0]).toHaveTextContent('—');

    fixture.transfers.set('a', { ...task(phase), bytes: 100 });
    view.rerender(<TransferList connectionId="a" />);
    expect(placeholders[0]).not.toBeInTheDocument();

    expect(screen.queryByText('—')).not.toBeInTheDocument();
  },
);
it.each([
  [59, '约 59 秒'],
  [60, '约 1分0秒'],
  [150, '约 2分30秒'],
  [3600, '约 1时0分'],
  [7320, '约 2时2分'],
])('shows %s remaining seconds only in hover details', async (seconds, expected) => {
  fixture.transfers.set('a', { ...task(), bytes: 0, total: seconds * 10 });
  const now = performance.now();
  fixture.samples.set('a', {
    points: [
      { time: now - 1000, bytes: 0 },
      { time: now, bytes: 10 },
    ],
  });
  render(<TransferList connectionId="a" />);
  expect(screen.queryByText(expected)).not.toBeInTheDocument();
  await userEvent.hover(document.getElementById('transfer-task-a')!);
  const details = await screen.findByRole('region', { name: '详情' });
  expect(within(details).getByText('预计剩余时间')).toBeInTheDocument();
  expect(within(details).getByText(expected)).toBeInTheDocument();
});

it('puts active metrics inside progress and completed status beside size instead of the title', () => {
  fixture.transfers.set('a', task());
  const view = render(<TransferList connectionId="a" />);
  const bar = screen.getByRole('progressbar', { name: 'access.log' });
  expect(bar).toHaveAttribute('aria-valuenow', '25');
  expect(within(bar).getByText('25 B / 100 B')).toBeInTheDocument();
  expect(within(bar).getByText('25%')).toBeInTheDocument();
  fixture.transfers.set('a', { ...task('complete'), bytes: 100 });
  view.rerender(<TransferList connectionId="a" />);
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  const status = screen.getByText('已完成');
  expect(within(status.parentElement!).getByText('100 B')).toBeInTheDocument();
  expect(status.closest('[data-slot="card-title"]')).toBeNull();
});

it.each([
  ['queued', 'enumerating', '排队中'],
  ['running', 'enumerating', '枚举中'],
  ['running', 'transferring', '传输中'],
])('keeps %s/%s indeterminate with stage text until the total is known', (phase, stage, label) => {
  fixture.transfers.set('a', {
    ...task(phase),
    total: null,
    details: { ...task().details, stage },
  });
  const view = render(<TransferList connectionId="a" />);
  const bar = screen.getByRole('progressbar');
  expect(bar).toHaveAttribute('data-indeterminate');
  expect(bar).not.toHaveAttribute('aria-valuenow');
  expect(within(bar).getByText(label)).toBeInTheDocument();
  expect(within(bar).queryByText('0%')).not.toBeInTheDocument();
  fixture.transfers.set('a', task());
  view.rerender(<TransferList connectionId="a" />);
  expect(screen.getByRole('progressbar')).toBe(bar);
  expect(bar).not.toHaveAttribute('data-indeterminate');
  expect(bar).toHaveAttribute('aria-valuenow', '25');
});

it('reuses transfer cards for a bounded cross-connection preview with connection labels', () => {
  fixture.transfers.set('a', task());
  fixture.transfers.set('b', { ...task(), id: 'b', connectionId: 'b', name: 'other.txt' });
  fixture.transfers.set('c', { ...task(), id: 'c', connectionId: 'c', name: 'hidden.txt' });
  render(<TransferList previewIds={new Set(['a', 'b'])} />);
  expect(screen.getByText('Connection a')).toBeInTheDocument();
  expect(screen.getByText('Connection b')).toBeInTheDocument();
  expect(screen.queryByText('hidden.txt')).not.toBeInTheDocument();
  expect(document.getElementById('transfer-preview-a')).toBeInTheDocument();
  expect(document.getElementById('transfer-task-a')).not.toBeInTheDocument();
});
