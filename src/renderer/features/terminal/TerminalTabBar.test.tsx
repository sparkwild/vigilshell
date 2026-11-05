// @vitest-environment jsdom
import { useSyncExternalStore, type ReactNode } from 'react';
import {
  act,
  cleanup,
  fireEvent,
  render as renderTree,
  within,
  screen,
  waitFor,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { TerminalSessionView, WorkspaceNotification } from '@contracts';
import type { WorkspaceApi } from '../../../shared/workspace-api';
import { translate, type MessageKey } from '../../../shared/i18n/messages';
import { WorkspaceStore } from '../connections/workspace-store';
import { ConfirmationProvider } from '../../shared/ConfirmationProvider';
import { TerminalTabBar } from './TerminalTabBar';
import { terminalPanelId, terminalTabId } from './terminal-tab-model';

const context = vi.hoisted(() => ({ store: null as WorkspaceStore | null }));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: MessageKey, values?: Record<string, string | number>) =>
      translate(key, 'zh-CN', values),
  }),
}));
vi.mock('../connections/workspace-context', async (original) => ({
  ...(await original<typeof import('../connections/workspace-context')>()),
  useWorkspace: () => {
    const store = context.store!;
    const state = useSyncExternalStore(store.subscribe, store.getSnapshot);
    return { store, state };
  },
}));
vi.mock('../connections/OperationFeedback', () => ({
  OperationButtonIcon: ({ children }: { children: React.ReactNode }) => children,
}));
function render(node: ReactNode) {
  return renderTree(<ConfirmationProvider>{node}</ConfirmationProvider>);
}
let emit: (id: string, phase?: TerminalSessionView['phase']) => void;
let available = 200;
let resized = () => {};
beforeEach(() => {
  available = 200;
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn().mockReturnValue({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
  globalThis.ResizeObserver = class {
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) {
      if (target.classList.contains('terminal-tab-navigation'))
        resized = () =>
          this.callback(
            [
              {
                target,
                contentRect: { width: available },
              } as ResizeObserverEntry,
            ],
            this as unknown as ResizeObserver,
          );
    }
    unobserve() {}
    disconnect() {}
  };
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.classList.contains('terminal-tab-navigation') ? available : 0;
  });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    return new DOMRect(0, 0, this.classList.contains('terminal-tab-label-measure') ? 160 : 0, 16);
  });
  let notify!: (note: WorkspaceNotification) => void;
  Object.assign(window, {
    vigilshell: {
      copyText: vi.fn(async () => {}),
      onWorkspaceEvent: (listener: typeof notify) => {
        notify = listener;
        return () => {};
      },
    },
  });
  context.store = new WorkspaceStore({
    closeTerminal: vi.fn(async () => ({ ok: true, value: { kind: 'done' } })),
    getConnection: vi.fn(async () => ({ ok: true, value: { kind: 'done' } })),
  } as unknown as WorkspaceApi);
  context.store.backend('runtime', false);
  emit = (id, phase = 'running') =>
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
            operationId: `op-${id}`,
            name: 'server',
            username: 'fixture',
            host: 'localhost',
            port: 22,
            phase,
            error: null,
            exitCode: null,
            terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
          },
        },
      },
    });
});
afterEach(() => {
  cleanup();
  context.store?.dispose();
  vi.restoreAllMocks();
});
function Panels({ visible = true }: { visible?: boolean }) {
  const state = useSyncExternalStore(context.store!.subscribe, context.store!.getSnapshot);
  return (
    <>
      {state.sessions.map((s) => (
        <section
          key={s.id}
          id={terminalPanelId(s.id)}
          role="tabpanel"
          aria-labelledby={terminalTabId(s.id)}
          hidden={s.id !== state.activeTab}
          data-terminal-visible={visible && s.id === state.activeTab}
        >
          <textarea className="xterm-helper-textarea" aria-label={`input-${s.id}`} />
        </section>
      ))}
    </>
  );
}
it('renders no controls for the empty workspace', () => {
  const { container } = render(<TerminalTabBar />);
  expect(container).toBeEmptyDOMElement();
});
it('supports 32 active sessions plus ended tabs without changing their order', async () => {
  for (let i = 0; i < 36; i++) emit(String(i), i < 32 ? 'running' : 'closed');
  render(<TerminalTabBar />);
  expect(screen.getAllByRole('tab')).toHaveLength(1);
  expect(screen.getByRole('button', { name: '全部会话' })).toHaveTextContent('35');
  await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
  await screen.findByRole('dialog', { name: '全部会话' });
  const dialog = screen.getByRole('dialog', { name: '全部会话' });
  expect(dialog.querySelectorAll('.terminal-session-row')).toHaveLength(36);
  await userEvent
    .setup()
    .click(within(dialog).getByRole('button', { name: '关闭连接 — localhost · 36' }));
  expect(screen.getAllByRole('tab')).toHaveLength(1);
  expect(context.store!.getSnapshot().sessions.map((s) => s.id)).toEqual(
    Array.from({ length: 35 }, (_, i) => String(i)),
  );
});
it('arrow keys move focus only; Enter activates the focused tab and exposes its panel', async () => {
  available = 600;
  emit('a');
  emit('b');
  render(
    <>
      <TerminalTabBar />
      <Panels />
    </>,
  );
  const tabs = screen.getAllByRole('tab');
  tabs[0]!.focus();
  await userEvent.setup().keyboard('{ArrowRight}');
  expect(tabs[1]).toHaveFocus();
  expect(context.store!.getSnapshot().activeTab).toBe('a');
  await userEvent.setup().keyboard('{Enter}');
  expect(context.store!.getSnapshot().activeTab).toBe('b');
  expect(tabs[1]).toHaveAttribute('aria-controls', terminalPanelId('b'));
  expect(screen.getByRole('tabpanel')).toHaveAttribute('id', terminalPanelId('b'));
});
it.each([true, false])(
  'menu selection restores focus according to terminal visibility (%s)',
  async (visible) => {
    emit('a');
    emit('b');
    render(
      <>
        <TerminalTabBar />
        <Panels visible={visible} />
      </>,
    );
    await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
    await screen.findByRole('dialog', { name: '全部会话' });
    await userEvent.setup().click(
      within(screen.getByRole('dialog', { name: '全部会话' })).getByRole('button', {
        name: 'localhost · 2 — 已连接',
      }),
    );
    expect(context.store!.getSnapshot().activeTab).toBe('b');
    await waitFor(() =>
      expect(
        visible
          ? screen.getByRole('textbox', { name: 'input-b' })
          : screen.getByRole('button', { name: '全部会话' }),
      ).toHaveFocus(),
    );
  },
);
it('Escape closes the menu without activating another session', async () => {
  emit('a');
  emit('b');
  render(<TerminalTabBar />);
  await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
  await screen.findByRole('dialog', { name: '全部会话' });
  await userEvent.setup().keyboard('{ArrowDown}{Escape}');
  expect(context.store!.getSnapshot().activeTab).toBe('a');
  await waitFor(() => expect(screen.getByRole('button', { name: '全部会话' })).toHaveFocus());
});
it('collects whole tabs on resize, keeps the active one visible, and restores original order', async () => {
  available = 600;
  emit('a');
  emit('b');
  emit('c');
  context.store!.tab('c');
  render(<TerminalTabBar />);
  const visibleIds = () => screen.getAllByRole('tab').map((tab) => tab.id);
  expect(visibleIds()).toEqual(['a', 'b', 'c'].map(terminalTabId));
  available = 450;
  act(() => resized());
  await waitFor(() => expect(visibleIds()).toEqual(['a', 'c'].map(terminalTabId)));
  expect(screen.getByRole('button', { name: '全部会话' })).toHaveTextContent('1');
  screen.getAllByRole('tab')[0]!.focus();
  await userEvent.setup().keyboard('{ArrowRight}');
  expect(document.getElementById(terminalTabId('c'))).toHaveFocus();
  expect(context.store!.getSnapshot().activeTab).toBe('c');
  expect(screen.queryByRole('button', { name: '向左滚动标签' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '向右滚动标签' })).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole('button', { name: '全部会话' }));
  await userEvent.setup().click(
    within(await screen.findByRole('dialog', { name: '全部会话' })).getByRole('button', {
      name: 'localhost · 2 — 已连接',
    }),
  );
  expect(visibleIds()).toEqual(['a', 'b'].map(terminalTabId));
  expect(context.store!.getSnapshot().activeTab).toBe('b');
  available = 600;
  act(() => resized());
  await waitFor(() => expect(visibleIds()).toEqual(['a', 'b', 'c'].map(terminalTabId)));
  expect(screen.queryByRole('button', { name: '全部会话' })).not.toBeInTheDocument();
});

