// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { useState, type ReactNode } from 'react';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { AgentSnapshot, ConnectionSummary } from '@contracts';
import { translate } from '../../../../shared/i18n/messages';
import { AgentPolicyPage } from './AgentPolicyPage';
import { defaultPolicySettings } from '../model';
import { initialPolicySession } from './policy-model';
const mocks = vi.hoisted(() => ({
  decide: vi.fn(),
  request: vi.fn(),
  connections: [] as ConnectionSummary[],
}));
// JSDOM 无命中几何，分隔线会误捕获所有指针；拖动尺寸在真实 Dev 中验收。
vi.mock('../../../components/ui/resizable', () => ({
  ResizablePanelGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null,
}));
vi.mock('../../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Parameters<typeof translate>[2]) =>
      translate(key, 'en', values),
  }),
}));
vi.mock('../../../shared/ConfirmationProvider', () => ({ useDecision: () => mocks.decide }));
vi.mock('../../connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: { ready: true, loading: false, error: null, connections: mocks.connections },
    store: { refresh: vi.fn() },
  }),
}));
const target = { connectionId: null, version: 0 };
function Harness({ route = target }: { route?: { connectionId: string | null; version: number } }) {
  const [session, setSession] = useState(initialPolicySession);
  return (
    <AgentPolicyPage
      target={route}
      session={session}
      setSession={setSession}
      onBack={() => {}}
      register={() => {}}
    />
  );
}
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi
    .fn()
    .mockReturnValue({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  mocks.connections = [
    {
      id: 'one',
      name: 'Dev host',
      host: 'dev.example',
      environment: 'development',
      revision: 1,
      deleting: false,
    },
    {
      id: 'two',
      name: 'Test host',
      host: 'test.example',
      environment: 'test',
      revision: 1,
      deleting: false,
    },
  ] as ConnectionSummary[];
  const snapshot: AgentSnapshot = {
    settings: {
      revision: 0,
      enabled: false,
      port: 51602,
      showTray: true,
      retentionDays: 30,
      storageMib: 256,
    },
    entries: [],
    policies: [],
    contexts: [],
    views: [],
    clients: [],
    observations: [],
    endpoint: 'http://127.0.0.1:51602/mcp',
    running: false,
    error: null,
  };
  mocks.request
    .mockReset()
    .mockImplementation(async (request) =>
      request.action === 'tasks'
        ? { ok: true, reply: { kind: 'tasks', tasks: [] } }
        : { ok: true, snapshot },
    );
  mocks.decide.mockReset().mockResolvedValue('continue');
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: { mcp: { request: mocks.request, onChanged: () => () => {} } },
  });
});
afterEach(cleanup);

it('opens a card overview without starting a session', async () => {
  render(<Harness />);
  expect(await screen.findAllByRole('button', { name: 'Configure' })).toHaveLength(2);
  expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'More information' })).not.toBeInTheDocument();
  expect(screen.queryByText('Access disabled')).not.toBeInTheDocument();
  expect(screen.getAllByRole('switch', { name: 'Allow access' })).toHaveLength(2);
  expect(mocks.request.mock.calls.every(([r]) => ['snapshot', 'tasks'].includes(r.action))).toBe(
    true,
  );
});
it('guards incomplete path input while switching and discards only on explicit choice', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click((await screen.findAllByRole('button', { name: 'Configure' }))[0]!);
  await screen.findByRole('combobox', { name: 'Operation level' });
  await user.click(screen.getByRole('switch', { name: 'Restrict local transfer directory' }));
  await user.type(screen.getByRole('textbox', { name: 'Local transfer directory' }), 'relative');
  await user.click(screen.getByRole('button', { name: 'Back to permissions' }));
  expect(screen.getByRole('combobox', { name: 'Operation level' })).toBeInTheDocument();
  mocks.decide.mockResolvedValue('discard');
  await user.click(screen.getByRole('button', { name: 'Back to permissions' }));
  await screen.findAllByRole('button', { name: 'Configure' });
  expect(mocks.request.mock.calls.every(([r]) => ['snapshot', 'tasks'].includes(r.action))).toBe(
    true,
  );
});
it('never substitutes another connection for a missing explicit target', async () => {
  render(<Harness route={{ connectionId: 'missing', version: 1 }} />);
  await waitFor(() => expect(screen.queryByText('Loading connections')).not.toBeInTheDocument());
  expect(screen.queryByRole('switch')).not.toBeInTheDocument();
});

