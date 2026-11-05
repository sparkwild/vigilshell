// @vitest-environment jsdom
import { useSyncExternalStore } from 'react';
import { act, cleanup, render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { TerminalSessionView, WorkspaceNotification } from '@contracts';
import type { WorkspaceApi } from '../../../shared/workspace-api';
import { WorkspaceStore } from '../connections/workspace-store';
import { TerminalWorkspace } from './terminal-workspace';
const harness = vi.hoisted(() => ({
  store: null as WorkspaceStore | null,
  created: vi.fn(),
  disposed: vi.fn(),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: string) => key,
    snapshot: { appearance: { theme: 'light', locale: 'en' } },
  }),
}));
vi.mock('../connections/workspace-context', async (original) => ({
  ...(await original<typeof import('../connections/workspace-context')>()),
  useWorkspace: () => ({
    store: harness.store!,
    state: useSyncExternalStore(harness.store!.subscribe, harness.store!.getSnapshot),
  }),
}));
vi.mock('../connections/ui-context', () => ({
  useWorkspaceUi: () => ({ paste: vi.fn() }),
}));
vi.mock('../connections/OperationFeedback', () => ({
  OperationStatus: () => null,
}));
vi.mock('../connections/catalogue-view', () => ({
  ConnectionOverview: () => <div>Empty</div>,
}));
vi.mock('./ConnectionFailure', () => ({ ConnectionFailure: () => null }));
vi.mock('./terminal-controller', () => ({
  TerminalController: class {
    id: string;
    root = document.createElement('textarea');
    terminal = { getSelection: vi.fn(), clear: vi.fn() };
    search = { findNext: vi.fn(), findPrevious: vi.fn() };
    constructor(id: string) {
      this.id = id;
      this.root.value = `output-${id}`;
      harness.created(id);
    }
    attach(element: HTMLElement) {
      element.append(this.root);
    }
    detach() {
      this.root.remove();
    }
    state() {}
    language() {}
    appearance() {}
    fit() {}
    focus() {}
    dispose() {
      harness.disposed(this.id);
      this.root.remove();
    }
  },
}));
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
afterEach(() => {
  cleanup();
  harness.store?.dispose();
  vi.clearAllMocks();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
function setup() {
  let notify!: (note: WorkspaceNotification) => void;
  Object.assign(window, {
    vigilshell: {
      onWorkspaceEvent: (listener: typeof notify) => {
        notify = listener;
        return () => {};
      },
      attachTerminalPort: vi.fn(),
    },
  });
  harness.store = new WorkspaceStore({
    getConnection: vi.fn(async () => ({ ok: true, value: { kind: 'done' } })),
  } as unknown as WorkspaceApi);
  harness.store.backend('runtime', false);
  const emit = (
    id: string,
    phase: TerminalSessionView['phase'] = 'running',
    endReason: TerminalSessionView['endReason'] = null,
  ) =>
    notify({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: {
        runtimeId: 'runtime',
        owner: 'owner',
        event: {
          kind: 'session',
          session: {
            id,
            connectionId: 'server',
            groupId: 'group',
            connectionRevision: 1,
            operationId: id,
            name: id,
            username: 'tester',
            host: 'localhost',
            port: 22,
            phase,
            error: null,
            exitCode: null,
            endReason,
            terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
          },
        },
      },
    });
  return emit;
}
it('keeps terminal displays through switching and file maximization, and releases only closed displays', async () => {
  const emit = setup();
  emit('a');
  emit('b');
  const view = render(<TerminalWorkspace visible />);
  const textarea = screen.getByDisplayValue('output-a');
  for (let index = 0; index < 12; index++) act(() => harness.store!.tab(index % 2 ? 'a' : 'b'));
  view.rerender(<TerminalWorkspace visible={false} />);
  expect(screen.getByRole('tabpanel')).toHaveAttribute('data-terminal-visible', 'false');
  view.rerender(<TerminalWorkspace visible />);
  expect(screen.getByDisplayValue('output-a')).toBe(textarea);
  expect(harness.created).toHaveBeenCalledTimes(2);
  expect(window.vigilshell.attachTerminalPort).toHaveBeenCalledTimes(2);
  await act(async () => {
    emit('b', 'closed');
    await harness.store!.closeTerminal('b');
  });
  expect(harness.disposed).toHaveBeenCalledExactlyOnceWith('b');
  expect(harness.store!.terminals.size).toBe(1);
});

it('reopens a hidden popup for a new failure and retains it through remounting', () => {
  const emit = setup();
  emit('a');
  emit('b');
  const close = vi.spyOn(harness.store!, 'closeTerminal');
  const disconnect = vi.spyOn(harness.store!, 'disconnectTerminal');
  const view = render(<TerminalWorkspace visible />);
  const info = () => screen.queryByRole('complementary', { name: 'ws_connection_info' });
  expect(info()).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'ws_hide_connection_info' }));
  expect(info()).toBeNull();
  expect(close).not.toHaveBeenCalled();
  expect(disconnect).not.toHaveBeenCalled();
  expect(harness.store!.getSnapshot().hiddenTerminalInfo).toEqual(new Set(['a']));
  act(() => harness.store!.tab('b'));
  expect(info()).toBeInTheDocument();
  act(() => {
    harness.store!.tab('a');
    emit('a', 'failed');
  });
  expect(info()).toHaveAttribute('data-phase', 'failed');
  view.unmount();
  const restored = render(<TerminalWorkspace visible />);
  expect(info()).toHaveAttribute('data-phase', 'failed');
  restored.rerender(<TerminalWorkspace visible={false} />);
  expect(info()).toBeNull();
  restored.rerender(<TerminalWorkspace visible />);
  expect(info()).toBeInTheDocument();
  expect(harness.created).toHaveBeenCalledTimes(2);
});