it('hides the overflow entry when tabs fit and offers the actions from the tab context menu', async () => {
  emit('a');
  render(<TerminalTabBar />);
  expect(screen.queryByRole('button', { name: '全部会话' })).not.toBeInTheDocument();
  fireEvent.contextMenu(screen.getByRole('tab'));
  await screen.findByRole('menu');
  expect(screen.getByRole('menuitem', { name: '关闭连接' })).toBeEnabled();
  expect(screen.getByRole('menuitem', { name: '断开连接' })).toBeEnabled();
  expect(screen.queryByRole('menuitem', { name: '关闭全部' })).not.toBeInTheDocument();
  expect(screen.queryByRole('menuitem', { name: '全部会话' })).not.toBeInTheDocument();
});
it('confirms close-all once and excludes tabs opened during confirmation', async () => {
  emit('a');
  emit('b');
  render(<TerminalTabBar />);
  await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
  const popover = await screen.findByRole('dialog', { name: '全部会话' });
  await userEvent.setup().click(within(popover).getByRole('button', { name: '关闭全部' }));
  const confirmation = await screen.findByRole('alertdialog');
  expect(confirmation).toHaveTextContent('2 个终端会话');
  act(() => emit('new'));
  await userEvent.setup().click(within(confirmation).getByRole('button', { name: '关闭全部' }));
  await waitFor(() =>
    expect(context.store!.getSnapshot().sessions.map((s) => s.id)).toEqual(['new']),
  );
});
it('canceling close-all leaves every session untouched', async () => {
  emit('a');
  emit('b');
  render(<TerminalTabBar />);
  const close = vi.spyOn(context.store!, 'closeTerminals');
  await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
  const popover = await screen.findByRole('dialog', { name: '全部会话' });
  await userEvent.setup().click(within(popover).getByRole('button', { name: '关闭全部' }));
  const confirmation = await screen.findByRole('alertdialog');
  await userEvent.setup().click(within(confirmation).getByRole('button', { name: '取消' }));
  expect(close).not.toHaveBeenCalled();
  expect(context.store!.getSnapshot().sessions).toHaveLength(2);
});

