// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { FileWorkspace } from './FileWorkspace';
import type { AgentTask } from '@contracts';
const fixture = vi.hoisted(() => ({
  autoOpen: true,
  agentTasks: [] as AgentTask[],
  session: { id: 's', connectionId: 'c', groupId: 'g', phase: 'running' },
}));
vi.mock('./model/use-agent-file-tasks', () => ({
  useAgentFileTasks: () => ({ tasks: fixture.agentTasks, error: null }),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: string) => key,
    snapshot: { preferences: { filesAutoOpen: fixture.autoOpen } },
  }),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    store: { terminals: new Map() },
    state: {
      transportReady: true,
      ready: true,
      activeTab: 's',
      runtimeId: 'runtime',
      sessions: [fixture.session],
      transfers: new Map(),
    },
  }),
}));
vi.mock('../terminal/terminal-workspace', () => ({
  TerminalWorkspace: () => <div>terminal</div>,
}));
vi.mock('./file-browser', () => ({
  FileBrowser: ({ panelActions }: { panelActions: React.ReactNode }) => (
    <div data-testid="files">files{panelActions}</div>
  ),
}));
vi.mock('./directory-service', () => ({
  directories: () => ({ prune: () => {} }),
}));
vi.mock('../../components/ui/resizable', () => ({
  ResizablePanelGroup: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null,
}));
beforeEach(() => {
  fixture.autoOpen = true;
  fixture.agentTasks = [];
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
it('opens a collapsed file area for a new file task and preserves the manual collapse during status updates', async () => {
  fixture.autoOpen = false;
  const view = render(<FileWorkspace active />);
  expect(screen.queryByTestId('files')).not.toBeInTheDocument();
  fixture.agentTasks = [{ id: 'file' } as AgentTask];
  view.rerender(<FileWorkspace active />);
  expect(await screen.findByTestId('files')).toBeVisible();
  await userEvent.click(screen.getByRole('button', { name: 'files_collapse' }));
  fixture.agentTasks = [{ id: 'file', phase: 'succeeded' } as AgentTask];
  view.rerender(<FileWorkspace active />);
  expect(screen.queryByTestId('files')).not.toBeInTheDocument();
  fixture.agentTasks = [{ id: 'next' } as AgentTask];
  view.rerender(<FileWorkspace active />);
  expect(await screen.findByTestId('files')).toBeVisible();
});
afterEach(cleanup);
it('opens on the first successful connection when enabled', async () => {
  render(<FileWorkspace active />);
  expect(await screen.findByTestId('files')).toBeVisible();
  expect(screen.getByRole('button', { name: 'files_collapse' })).toHaveAttribute(
    'aria-expanded',
    'true',
  );
});
it('shows existing file feedback on mount even when files normally start collapsed', async () => {
  fixture.autoOpen = false;
  fixture.agentTasks = [{ id: 'file', createdAt: 1 } as AgentTask];
  render(<FileWorkspace active />);
  expect(await screen.findByTestId('files')).toBeVisible();
});
it('starts collapsed when disabled and preserves an explicit manual opening', async () => {
  fixture.autoOpen = false;
  const view = render(<FileWorkspace active />);
  expect(screen.queryByTestId('files')).not.toBeInTheDocument();
  const user = userEvent.setup();
  const expand = screen.getByRole('button', { name: 'files_expand' });
  expect(expand).toHaveAttribute('aria-expanded', 'false');
  await user.click(expand);
  expect(await screen.findByTestId('files')).toBeVisible();
  const collapse = screen.getByRole('button', { name: 'files_collapse' });
  expect(collapse).toHaveFocus();
  fixture.autoOpen = true;
  view.rerender(<FileWorkspace active />);
  fixture.autoOpen = false;
  view.rerender(<FileWorkspace active />);
  await waitFor(() => expect(screen.getByTestId('files')).toBeVisible());
  await user.click(screen.getByRole('button', { name: 'files_maximize' }));
  expect(screen.getByRole('button', { name: 'files_restore' })).toBeInTheDocument();
  collapse.focus();
  await user.keyboard('{Enter}');
  expect(screen.queryByTestId('files')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'files_expand' })).toHaveFocus();
  await user.keyboard(' ');
  expect(await screen.findByTestId('files')).toBeVisible();
  expect(screen.getByRole('button', { name: 'files_collapse' })).toHaveFocus();
  expect(screen.getByRole('button', { name: 'files_maximize' })).toBeInTheDocument();
});
