// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { translate } from '../../../shared/i18n/messages';
import { TransferCloseDialog } from './tool-dialogs';
import { parentPath, validFileName } from './file-browser';
import { ServerToolsPanel } from './ServerToolsPanel';
import type { TerminalSessionView } from '@contracts';
const fixtures = vi.hoisted(() => ({
  activeTab: 'one',
  sharedConnection: false,
  closedFirst: false,
  toolsTab: 'overview',
  monitorDisplay: 'bar',
  selected: 'two',
  phase: 'running' as TerminalSessionView['phase'],
  calls: vi.fn().mockResolvedValue({ kind: 'done' }),
  preferences: new Map(),
  tools: new Map(),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    snapshot: {
      preferences: {
        monitorDisplay: fixtures.monitorDisplay,
        monitorSampleSeconds: 5,
        monitorFullSeconds: 60,
      },
    },
    t: (key: Parameters<typeof translate>[0], values?: Record<string, string | number>) =>
      translate(key, 'zh-CN', values),
  }),
}));
function session(id: string): TerminalSessionView {
  return {
    id,
    groupId: fixtures.sharedConnection ? 'group-one' : `group-${id}`,
    connectionId: fixtures.sharedConnection ? 'one' : id,
    operationId: id,
    connectionRevision: 1,
    host: `${id}.example.test`,
    username: 'fixture',
    port: 22,
    name: id,
    phase: fixtures.phase,
    error: null,
    exitCode: null,
    terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
  };
}
vi.mock('../connections/workspace-context', () => {
  const store = {
    feedback: { setAnchor: vi.fn() },
    call: fixtures.calls,
    refreshTransfers: vi.fn(async () => {}),
    selectToolsTab: vi.fn(),
    toolScrollPosition: (id: string) => fixtures.preferences.get(id) ?? 0,
    saveToolScrollPosition: (id: string, value: number) => fixtures.preferences.set(id, value),
  };
  return {
    useWorkspace: () => ({
      state: {
        activeTab: fixtures.activeTab,
        selected: { id: fixtures.selected },
        sessions: [session('one'), session('two')].filter(
          (s) => !fixtures.closedFirst || s.id !== 'one',
        ),
        connections: [],
        runtimeId: 'runtime',
        transportReady: true,
        ready: true,
        tools: fixtures.tools,
        toolsTab: fixtures.toolsTab,
        transfers: new Map(),
      },
      store,
    }),
    errorCode: () => 'tools_unavailable',
    errorKey: () => 'ws_error_tools_unavailable',
    phaseKey: (phase: string) => `ws_${phase}`,
  };
});
afterEach(cleanup);
beforeEach(() => {
  fixtures.calls.mockClear();
  fixtures.phase = 'running';
  fixtures.activeTab = 'one';
  fixtures.sharedConnection = false;
  fixtures.closedFirst = false;
  fixtures.toolsTab = 'overview';
  fixtures.monitorDisplay = 'bar';
  fixtures.selected = 'two';
  fixtures.preferences.clear();
  fixtures.tools.clear();
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn().mockImplementation(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
it('binds tools to the active terminal, ignores sidebar selection, and unsubscribes when hidden', async () => {
  const rendered = render(<ServerToolsPanel visible />);
  expect(screen.getByText('fixture@one.example.test:22', { selector: 'dd' })).toBeVisible();
  expect(screen.getByRole('tabpanel', { name: '概览' }).querySelector('dt')).toHaveTextContent(
    '连接目标',
  );
  expect(screen.queryByText('one', { selector: 'strong' })).not.toBeInTheDocument();
  expect(screen.queryByText('已连接')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument();
  expect(screen.queryByText('fixture@two.example.test:22')).not.toBeInTheDocument();
  await waitFor(() =>
    expect(fixtures.calls).toHaveBeenCalledWith('subscribeTools', {
      groupId: 'group-one',
      visible: true,
    }),
  );
  fixtures.selected = 'one';
  fixtures.activeTab = 'two';
  rendered.rerender(<ServerToolsPanel visible />);
  expect(screen.getByText('fixture@two.example.test:22')).toBeVisible();
  expect(fixtures.calls).toHaveBeenCalledWith('subscribeTools', {
    groupId: 'group-one',
    visible: false,
  });
  rendered.rerender(<ServerToolsPanel visible={false} />);
  expect(fixtures.calls).toHaveBeenCalledWith('subscribeTools', {
    groupId: 'group-two',
    visible: false,
  });
});
it('never connects from an empty tools panel', () => {
  fixtures.activeTab = 'overview';
  render(<ServerToolsPanel visible />);
  expect(screen.getByText('连接服务器后查看')).toBeVisible();
  expect(fixtures.calls).not.toHaveBeenCalled();
});
it.each([
  ['取消关闭', 'cancel'],
  ['后台传输并关闭终端', 'background'],
  ['取消传输并关闭终端', 'cancelTransfers'],
] as const)('closing with transfers leaves the %s choice to the user', async (label, choice) => {
  const choose = vi.fn();
  render(<TransferCloseDialog session={session('one')} choose={choose} />);
  await waitFor(() => expect(screen.getByRole('button', { name: '取消关闭' })).toHaveFocus());
  expect(choose).not.toHaveBeenCalled();
  await userEvent.setup().click(screen.getByRole('button', { name: label }));
  expect(choose).toHaveBeenCalledExactlyOnceWith(choice);
});
it('keeps file names separate from directory paths', () => {
  expect(validFileName('中文 空格.txt')).toBe(true);
  for (const value of ['', '.', '..', '../file', 'x/y', 'x\0y'])
    expect(validFileName(value)).toBe(false);
  expect(parentPath('/home/user')).toBe('/home');
  expect(parentPath('/')).toBe('/');
});

it.each([
  ['credential_denied', '重试'],
  ['tools_unavailable', '重试'],
])('offers recovery for %s without issuing it on render', async (error, label) => {
  fixtures.tools.set('group-one', {
    sample: {
      phase: 'unavailable',
      error,
      disks: [],
      cpuPercent: null,
      memoryTotal: null,
      memoryAvailable: null,
      collectedAt: null,
    },
  });
  const view = render(<ServerToolsPanel visible />);
  const retry = screen.getByRole('button', { name: label });
  expect(fixtures.calls.mock.calls.some(([method]) => method === 'refreshTools')).toBe(false);
  await userEvent.click(retry);
  expect(fixtures.calls).toHaveBeenCalledWith('refreshTools', {
    sessionId: 'one',
  });
  fixtures.tools.set('group-one', {
    sample: { ...fixtures.tools.get('group-one').sample, phase: 'ready', error: null },
  });
  view.rerender(<ServerToolsPanel visible />);
  expect(screen.queryByRole('button', { name: label })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument();
});

it('keeps the target in overview and marks retained data as stopped without a repeated header', () => {
  fixtures.phase = 'failed';
  fixtures.tools.set('group-one', {
    sample: {
      hostname: 'remote-host',
      disks: [],
      cpuPercent: 12,
      memoryTotal: null,
      memoryAvailable: null,
      collectedAt: '2026-09-20T08:00:00Z',
    },
  });
  render(<ServerToolsPanel visible />);
  expect(screen.queryByText('服务器地址')).not.toBeInTheDocument();
  expect(
    within(screen.getByRole('tabpanel', { name: '概览' })).getByText(
      'fixture@one.example.test:22',
      { selector: 'dd' },
    ),
  ).toBeVisible();
  expect(screen.queryByText('one', { selector: 'strong' })).not.toBeInTheDocument();
  expect(screen.queryByText('已连接')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: '查看更多' })).toBeVisible();
  expect(screen.getByText('连接已断开，监控已停止更新')).toBeVisible();
  expect(screen.getByText('12.0%')).toBeVisible();
  expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument();
});

it('restores each connection scroll position when switching the active terminal', () => {
  const view = render(<ServerToolsPanel visible />);
  const scroll = () => view.container.firstElementChild as HTMLElement;
  expect(scroll().scrollTop).toBe(0);
  fireEvent.scroll(scroll(), { target: { scrollTop: 120 } });
  fixtures.activeTab = 'two';
  view.rerender(<ServerToolsPanel visible />);
  expect(scroll().scrollTop).toBe(0);
  fireEvent.scroll(scroll(), { target: { scrollTop: 45 } });
  fixtures.activeTab = 'one';
  view.rerender(<ServerToolsPanel visible />);
  expect(scroll().scrollTop).toBe(120);
  expect(fixtures.preferences.get('two')).toBe(45);
});

it.each(['panel', 'tools-tab', 'terminal-tab'] as const)(
  'hides the borderless trigger after expansion and resets on %s changes',
  async (change) => {
    fixtures.tools.set('group-one', {
      sample: {
        hostname: 'host-details-test',
        disks: [],
        cpuPercent: null,
        memoryTotal: null,
        memoryAvailable: null,
      },
    });
    const view = render(<ServerToolsPanel visible />);
    const button = screen.getByRole('button', { name: '查看更多' });
    expect(button).toHaveAttribute('data-variant', 'ghost');
    expect(button.querySelector('svg[data-icon="inline-end"]')).toBeInTheDocument();
    await userEvent.click(button);
    expect(screen.getByText('host-details-test')).toBeVisible();
    expect(screen.queryByRole('button', { name: /查看更多|收起/ })).not.toBeInTheDocument();
    view.rerender(<ServerToolsPanel visible />);
    expect(screen.queryByRole('button', { name: '查看更多' })).not.toBeInTheDocument();
    if (change === 'panel') view.rerender(<ServerToolsPanel visible={false} />);
    if (change === 'tools-tab') {
      fixtures.toolsTab = 'transfers';
      view.rerender(<ServerToolsPanel visible />);
      fixtures.toolsTab = 'overview';
    }
    if (change === 'terminal-tab') {
      fixtures.activeTab = 'two';
      view.rerender(<ServerToolsPanel visible />);
      fixtures.activeTab = 'one';
    }
    view.rerender(<ServerToolsPanel visible />);
    expect(screen.getByRole('button', { name: '查看更多' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    expect(screen.queryByText('host-details-test')).not.toBeInTheDocument();
  },
);

it('defaults CPU and memory to bars and changes only those charts with the saved preference', () => {
  fixtures.tools.set('group-one', {
    sample: {
      cpuPercent: 25,
      memoryTotal: 100,
      memoryAvailable: 40,
      disks: [],
      networkReceive: 100,
      networkSend: 200,
    },
  });
  const view = render(<ServerToolsPanel visible />);
  expect(screen.getByRole('meter', { name: 'CPU' })).toHaveAttribute('aria-valuenow', '25');
  expect(screen.getByRole('meter', { name: '内存' })).toHaveAttribute('aria-valuenow', '60');
  const network = within(screen.getByRole('region', { name: '网络' })).getByLabelText(
    '最近 5 分钟',
  );
  fixtures.monitorDisplay = 'chart';
  view.rerender(<ServerToolsPanel visible />);
  expect(screen.queryByRole('meter', { name: 'CPU' })).not.toBeInTheDocument();
  expect(
    within(screen.getByRole('region', { name: 'CPU' })).getByLabelText('最近 5 分钟'),
  ).toBeVisible();
  expect(network).toBeInTheDocument();
  fixtures.monitorDisplay = 'bar';
  view.rerender(<ServerToolsPanel visible />);
  expect(screen.getByRole('meter', { name: 'CPU' })).toBeVisible();
});

it('keeps metric boundaries and placeholders while first samples are pending without inventing zero values', () => {
  const view = render(<ServerToolsPanel visible />);
  for (const name of ['CPU', '内存', '网络', '磁盘']) {
    expect(screen.getByRole('region', { name })).toHaveAttribute('data-slot', 'card');
  }
  const cpu = screen.getByRole('region', { name: 'CPU' });
  expect(within(cpu).getByText('采集中…')).toBeVisible();
  expect(within(cpu).queryByRole('meter')).not.toBeInTheDocument();
  expect(cpu.querySelector('[data-slot="skeleton"]')).toBeInTheDocument();
  fixtures.tools.set('group-one', {
    sample: {
      cpuPercent: 0,
      memoryTotal: 100,
      memoryAvailable: 100,
      networkReceive: 0,
      networkSend: 0,
      disks: [],
      phase: 'ready',
      collectedAt: 1000,
    },
  });
  view.rerender(<ServerToolsPanel visible />);
  expect(within(cpu).getByRole('meter')).toHaveAttribute('aria-valuenow', '0');
  expect(cpu.querySelector('[data-slot="skeleton"]')).not.toBeInTheDocument();
  fixtures.tools.set('group-one', {
    sample: { cpuPercent: null, disks: [], error: 'tools_unavailable' },
  });
  view.rerender(<ServerToolsPanel visible />);
  expect(within(cpu).queryByText('采集中…')).not.toBeInTheDocument();
  expect(cpu.querySelector('[data-slot="skeleton"]')).toHaveAttribute('aria-busy', 'false');
});

it('keeps a shared group subscribed when switching tabs or closing the original tab', () => {
  fixtures.sharedConnection = true;
  const view = render(<ServerToolsPanel visible />);
  const calls = fixtures.calls.mock.calls.length;
  fixtures.activeTab = 'two';
  view.rerender(<ServerToolsPanel visible />);
  expect(fixtures.calls).toHaveBeenCalledTimes(calls);
  fixtures.closedFirst = true;
  view.rerender(<ServerToolsPanel visible />);
  expect(fixtures.calls).toHaveBeenCalledTimes(calls);
  view.rerender(<ServerToolsPanel visible={false} />);
  expect(fixtures.calls).toHaveBeenLastCalledWith('subscribeTools', {
    groupId: 'group-one',
    visible: false,
  });
});
