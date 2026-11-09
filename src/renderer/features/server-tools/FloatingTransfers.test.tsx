// @vitest-environment jsdom
import { render, screen, cleanup, waitFor, act } from '@testing-library/react';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { FloatingTransfers } from './FloatingTransfers';
import { translate } from '../../../shared/i18n/messages';
const fixture = vi.hoisted(() => ({
  state: {
    runtimeId: 'runtime',
    transportReady: true,
    ready: true,
    transfers: new Map(),
    sessions: [{ id: 'session', connectionId: 'connection' }],
    activeTab: 'session',
    toolsTab: 'overview',
    transferWindows: [] as string[],
    transferConflictTarget: null as object | null,
    editor: null,
    connections: [{ id: 'connection', name: 'Test connection' }],
  },
  targets: new Map(),
  show: vi.fn(),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: fixture.state,
    store: {
      transferTargets: fixture.targets,
      connectionLabel: () => 'Test connection',
      showTransfersInSidebar: fixture.show,
    },
  }),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Record<string, string | number>) =>
      translate(key, 'zh-CN', values),
  }),
}));
vi.mock('./TransferPanel', () => ({
  TransferList: ({ previewIds }: { previewIds: Set<string> }) => (
    <div>
      {[...previewIds].map((id) => (
        <p key={id}>{id}</p>
      ))}
    </div>
  ),
}));
const task = (id: string) => ({
  id,
  connectionId: 'connection',
  batchId: id,
  phase: 'running',
  createdAt: Number(id) || 1,
});
beforeEach(() => {
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
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
  fixture.state.transfers = new Map([['a', task('a')]]);
  fixture.state.sessions = [{ id: 'session', connectionId: 'connection' }];
  fixture.state.activeTab = 'session';
  fixture.state.toolsTab = 'overview';
  fixture.state.runtimeId = 'runtime';
  fixture.state.transferWindows = [];
  fixture.state.transferConflictTarget = null;
  fixture.targets.clear();
  fixture.show.mockClear();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
it('automatically opens without moving focus when the inspector is closed or on overview', async () => {
  const view = render(
    <>
      <input aria-label="terminal" />
      <FloatingTransfers active sidebarVisible />
    </>,
  );
  const input = screen.getByRole('textbox');
  input.focus();
  await screen.findByRole('dialog', { name: '传输动态' });
  expect(document.activeElement).toBe(input);
  fixture.state.toolsTab = 'transfers';
  view.rerender(
    <>
      <input aria-label="terminal" />
      <FloatingTransfers active sidebarVisible />
    </>,
  );
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument(),
  );
});
it('does not reopen the same batch after dismissal and allows manual reopening', async () => {
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  await screen.findByRole('dialog', { name: '传输动态' });
  await userEvent.click(screen.getByRole('button', { name: '关闭' }));
  fixture.state.transfers = new Map([['a', { ...task('a'), bytes: 50 }]]);
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '文件传输' }));
  expect(await screen.findByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
});
it('limits the preview to three items and opens the right transfer sidebar for the connection', async () => {
  fixture.state.transfers = new Map(['1', '2', '3', '4'].map((id) => [id, task(id)]));
  render(<FloatingTransfers active sidebarVisible={false} />);
  await screen.findByRole('dialog', { name: '传输动态' });
  expect(screen.queryByText('1')).not.toBeInTheDocument();
  expect(screen.getByText('3')).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '显示全部' }));
  expect(fixture.show).toHaveBeenCalledWith('connection');
});
it('gives way to conflict dialogs and hidden workspaces', async () => {
  fixture.state.transferConflictTarget = {};
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
  fixture.state.transferConflictTarget = null;
  view.rerender(<FloatingTransfers active={false} sidebarVisible={false} />);
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(await screen.findByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
});
it('only suppresses activity covered by the independent window batch filter', async () => {
  fixture.state.transferWindows = ['connection'];
  fixture.targets.set('connection', {
    runtimeId: 'runtime',
    connectionId: 'connection',
    batchId: 'other',
  });
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  await screen.findByRole('dialog', { name: '传输动态' });
  fixture.targets.clear();
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument(),
  );
});