it('keeps failed closes in the compact list with a reason', async () => {
  emit('a');
  emit('b');
  vi.spyOn(context.store!, 'closeTerminals').mockResolvedValue({
    failed: [{ id: 'a', code: 'busy' }],
    cancelled: false,
  });
  render(<TerminalTabBar />);
  await userEvent.setup().click(await screen.findByRole('button', { name: '全部会话' }));
  await userEvent.setup().click(
    within(await screen.findByRole('dialog', { name: '全部会话' })).getByRole('button', {
      name: '关闭全部',
    }),
  );
  await userEvent.setup().click(
    within(await screen.findByRole('alertdialog')).getByRole('button', {
      name: '关闭全部',
    }),
  );
  const list = await screen.findByRole('dialog', { name: '全部会话' });
  expect(within(list).getByRole('alert')).not.toBeEmptyDOMElement();
  expect(context.store!.getSnapshot().sessions).toHaveLength(2);
});
it('keeps session close buttons but dismisses the list through its trigger', async () => {
  emit('a');
  emit('b');
  render(<TerminalTabBar />);
  const user = userEvent.setup();
  await user.keyboard('{Tab}{Tab}');
  expect(await screen.findByText('关闭连接', { exact: true })).toBeVisible();
  await user.click(await screen.findByRole('button', { name: '全部会话' }));
  const list = await screen.findByRole('dialog', { name: '全部会话' });
  expect(list.querySelector('.terminal-session-heading button')).toBeNull();
  expect(within(list).getAllByRole('button', { name: /关闭连接 — / })).toHaveLength(2);
  await user.click(screen.getByRole('button', { name: '全部会话' }));
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: '全部会话' })).not.toBeInTheDocument(),
  );
  expect(context.store!.getSnapshot().sessions).toHaveLength(2);
});

