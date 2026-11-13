import type { NotificationTarget } from '@contracts';
import { notificationApiFixture } from '../../../tests/fixtures/notifications';
// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { useEffect, type ReactNode } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { McpNavigation } from '../../contracts/mcp';
import type { LeaveGuard, RegisterLeaveGuard } from '../features/mcp/SettingsParts';
import { navigateMcp } from '../features/mcp/model';
import { App } from './App';
const state = vi.hoisted(() => ({
  guard: vi.fn(),
  mounts: 0,
  newConnection: vi.fn(),
  diagnostics: vi.fn(),
  tab: vi.fn(),
  navigate: null as ((v: McpNavigation) => void) | null,
  settings: null as ((section?: 'notifications' | 'about' | 'updates') => void) | null,
  command: null as ((v: string) => void) | null,
  notification: null as ((v: NotificationTarget) => void) | null,
}));
vi.mock('./DesktopProvider', () => ({ useDesktop: () => ({ t: (key: string) => key }) }));
vi.mock('../features/connections/workspace-context', () => ({
  useWorkspace: () => ({
    store: {
      getSnapshot: () => ({
        editor: null,
        sessions: [{ id: 'failed-session' }],
        agentViews: [{ context: { id: 'session' } }],
        transferWindows: [],
        operations: new Map(),
        transfers: new Map(),
        tunnelStates: new Map(),
      }),
      tab: state.tab,
      newConnection: state.newConnection,
      showTerminalDiagnostics: state.diagnostics,
    },
  }),
}));
vi.mock('../shared/UiProvider', () => ({
  UiProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../shared/ConfirmationProvider', () => ({
  ConfirmationProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../components/ui/navigation-interaction', () => ({
  NavigationInteractionProvider: ({ children }: { children: ReactNode }) => children,
}));
vi.mock('../features/server-tools/FileEditorsDialog', () => ({ FileEditorsDialog: () => null }));
vi.mock('../features/server-tools/TransferConflictDialog', () => ({
  TransferConflictDialog: () => null,
}));
vi.mock('../features/mcp/SessionAuthorization', () => ({ SessionAuthorization: () => null }));
vi.mock('./Workbench', () => ({
  Workbench: ({
    active,
    onSettings,
    policyContent,
    activityTarget,
    beforeConnectionAction,
  }: {
    active: boolean;
    onSettings(): void;
    policyContent?: ReactNode;
    activityTarget?: Extract<McpNavigation, { view: 'activity' }>;
    beforeConnectionAction?: () => Promise<boolean>;
  }) => {
    useEffect(() => {
      state.mounts++;
    }, []);
    return (
      <div hidden={!active}>
        <h1>Workbench</h1>
        {policyContent}
        {activityTarget ? (
          <div>
            <h1>Activity page</h1>
            <span>{activityTarget.taskId}</span>
          </div>
        ) : null}
        <button onClick={() => navigateMcp({ view: 'policy', connectionId: null })}>
          Policies
        </button>
        <button onClick={() => navigateMcp({ view: 'activity' })}>Activity</button>
        <button onClick={onSettings}>Settings</button>
        <button
          onClick={async () => {
            if (await beforeConnectionAction?.()) state.newConnection();
          }}
        >
          Connect from sidebar
        </button>
      </div>
    );
  },
}));
vi.mock('../features/mcp/policy/AgentPolicyPage', () => ({
  AgentPolicyPage: ({
    target,
    register,
    onBack,
  }: {
    target: { connectionId: string | null };
    register: RegisterLeaveGuard;
    onBack(): void;
  }) => {
    useEffect(() => {
      register(state.guard as LeaveGuard);
      return () => register(null);
    }, [register]);
    return (
      <div>
        <h1>Policy page</h1>
        <span>{target.connectionId ?? 'all'}</span>
        <button onClick={onBack}>Back from policy</button>
      </div>
    );
  },
}));
vi.mock('../features/mcp/AgentActivity', () => ({
  AgentActivity: ({
    target,
    onBack,
  }: {
    target: Extract<McpNavigation, { view: 'activity' }>;
    onBack(): void;
  }) => (
    <div>
      <h1>Activity page</h1>
      <span>{target.taskId}</span>
      <button onClick={onBack}>Back from activity</button>
    </div>
  ),
}));
vi.mock('../features/settings/SettingsView', () => ({
  SettingsView: ({ onBack, initialCategory }: { onBack(): void; initialCategory: string }) => (
    <div>
      <h1>Settings page</h1>
      <output aria-label="Settings route">{initialCategory}</output>
      <button onClick={onBack}>Back from settings</button>
    </div>
  ),
}));
afterEach(cleanup);
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  state.guard.mockReset().mockResolvedValue(true);
  state.newConnection.mockReset();
  state.diagnostics.mockReset();
  state.tab.mockReset();
  state.mounts = 0;
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      rendererReady: vi.fn(),
      application: {
        info: vi.fn(async () => ({ name: 'VigilShell', version: '0.1.2' })),
        check: vi.fn(async () => ({ phase: 'current', release: null })),
      },
      notifications: {
        ...notificationApiFixture(),
        onNavigate: (receive: typeof state.notification) => {
          state.notification = receive;
          return () => {
            state.notification = null;
          };
        },
      },
      mcp: {
        request: vi.fn(async () => ({
          ok: true,
          reply: { kind: 'task', task: { id: 'notification-task', contextId: 'session' } },
        })),
        onNavigate: (receive: typeof state.navigate) => {
          state.navigate = receive;
          return () => {
            state.navigate = null;
          };
        },
      },
      onOpenSettings: (receive: typeof state.settings) => {
        state.settings = receive;
        return () => {
          state.settings = null;
        };
      },
      onWorkspaceCommand: (receive: typeof state.command) => {
        state.command = receive;
        return () => {
          state.command = null;
        };
      },
    },
  });
});
it('routes policies separately from settings and preserves the mounted workbench on return', async () => {
  const user = userEvent.setup();
  render(<App fontFailed={false} />);
  await user.click(screen.getByRole('button', { name: 'Policies' }));
  await screen.findByRole('heading', { name: 'Policy page' });
  expect(screen.queryByRole('heading', { name: 'Settings page' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Back from policy' }));
  await screen.findByRole('heading', { name: 'Workbench' });
  expect(state.mounts).toBe(1);
  await act(async () => {
    state.navigate!({ view: 'policy', connectionId: 'server-two' });
  });
  expect(screen.getByText('server-two')).toBeInTheDocument();
});
it('native routes, settings, keyboard routes all respect the policy leave guard', async () => {
  const user = userEvent.setup();
  render(<App fontFailed={false} />);
  await user.click(screen.getByRole('button', { name: 'Policies' }));
  await screen.findByRole('heading', { name: 'Policy page' });
  state.guard.mockResolvedValue(false);
  await act(async () => {
    state.navigate!({ view: 'policy', connectionId: 'other' });
  });
  await act(async () => {
    state.settings!();
  });
  await act(async () => {
    state.command!('close');
  });
  await act(async () => {
    state.command!('new');
  });
  expect(screen.getByRole('heading', { name: 'Policy page' })).toBeInTheDocument();
  expect(screen.queryByText('other')).toBeNull();
  expect(state.newConnection).not.toHaveBeenCalled();
  state.guard.mockResolvedValue(true);
  await act(async () => {
    state.navigate!({ view: 'activity', taskId: 'task-one', contextId: 'context-one' });
  });
  await waitFor(() =>
    expect(screen.getByRole('heading', { name: 'Activity page' })).toBeInTheDocument(),
  );
  expect(screen.getByText('task-one')).toBeInTheDocument();
  expect(state.mounts).toBe(1);
});

it('checks policy departure before a sidebar connection side effect', async () => {
  const user = userEvent.setup();
  render(<App fontFailed={false} />);
  await user.click(screen.getByRole('button', { name: 'Policies' }));
  await screen.findByRole('heading', { name: 'Policy page' });
  state.guard.mockResolvedValue(false);
  await user.click(screen.getByRole('button', { name: 'Connect from sidebar' }));
  expect(state.newConnection).not.toHaveBeenCalled();
  expect(screen.getByRole('heading', { name: 'Policy page' })).toBeInTheDocument();
  state.guard.mockResolvedValue(true);
  await user.click(screen.getByRole('button', { name: 'Connect from sidebar' }));
  await waitFor(() => expect(state.newConnection).toHaveBeenCalledOnce());
  expect(screen.queryByRole('heading', { name: 'Policy page' })).not.toBeInTheDocument();
});

it('routes a system task notification through the leave guard exactly once', async () => {
  const user = userEvent.setup();
  render(<App fontFailed={false} />);
  await user.click(screen.getByRole('button', { name: 'Policies' }));
  await screen.findByRole('heading', { name: 'Policy page' });
  const target: NotificationTarget = {
    kind: 'agentTask',
    contextId: 'session',
    taskId: 'notification-task',
    review: false,
  };
  state.guard.mockClear().mockResolvedValue(false);
  await act(async () => state.notification?.(target));
  expect(state.guard).toHaveBeenCalledOnce();
  expect(screen.getByRole('heading', { name: 'Policy page' })).toBeVisible();
  state.guard.mockClear().mockResolvedValue(true);
  await act(async () => state.notification?.(target));
  await screen.findByRole('heading', { name: 'Activity page' });
  expect(screen.getByText('notification-task')).toBeVisible();
  expect(state.guard).toHaveBeenCalledOnce();
});

it('guards local connection diagnostics before changing the workspace', async () => {
  render(<App fontFailed={false} />);
  await userEvent.click(screen.getByRole('button', { name: 'Policies' }));
  state.guard.mockResolvedValue(false);
  await act(async () => {
    window.dispatchEvent(
      new CustomEvent('vigilshell:terminal-diagnostics', { detail: 'failed-session' }),
    );
  });
  expect(state.diagnostics).not.toHaveBeenCalled();
  expect(screen.getByRole('heading', { name: 'Policy page' })).toBeInTheDocument();
  state.guard.mockResolvedValue(true);
  await act(async () => {
    window.dispatchEvent(
      new CustomEvent('vigilshell:terminal-diagnostics', { detail: 'failed-session' }),
    );
  });
  expect(state.diagnostics).toHaveBeenCalledExactlyOnceWith('failed-session');
  expect(screen.queryByRole('heading', { name: 'Policy page' })).toBeNull();
});

it('returns to the homepage for a notification whose terminal is gone without opening a fallback', async () => {
  render(<App fontFailed={false} />);
  await userEvent.click(screen.getByRole('button', { name: 'Policies' }));
  await act(async () =>
    state.notification?.({
      kind: 'connection',
      runtimeId: 'retired',
      connectionId: 'removed',
      sessionId: 'closed',
      tunnelId: null,
    }),
  );
  expect(state.tab).toHaveBeenCalledWith('overview');
  expect(screen.queryByRole('heading', { name: 'Policy page' })).toBeNull();
  expect(state.newConnection).not.toHaveBeenCalled();
});

it('opens menu updates over the current page without invoking its leave guard or remounting the workspace', async () => {
  render(<App fontFailed={false} />);
  await userEvent.click(screen.getByRole('button', { name: 'Policies' }));
  state.guard.mockClear().mockResolvedValue(false);
  await act(async () => state.settings?.('updates'));
  expect(screen.getByRole('dialog')).toBeVisible();
  expect(screen.queryByLabelText('Settings route')).toBeNull();
  expect(state.guard).not.toHaveBeenCalled();
  expect(state.mounts).toBe(1);
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('heading', { name: 'Policy page' })).toBeVisible();
  state.guard.mockResolvedValue(true);
  await act(async () => state.settings?.('about'));
  expect(screen.getByLabelText('Settings route')).toHaveTextContent(/^about$/);
});
