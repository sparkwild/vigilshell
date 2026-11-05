// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import type { AgentSessionView } from '@contracts';
import { useEffect, type CSSProperties, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { cleanup, render, screen, fireEvent } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Workbench } from './Workbench';
const fixture = vi.hoisted(() => ({
  terminalMounts: 0,
  terminalUnmounts: 0,
  activeTab: 'overview',
  agentViews: [] as AgentSessionView[],
  attached: new Map<string, string>(),
}));
vi.mock('./DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: string) => key,
    snapshot: { preferences: { sidebarHoverPreview: true } },
  }),
}));
vi.mock('../features/connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: {
      toolsReveal: 0,
      toolsTab: 'overview',
      activeTab: fixture.activeTab,
      agentViews: fixture.agentViews,
    },
    store: { agentTerminal: (id: string) => fixture.attached.get(id) },
  }),
}));
vi.mock('../features/connections/workspace-ui', () => ({
  WorkspaceUi: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../features/terminal/TerminalTabBar', () => ({
  TerminalTabBar: () => <div data-testid="terminal-tabs" />,
}));
vi.mock('../features/mcp/AgentActivity', () => ({
  AgentActivity: ({ onBack }: { onBack?: () => void }) => (
    <>
      <input aria-label="Activity search" defaultValue="" />
      <button aria-label="activity_close" onClick={onBack} />
    </>
  ),
}));
vi.mock('../features/mcp/AgentSessionPending', () => ({
  AgentSessionPending: () => <div data-testid="pending-agent" />,
}));
vi.mock('../features/server-tools/ServerToolsPanel', () => ({ ServerToolsPanel: () => null }));
vi.mock('../features/server-tools/FloatingTransfers', () => ({ FloatingTransfers: () => null }));
vi.mock('../features/server-tools/TransferPanel', () => ({ TransferWindows: () => null }));
vi.mock('../features/server-tools/FileWorkspace', () => ({
  FileWorkspace: () => {
    useEffect(() => {
      fixture.terminalMounts++;
      return () => {
        fixture.terminalUnmounts++;
      };
    }, []);
    return <div data-testid="terminal-surface" />;
  },
}));
vi.mock('./WorkbenchNavigation', () => ({
  WorkbenchNavigation: () => <span>Connection navigation</span>,
}));
vi.mock('../components/ui/navigation-interaction', () => ({
  useNavigationInteractionHeld: () => false,
}));
vi.mock('../components/ui/sidebar', () => ({
  SidebarProvider: ({ children, style }: { children: ReactNode; style: CSSProperties }) => (
    <div data-testid="workbench" style={style}>
      {children}
    </div>
  ),
}));
vi.mock('../components/ui/resizable', () => ({
  ResizablePanelGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null,
}));
// Model a portal retained during its exit animation, independently of the old page.
vi.mock('../components/ui/popover', () => ({
  Popover: ({ children }: { children: ReactNode }) => children,
  PopoverTrigger: () => null,
  PopoverContent: ({ children, style }: { children: ReactNode; style: CSSProperties }) =>
    createPortal(
      <div data-testid="retained-preview" style={style}>
        {children}
      </div>,
      document.body,
    ),
}));
vi.mock('../components/ui/icon-button', () => ({
  IconButton: ({ label, onClick }: { label: string; onClick?: () => void }) => (
    <button aria-label={label} onClick={onClick} />
  ),
}));
vi.mock('../components/ui/tooltip', () => ({
  Tooltip: ({ children }: { children: ReactNode }) => children,
  TooltipTrigger: () => null,
  TooltipContent: () => null,
}));
beforeEach(() => {
  fixture.activeTab = 'overview';
  fixture.agentViews = [];
  fixture.attached.clear();
  fixture.terminalMounts = 0;
  fixture.terminalUnmounts = 0;
});
afterEach(cleanup);
it('hides both navigation surfaces immediately without remounting the terminal surface', () => {
  const view = render(<Workbench active onSettings={() => {}} />);
  const terminal = screen.getByTestId('terminal-surface');
  const sidebar = document.getElementById('workbench-navigation')!;
  const preview = screen.getByTestId('retained-preview');
  expect(sidebar.style.display).not.toBe('none');
  view.rerender(<Workbench active={false} onSettings={() => {}} />);
  expect(sidebar).toHaveStyle({ display: 'none' });
  expect(preview).toHaveStyle({ display: 'none' });
  expect(screen.getByTestId('terminal-surface')).toBe(terminal);
  expect(screen.getByTestId('workbench')).toHaveStyle({
    visibility: 'hidden',
    position: 'absolute',
  });
  expect(fixture.terminalMounts).toBe(1);
  expect(fixture.terminalUnmounts).toBe(0);
  view.rerender(<Workbench active onSettings={() => {}} />);
  expect(sidebar.style.display).not.toBe('none');
  expect(preview.style.display).not.toBe('none');
  expect(screen.getByTestId('terminal-surface')).toBe(terminal);
  expect(fixture.terminalMounts).toBe(1);
});