it('binds the context menu to the clicked tab even when the active terminal changes', async () => {
  available = 600;
  emit('a');
  emit('b');
  emit('c');
  render(<TerminalTabBar />);
  const disconnect = vi.spyOn(context.store!, 'disconnectTerminal');
  fireEvent.contextMenu(screen.getAllByRole('tab')[1]!);
  await screen.findByRole('menu');
  expect(context.store!.getSnapshot().activeTab).toBe('a');
  act(() => context.store!.tab('c'));
  await userEvent.setup().click(screen.getByRole('menuitem', { name: '断开连接' }));
  expect(disconnect).toHaveBeenCalledExactlyOnceWith('b');
  expect(context.store!.getSnapshot().sessions).toHaveLength(3);
  expect(context.store!.getSnapshot().activeTab).toBe('c');
});
it('closes only the right-clicked connection and leaves the active tab untouched', async () => {
  available = 600;
  emit('a');
  emit('b');
  render(<TerminalTabBar />);
  fireEvent.contextMenu(screen.getAllByRole('tab')[1]!);
  await userEvent.setup().click(await screen.findByRole('menuitem', { name: '关闭连接' }));
  await waitFor(() =>
    expect(context.store!.getSnapshot().sessions.map((s) => s.id)).toEqual(['a']),
  );
  expect(context.store!.getSnapshot().activeTab).toBe('a');
});
it('disables disconnect for an ended session while allowing copy and close', async () => {
  emit('a', 'closed');
  render(<TerminalTabBar />);
  fireEvent.contextMenu(screen.getByRole('tab'));
  await screen.findByRole('menu');
  expect(screen.getByRole('menuitem', { name: '断开连接' })).toHaveAttribute(
    'aria-disabled',
    'true',
  );
  await userEvent.setup().click(screen.getByRole('menuitem', { name: '复制连接地址' }));
  expect(window.vigilshell.copyText).toHaveBeenCalledExactlyOnceWith('fixture@localhost:22');
});
it('does not show a global context menu on empty tab-strip space', () => {
  emit('a');
  const { container } = render(<TerminalTabBar />);
  fireEvent.contextMenu(container.querySelector('.terminal-tab-navigation')!);
  expect(screen.queryByRole('menu')).not.toBeInTheDocument();
});

it('offers reconnect only for ended sessions, targets the clicked tab, and removes manual popup controls', async () => {
  available = 600;
  emit('a');
  emit('b');
  const reconnect = vi.spyOn(context.store!, 'reconnectTerminal').mockResolvedValue(null);
  const canReconnect = vi.spyOn(context.store!, 'canReconnectTerminal').mockReturnValue(false);
  render(<TerminalTabBar />);
  fireEvent.contextMenu(screen.getAllByRole('tab')[1]!);
  expect(await screen.findByRole('menuitem', { name: '重新连接' })).toHaveAttribute(
    'aria-disabled',
    'true',
  );
  expect(screen.queryByRole('menuitem', { name: /显示连接信息|隐藏连接信息/ })).toBeNull();
  await userEvent.setup().keyboard('{Escape}');
  canReconnect.mockReturnValue(true);
  fireEvent.contextMenu(screen.getAllByRole('tab')[1]!);
  await userEvent.setup().click(await screen.findByRole('menuitem', { name: '重新连接' }));
  expect(reconnect).toHaveBeenCalledExactlyOnceWith('b');
  expect(context.store!.getSnapshot().activeTab).toBe('a');
});

it('collects four tabs at natural width and keeps the active session number separate from truncation', () => {
  available = 520;
  emit('a');
  emit('b');
  emit('c');
  emit('d');
  context.store!.tab('d');
  render(<TerminalTabBar />);
  const tabs = screen.getAllByRole('tab');
  expect(tabs.map((tab) => tab.id)).toEqual(['a', 'd'].map(terminalTabId));
  expect(tabs[0]!.parentElement).toHaveStyle({ width: '192px' });
  expect(tabs[1]!.parentElement).toHaveStyle({ width: '192px' });
  expect(tabs[1]!.querySelector('.terminal-tab-label')).toHaveTextContent('localhost');
  expect(tabs[1]!.querySelector('.terminal-tab-ordinal')).toHaveTextContent('· 4');
  expect(screen.getByRole('button', { name: '全部会话' })).toHaveTextContent('2');
});