it('retains completion for four seconds and closes without moving keyboard focus', async () => {
  const view = render(
    <>
      <input aria-label="terminal" />
      <FloatingTransfers active sidebarVisible={false} />
    </>,
  );
  await screen.findByRole('dialog', { name: '传输动态' });
  const input = screen.getByRole('textbox');
  input.focus();
  fixture.state.transfers = new Map([['a', { ...task('a'), phase: 'complete' }]]);
  view.rerender(
    <>
      <input aria-label="terminal" />
      <FloatingTransfers active sidebarVisible={false} />
    </>,
  );
  expect(screen.getByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
  await waitFor(
    () => expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument(),
    { timeout: 5000 },
  );
  expect(document.activeElement).toBe(input);
}, 8000);
it('expires completion when an early timer update is processed after its deadline', async () => {
  vi.useFakeTimers();
  const started = Date.now();
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  fixture.state.transfers = new Map([['a', { ...task('a'), phase: 'complete' }]]);
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.getByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
  await act(() => vi.advanceTimersByTimeAsync(3999));
  vi.spyOn(Date, 'now')
    .mockReturnValueOnce(started + 3999)
    .mockReturnValue(started + 4000);
  await act(() => vi.advanceTimersByTimeAsync(1));
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(screen.getByRole('button', { name: '文件传输' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  await act(() => vi.advanceTimersByTimeAsync(100));
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
});
it('hides automatically while a modal is present and resumes afterward', async () => {
  const view = render(
    <>
      <div role="dialog" aria-modal="true">
        form
      </div>
      <FloatingTransfers active sidebarVisible={false} />
    </>,
  );
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument(),
  );
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(await screen.findByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
});

it('hides both the floating panel and its entry after the last terminal tab closes', async () => {
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  await screen.findByRole('dialog', { name: '传输动态' });
  fixture.state.sessions = [];
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '文件传输' })).not.toBeInTheDocument();
  expect(fixture.state.transfers.has('a')).toBe(true);
});
it('keeps a connection visible until its last tab closes and excludes closed connections from manual previews', async () => {
  fixture.state.sessions = [
    { id: 'one', connectionId: 'connection' },
    { id: 'two', connectionId: 'connection' },
    { id: 'other', connectionId: 'other' },
  ];
  fixture.state.transfers = new Map([
    ['a', task('a')],
    ['b', { ...task('b'), connectionId: 'other' }],
  ]);
  fixture.state.activeTab = 'other';
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  await screen.findByRole('dialog', { name: '传输动态' });
  fixture.state.sessions = fixture.state.sessions.filter((s) => s.id !== 'one');
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.getByText('a')).toBeInTheDocument();
  fixture.state.sessions = fixture.state.sessions.filter((s) => s.id !== 'two');
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByText('a')).not.toBeInTheDocument();
  expect(screen.getByText('b')).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '关闭' }));
  await userEvent.click(screen.getByRole('button', { name: '文件传输' }));
  expect(screen.queryByText('a')).not.toBeInTheDocument();
  expect(screen.getByText('b')).toBeInTheDocument();
});

it('shows the entry only when the current terminal connection has transfer records', () => {
  fixture.state.sessions.push({ id: 'other', connectionId: 'other' });
  fixture.state.transfers = new Map();
  const view = render(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('button', { name: '文件传输' })).not.toBeInTheDocument();

  fixture.state.transfers = new Map([['a', { ...task('a'), phase: 'complete' }]]);
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.getByRole('button', { name: '文件传输' })).toBeInTheDocument();

  fixture.state.activeTab = 'other';
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('button', { name: '文件传输' })).not.toBeInTheDocument();

  fixture.state.activeTab = 'session';
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.getByRole('button', { name: '文件传输' })).toBeInTheDocument();

  fixture.state.transfers = new Map();
  view.rerender(<FloatingTransfers active sidebarVisible={false} />);
  expect(screen.queryByRole('button', { name: '文件传输' })).not.toBeInTheDocument();
});

it('keeps the preview hidden behind an opening task dialog and when a nested conflict closes', async () => {
  const view = render(
    <>
      <div
        role="dialog"
        data-slot="dialog-content"
        data-mode="modal"
        data-open=""
        style={{ opacity: 0 }}
      >
        File tasks
      </div>
      <div role="dialog" aria-modal="true">
        Conflict
      </div>
      <FloatingTransfers key="preview" active sidebarVisible={false} />
    </>,
  );
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument(),
  );
  fixture.state.transfers = new Map([['a', { ...task('a'), phase: 'complete' }]]);
  view.rerender(
    <>
      <div role="dialog" data-slot="dialog-content" data-mode="modal" data-open="">
        File tasks
      </div>
      <FloatingTransfers key="preview" active sidebarVisible={false} />
    </>,
  );
  expect(screen.queryByRole('dialog', { name: '传输动态' })).not.toBeInTheDocument();
  view.rerender(
    <>
      <FloatingTransfers key="preview" active sidebarVisible={false} />
    </>,
  );
  expect(await screen.findByRole('dialog', { name: '传输动态' })).toBeInTheDocument();
});