it('keeps navigation and terminal mounted while activity occupies the center', () => {
  const view = render(<Workbench active onSettings={() => {}} />);
  const terminal = screen.getByTestId('terminal-surface');
  view.rerender(<Workbench active onSettings={() => {}} activityTarget={{ view: 'activity' }} />);
  expect(document.getElementById('workbench-navigation')).toBeVisible();
  expect(screen.getByTestId('terminal-surface')).toBe(terminal);
  expect(terminal.closest('[hidden]')).not.toBeNull();
  expect(fixture.terminalUnmounts).toBe(0);
});

it('closes activity without closing sessions or losing its input state', () => {
  const close = vi.fn();
  const props = { active: true, onSettings: () => {}, onReturnToTerminal: close };
  const view = render(<Workbench {...props} />);
  const terminal = screen.getByTestId('terminal-surface');
  expect(screen.getByTestId('terminal-tabs')).toBeInTheDocument();
  view.rerender(<Workbench {...props} activityTarget={{ view: 'activity' }} />);
  expect(screen.queryByTestId('terminal-tabs')).not.toBeInTheDocument();
  const input = screen.getByRole('textbox', { name: 'Activity search' });
  fireEvent.change(input, { target: { value: 'retained query' } });
  fireEvent.click(screen.getByRole('button', { name: 'activity_close' }));
  expect(close).toHaveBeenCalledOnce();
  view.rerender(<Workbench {...props} />);
  expect(screen.getByTestId('terminal-tabs')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'activity_close' })).not.toBeInTheDocument();
  expect(screen.getByTestId('terminal-surface')).toBe(terminal);
  expect(fixture.terminalMounts).toBe(1);
  expect(fixture.terminalUnmounts).toBe(0);
  view.rerender(<Workbench {...props} activityTarget={{ view: 'activity' }} />);
  expect(screen.getByRole('textbox', { name: 'Activity search' })).toHaveValue('retained query');
});

it('keeps terminal mounts while policy occupies the center and restores tabs afterwards', () => {
  const props = { active: true, onSettings: () => {} };
  const view = render(<Workbench {...props} />);
  const terminal = screen.getByTestId('terminal-surface');
  view.rerender(<Workbench {...props} policyContent={<section>Policy cards</section>} />);
  expect(document.getElementById('workbench-navigation')).toBeVisible();
  expect(screen.getByText('Policy cards')).toBeVisible();
  expect(screen.queryByTestId('terminal-tabs')).not.toBeInTheDocument();
  expect(terminal.closest('[hidden]')).not.toBeNull();
  view.rerender(<Workbench {...props} />);
  expect(screen.getByTestId('terminal-surface')).toBe(terminal);
  expect(screen.getByTestId('terminal-tabs')).toBeInTheDocument();
  expect(fixture.terminalMounts).toBe(1);
  expect(fixture.terminalUnmounts).toBe(0);
});

it('routes a new agent from authorization into the ordinary workspace without mounting the legacy workbench', () => {
  fixture.activeTab = 'agent:new';
  fixture.agentViews = [
    {
      runningTasks: 0,
      context: { id: 'new', phase: 'waitingUser' },
    } as AgentSessionView,
  ];
  const view = render(<Workbench active onSettings={() => {}} />);
  expect(screen.getByTestId('pending-agent')).toBeVisible();
  expect(screen.queryByTestId('legacy-workbench')).not.toBeInTheDocument();
  fixture.attached.set('new', 'ordinary-terminal');
  fixture.activeTab = 'ordinary-terminal';
  view.rerender(<Workbench active onSettings={() => {}} />);
  expect(screen.queryByTestId('pending-agent')).not.toBeInTheDocument();
  expect(screen.queryByTestId('legacy-workbench')).not.toBeInTheDocument();
  expect(screen.getByTestId('terminal-surface')).toBeVisible();
  expect(fixture.terminalMounts).toBe(1);
});

it.each([
  ['failed', 'authorization_expired'],
  ['failed', 'connection_failed'],
  ['disconnected', 'runtime_unavailable'],
  ['closed', null],
] as const)('keeps the unified notice when a new session becomes %s (%s)', (phase, error) => {
  fixture.activeTab = 'agent:new';
  fixture.agentViews = [
    {
      runningTasks: 0,
      context: { id: 'new', phase: 'waitingUser' },
    } as AgentSessionView,
  ];
  const view = render(<Workbench active onSettings={() => {}} />);
  const pending = screen.getByTestId('pending-agent');
  fixture.agentViews = [
    { ...fixture.agentViews[0]!, context: { ...fixture.agentViews[0]!.context, phase, error } },
  ];
  view.rerender(<Workbench active onSettings={() => {}} />);
  expect(screen.getByTestId('pending-agent')).toBe(pending);
  expect(pending).toBeVisible();
  expect(screen.queryByTestId('legacy-workbench')).not.toBeInTheDocument();
  expect(fixture.terminalUnmounts).toBe(0);
});