it('chooses an additional protected path without granting operation access', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click((await screen.findAllByRole('button', { name: 'Configure' }))[0]!);
  await screen.findByRole('combobox', { name: 'Operation level' });
  const prior = mocks.request.getMockImplementation()!;
  mocks.request.mockImplementation(async (request) => {
    if (request.action === 'openPolicyBrowser')
      return { ok: true, reply: { kind: 'policyBrowser', id: 'browser' } };
    if (request.action === 'readPolicyResources')
      return {
        ok: true,
        reply: {
          kind: 'policyResources',
          page: {
            kind: 'directory',
            path: '/',
            page: {
              entries: [
                {
                  name: 'srv',
                  path: '/srv',
                  kind: 'directory',
                  size: 0,
                  mode: 493,
                  uid: 0,
                  gid: 0,
                },
              ],
              nextCursor: null,
            },
          },
        },
      };
    if (request.action === 'closePolicyBrowser') return { ok: true };
    if (request.action === 'updatePolicySettings') {
      const result = await prior(request);
      const base = defaultPolicySettings('one');
      return {
        ...result,
        snapshot: {
          ...result.snapshot,
          policies: [
            {
              ...base,
              connectionRevision: 1,
              revision: 1,
              execution: { ...base.execution, protectedPaths: request.patch.value },
            },
          ],
        },
      };
    }
    return prior(request);
  });
  await user.click(
    screen.getByRole('button', { name: translate('policy_choose_server_paths', 'en') }),
  );
  const dialog = await screen.findByRole('dialog');
  await user.click(await within(dialog).findByRole('checkbox', { name: 'srv' }));
  await user.click(
    within(dialog).getByRole('button', { name: translate('policy_picker_save', 'en') }),
  );
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('textbox', { name: 'Directory path' })).toHaveValue('/srv');
  expect(screen.getByRole('combobox', { name: 'Operation level' })).toHaveTextContent('Read only');
  expect(
    mocks.request.mock.calls.every(([r]) =>
      [
        'snapshot',
        'openPolicyBrowser',
        'readPolicyResources',
        'closePolicyBrowser',
        'updatePolicySettings',
        'tasks',
      ].includes(r.action),
    ),
  ).toBe(true);
});

it('opens protection details without saving policy and returns focus on close', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click((await screen.findAllByRole('button', { name: 'Configure' }))[0]!);
  const trigger = await screen.findByRole('button', { name: 'System file protection' });
  await user.click(trigger);
  const dialog = await screen.findByRole('dialog', { name: 'System file protection' });
  expect(
    within(dialog).getByText('System protection details are not available yet'),
  ).toBeInTheDocument();
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(
    mocks.request.mock.calls.every(([request]) => ['snapshot', 'tasks'].includes(request.action)),
  ).toBe(true);
});

it('offers the shared empty state and clears search without changing the selected connection', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await screen.findAllByRole('button', { name: 'Configure' });
  const search = screen.getByRole('textbox', { name: 'Search connection name or address' });
  await user.type(search, 'no-such-connection');
  expect(screen.getByText('No matching connections').closest('[data-slot="empty"]')).not.toBeNull();
  await user.click(screen.getByRole('button', { name: 'Clear search' }));
  expect(search).toHaveValue('');
  expect(screen.getAllByRole('button', { name: 'Configure' })).toHaveLength(2);
  expect(
    mocks.request.mock.calls.every(([request]) => ['snapshot', 'tasks'].includes(request.action)),
  ).toBe(true);
});

it('restores the overview scroll position and configure focus', async () => {
  const user = userEvent.setup();
  const { container } = render(<Harness />);
  const buttons = await screen.findAllByRole('button', { name: 'Configure' });
  const scroll = container.querySelector('.overflow-auto') as HTMLElement;
  scroll.scrollTop = 130;
  await user.click(buttons[0]!);
  await screen.findByRole('combobox', { name: 'Operation level' });
  await user.click(screen.getByRole('button', { name: 'Back to permissions' }));
  await waitFor(() =>
    expect(screen.getAllByRole('button', { name: 'Configure' })[0]).toHaveFocus(),
  );
  expect((container.querySelector('.overflow-auto') as HTMLElement).scrollTop).toBe(130);
});
it('opens configuration on card double click but never on the switch', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  await screen.findAllByRole('button', { name: 'Configure' });
  await user.dblClick(screen.getByText('Dev host'));
  expect(await screen.findByRole('combobox', { name: 'Operation level' })).toBeInTheDocument();
});
it.each([
  'authorization_cancelled',
  'authorization_failed',
  'authorization_timed_out',
  'authorization_interrupted',
  'configuration_conflict',
])('serializes access changes without claiming success: %s', async (code) => {
  const user = userEvent.setup();
  render(<Harness />);
  await screen.findAllByRole('button', { name: 'Configure' });
  const prior = mocks.request.getMockImplementation()!;
  let finish!: (v: unknown) => void;
  mocks.request.mockImplementation((request) =>
    request.action === 'setPolicyAccess'
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : prior(request),
  );
  const switches = screen.getAllByRole('switch', { name: 'Allow access' });
  await user.click(screen.getAllByText('Allow access')[0]!);
  expect(switches[0]).not.toBeChecked();
  expect(switches[1]).toHaveAttribute('aria-disabled', 'true');
  expect(mocks.request).toHaveBeenCalledWith(
    expect.objectContaining({
      action: 'setPolicyAccess',
      connectionId: 'one',
      connectionRevision: 1,
      expectedRevision: 0,
      enabled: true,
    }),
  );
  await act(async () => finish({ ok: false, code }));
  await waitFor(() =>
    expect(screen.getAllByRole('button', { name: 'Configure' })[0]).not.toBeDisabled(),
  );
  expect(screen.getAllByRole('switch', { name: 'Allow access' })[0]).not.toBeChecked();
  expect(mocks.request.mock.calls.filter(([r]) => r.action === 'setPolicyAccess')).toHaveLength(1);
  if (code === 'configuration_conflict') expect(screen.getByRole('alert')).toBeInTheDocument();
  else expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