function fakeClock() {
  vi.useFakeTimers({
    toFake: ['setTimeout', 'clearTimeout', 'Date', 'performance'],
  });
}
function advance(milliseconds: number) {
  act(() => vi.advanceTimersByTime(milliseconds));
}
function info() {
  return screen.queryByRole('complementary', { name: 'ws_connection_info' });
}

it('shows success for five seconds, restores automatic feedback on failure, and respects manual hiding', () => {
  fakeClock();
  const emit = setup();
  emit('a', 'connecting');
  render(<TerminalWorkspace visible />);
  advance(12000);
  expect(info()).toBeInTheDocument();
  act(() => emit('a', 'running'));
  expect(screen.getByText('ws_connection_success')).toBeInTheDocument();
  advance(4999);
  expect(info()).toBeInTheDocument();
  advance(1);
  expect(info()).toBeNull();
  act(() => emit('a', 'failed'));
  expect(info()).toHaveAttribute('data-phase', 'failed');
  advance(60000);
  expect(info()).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'ws_hide_connection_info' }));
  act(() => emit('a', 'failed'));
  expect(info()).toBeNull();
});
it("counts only the active terminal's visible time independently", () => {
  fakeClock();
  const emit = setup();
  emit('a');
  emit('b');
  render(<TerminalWorkspace visible />);
  advance(2000);
  act(() => harness.store!.tab('b'));
  advance(4000);
  expect(info()).toBeInTheDocument();
  expect(harness.store!.getSnapshot().hiddenTerminalInfo.size).toBe(0);
  act(() => harness.store!.tab('a'));
  advance(2999);
  expect(info()).toBeInTheDocument();
  advance(1);
  expect(info()).toBeNull();
  act(() => harness.store!.tab('b'));
  advance(999);
  expect(info()).toBeInTheDocument();
  advance(1);
  expect(info()).toBeNull();
});
it('pauses for pointer hover, keyboard focus and a hidden terminal pane', () => {
  fakeClock();
  const emit = setup();
  emit('a');
  const view = render(<TerminalWorkspace visible />);
  advance(1000);
  fireEvent.pointerEnter(info()!);
  advance(10000);
  expect(info()).toBeInTheDocument();
  fireEvent.pointerLeave(info()!);
  advance(1000);
  act(() => screen.getByRole('button', { name: 'ws_hide_connection_info' }).focus());
  advance(10000);
  expect(info()).toBeInTheDocument();
  act(() => screen.getByDisplayValue('output-a').focus());
  advance(1000);
  view.rerender(<TerminalWorkspace visible={false} />);
  advance(20000);
  expect(info()).toBeNull();
  view.rerender(<TerminalWorkspace visible />);
  advance(1999);
  expect(info()).toBeInTheDocument();
  advance(1);
  expect(info()).toBeNull();
});
it('preserves remaining time on remount and pins only a new failure', () => {
  fakeClock();
  const emit = setup();
  emit('a');
  const view = render(<TerminalWorkspace visible />);
  advance(2000);
  view.unmount();
  advance(20000);
  render(<TerminalWorkspace visible />);
  advance(2999);
  expect(info()).toBeInTheDocument();
  advance(1);
  expect(info()).toBeNull();
  act(() => emit('a', 'failed'));
  advance(60000);
  expect(info()).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'ws_hide_connection_info' }));
  expect(info()).toBeNull();
  act(() => emit('a', 'failed'));
  expect(info()).toBeNull();
  expect(harness.created).toHaveBeenCalledTimes(1);
});
it('pauses when the document is hidden and does not restart on repeated running events', () => {
  fakeClock();
  let hidden = false;
  vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
  const emit = setup();
  emit('a');
  render(<TerminalWorkspace visible />);
  advance(2000);
  hidden = true;
  fireEvent(document, new Event('visibilitychange'));
  advance(60000);
  expect(harness.store!.getSnapshot().hiddenTerminalInfo.size).toBe(0);
  hidden = false;
  fireEvent(document, new Event('visibilitychange'));
  advance(2000);
  act(() => emit('a'));
  advance(1000);
  expect(info()).toBeNull();
});

it.each(['automatic', 'manual'] as const)(
  'shows a simple manual-disconnect popup after %s hiding, and does not reopen it on duplicate events',
  (hiding) => {
    fakeClock();
    const emit = setup();
    emit('a');
    render(<TerminalWorkspace visible />);
    if (hiding === 'automatic') advance(5000);
    else fireEvent.click(screen.getByRole('button', { name: 'ws_hide_connection_info' }));
    expect(info()).toBeNull();
    act(() => harness.store!.terminalStreamEnded('a'));
    act(() => emit('a', 'closed', { kind: 'userDisconnected', message: null }));
    expect(info()).toHaveAttribute('data-phase', 'closed');
    expect(screen.getByRole('button', { name: 'ws_reconnect' })).toBeInTheDocument();
    expect(screen.queryByText('connection_details')).not.toBeInTheDocument();
    advance(60000);
    expect(info()).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'ws_hide_connection_info' }));
    act(() => emit('a', 'closed', { kind: 'userDisconnected', message: null }));
    expect(info()).toBeNull();
  },
);
