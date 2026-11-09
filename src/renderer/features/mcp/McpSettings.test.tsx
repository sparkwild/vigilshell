// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { AgentSnapshot, AgentUiRequest, AgentTask, RecordedSession } from '@contracts';
import { translate } from '../../../shared/i18n/messages';
import { ConfirmationProvider } from '../../shared/ConfirmationProvider';
import { McpSettings } from './McpSettings';
import { AgentActivitySettings } from './AgentActivitySettings';
import { AgentActivity } from './AgentActivity';
const state = vi.hoisted(() => ({
  snapshot: null as AgentSnapshot | null,
  issue: vi.fn(),
  refresh: vi.fn(),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    snapshot: { appearance: { locale: 'en' } },
    t: (key: Parameters<typeof translate>[0], values?: Parameters<typeof translate>[2]) =>
      translate(key, 'en', values),
  }),
}));
vi.mock('./model', () => ({
  useMcp: () => ({
    snapshot: state.snapshot,
    issue: state.issue,
    refresh: state.refresh,
    busy: false,
    error: null,
  }),
}));
afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
});
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
  state.snapshot = {
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
    clients: [
      { client: 'codex', installed: true, configurationFound: true, unavailable: false },
      { client: 'cursor', installed: false, configurationFound: true, unavailable: false },
      { client: 'claudeCode', installed: false, configurationFound: false, unavailable: false },
    ],
    observations: [],
    endpoint: 'http://127.0.0.1:51602/mcp',
    running: false,
    error: null,
  };
  state.issue
    .mockReset()
    .mockImplementation(async (request: AgentUiRequest, receive?: (s: AgentSnapshot) => void) => {
      if (request.action === 'prepareAccess') {
        state.snapshot!.entries.push({
          id: 'created',
          name: request.name,
          client: request.client,
          enabled: true,
          generation: 1,
        });
        state.snapshot!.settings.enabled = true;
      }
      receive?.(state.snapshot!);
      return true;
    });
});
function view() {
  return render(
    <ConfirmationProvider>
      <McpSettings footer={null} />
    </ConfirmationProvider>,
  );
}
async function addConnection(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Add connection' }));
  await user.click(await screen.findByRole('menuitem', { name: 'Generic Agent' }));
  const dialog = await screen.findByRole('dialog');
  state.issue.mockClear();
  return dialog;
}
it.each([
  ['0', '0', 'vigilshell'],
  ['1', '0', 'vigilshell-dev'],
  ['0', '1', 'vigilshell-dev'],
])(
  'previews the channel name with development=%s and package=%s',
  async (development, packaged, name) => {
    vi.stubEnv('VIGILSHELL_DEVELOPMENT_BUILD', development);
    vi.stubEnv('VIGILSHELL_DEV_PACKAGE_BUILD', packaged);
    const user = userEvent.setup();
    view();
    const dialog = await addConnection(user);
    expect(within(dialog).getByLabelText('Setup content')).toHaveTextContent(`"${name}":`);
    await user.click(within(dialog).getByRole('button', { name: 'Manual configuration' }));
    const configuration = JSON.parse(dialog.querySelector('pre')!.textContent!);
    expect(Object.keys(configuration.mcpServers)).toEqual([name]);
    expect(state.issue).not.toHaveBeenCalled();
  },
);
it('connection setup keeps configuration focused after generic selection', async () => {
  const user = userEvent.setup();
  view();
  expect(
    screen
      .getByText('Local service')
      .compareDocumentPosition(screen.getByText('Agent connection configuration')) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(screen.getByText('No connections yet')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Copy instructions' })).not.toBeInTheDocument();
  expect(state.issue).not.toHaveBeenCalled();
  await addConnection(user);
  expect(state.issue).not.toHaveBeenCalled();
  expect(
    screen.queryByRole('button', { name: 'Choose a client (optional)' }),
  ).not.toBeInTheDocument();
  expect(screen.getByLabelText('Setup content')).toHaveTextContent(
    'Filled with this entry’s token when copied',
  );
});
it('connection setup creates only on explicit copy and reuses the connection for both methods and reopening', async () => {
  const user = userEvent.setup();
  view();
  const dialog = await addConnection(user);
  expect(within(dialog).getByText('Copying also enables the MCP service')).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Add and copy' }));
  expect(state.issue.mock.calls[0]![0]).toMatchObject({
    action: 'prepareAccess',
    client: 'generic',
    delivery: 'prompt',
  });
  expect(within(dialog).getByRole('status')).toHaveTextContent(
    'Instructions copied. Paste them into your Agent to complete setup',
  );
  expect(within(dialog).queryByRole('textbox')).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Copy again' }));
  expect(state.issue.mock.calls[1]![0]).toMatchObject({
    action: 'deliverAccess',
    id: 'created',
    delivery: 'prompt',
  });
  await user.click(within(dialog).getByRole('button', { name: 'View configuration' }));
  await user.click(within(dialog).getByRole('button', { name: 'Manual configuration' }));
  await user.click(within(dialog).getByRole('button', { name: 'Copy configuration' }));
  expect(state.issue.mock.calls[2]![0]).toMatchObject({
    action: 'deliverAccess',
    id: 'created',
    delivery: 'configuration',
  });
  expect(within(dialog).getByRole('status')).toHaveTextContent(
    'Configuration copied. Add it to your client’s MCP settings',
  );
  await user.click(within(dialog).getByRole('button', { name: 'Back to connections' }));
  expect(screen.getByText('No usage recorded')).toBeInTheDocument();
  expect(screen.queryByText('Connected')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Edit Agent 1' }));
  await user.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Copy instructions' }),
  );
  expect(state.issue.mock.calls[3]![0]).toMatchObject({
    action: 'deliverAccess',
    id: 'created',
    delivery: 'prompt',
  });
  expect(state.snapshot!.entries).toHaveLength(1);
});
it('connection setup preserves existing and revoked records and can configure an uninstalled client', async () => {
  state.snapshot!.entries = [
    { id: 'old', name: 'My existing Cursor', client: 'cursor', enabled: true, generation: 1 },
    { id: 'revoked', name: 'Revoked access', client: null, enabled: false, generation: 2 },
  ];
  const before = structuredClone(state.snapshot!.entries);
  const user = userEvent.setup();
  view();
  expect(screen.getByText('My existing Cursor')).toBeInTheDocument();
  expect(screen.getByText('Revoked access')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Edit Revoked access' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Edit My existing Cursor' }));
  const dialog = await screen.findByRole('dialog');
  expect(
    within(dialog).queryByRole('button', { name: 'Choose a client (optional)' }),
  ).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Manual configuration' }));
  await user.click(within(dialog).getByRole('button', { name: 'Copy configuration' }));
  expect(state.issue).toHaveBeenCalledWith(
    { action: 'deliverAccess', id: 'old', client: 'cursor', delivery: 'configuration' },
    expect.any(Function),
  );
  expect(state.snapshot!.entries).toEqual(before);
});
it('connection setup previews and closes a draft without creating anything or retaining its name', async () => {
  const user = userEvent.setup();
  view();
  const dialog = await addConnection(user);
  const name = within(dialog).getByRole('textbox', { name: 'Connection name' });
  await user.clear(name);
  expect(within(dialog).getByRole('button', { name: 'Add and copy' })).toBeDisabled();
  await user.type(name, 'Unsaved connection');
  expect(dialog.querySelector('pre')).toHaveTextContent(
    'Filled with this entry’s token when copied',
  );
  await user.click(within(dialog).getByRole('button', { name: 'Manual configuration' }));
  expect(dialog.querySelector('pre')).toHaveTextContent('mcpServers');
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(state.issue).not.toHaveBeenCalled();
  expect(state.snapshot!.entries).toHaveLength(0);
  const reopened = await addConnection(user);
  expect(within(reopened).getByRole('textbox', { name: 'Connection name' })).toHaveValue('Agent 1');
});
it('connection setup accepts a custom name independently of the optional client and supports manual creation', async () => {
  const user = userEvent.setup();
  view();
  const dialog = await addConnection(user);
  const name = within(dialog).getByRole('textbox', { name: 'Connection name' });
  await user.clear(name);
  await user.type(name, 'My work Agent');
  expect(name).toHaveValue('My work Agent');
  await user.click(within(dialog).getByRole('button', { name: 'Manual configuration' }));
  await user.click(within(dialog).getByRole('button', { name: 'Add and copy' }));
  expect(state.issue).toHaveBeenLastCalledWith(
    expect.objectContaining({
      action: 'prepareAccess',
      name: 'My work Agent',
      client: 'generic',
      delivery: 'configuration',
    }),
    expect.any(Function),
  );
  expect(within(dialog).getByRole('button', { name: 'Copy again' })).toBeInTheDocument();
  expect(state.snapshot!.entries).toHaveLength(1);
});
it('connection setup retries a failed add with the same request instead of creating a second entry', async () => {
  const normal = state.issue.getMockImplementation()!;
  let failed = false;
  state.issue.mockImplementation(
    async (request: AgentUiRequest, receive?: (s: AgentSnapshot) => void) => {
      if (request.action === 'prepareAccess' && !failed) {
        failed = true;
        return false;
      }
      return normal(request, receive);
    },
  );
  const user = userEvent.setup();
  view();
  const dialog = await addConnection(user);
  await user.click(within(dialog).getByRole('button', { name: 'Add and copy' }));
  const original = state.issue.mock.calls[0]![0];
  expect(within(dialog).getByRole('textbox', { name: 'Connection name' })).toBeDisabled();
  expect(within(dialog).getByRole('button', { name: 'Manual configuration' })).toBeDisabled();
  expect(within(dialog).getByRole('alert')).toHaveTextContent('Adding was not confirmed');
  await user.click(within(dialog).getByRole('button', { name: 'Retry' }));
  expect(state.issue.mock.calls[1]![0]).toEqual(original);
  expect(state.snapshot!.entries).toHaveLength(1);
});
it('connection setup counts revoked entries toward 32 and blocks only adding when capacity changes', async () => {
  state.snapshot!.entries = Array.from({ length: 31 }, (_, i) => ({
    id: String(i),
    name: `Agent ${i}`,
    client: null,
    enabled: i !== 0,
    generation: 1,
  }));
  const user = userEvent.setup();
  const rendered = view();
  await addConnection(user);
  state.snapshot!.entries.push({
    id: 'last',
    name: 'Last',
    client: null,
    enabled: true,
    generation: 1,
  });
  rendered.rerender(
    <ConfirmationProvider>
      <McpSettings footer={null} />
    </ConfirmationProvider>,
  );
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('button', { name: 'Add and copy' })).toBeDisabled();
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(screen.getByRole('button', { name: 'Add connection' })).toBeDisabled();
  expect(rendered.container.querySelector('.mcp-access-heading')).toHaveTextContent('32 / 32');
  await user.click(screen.getByRole('button', { name: 'Edit Last' }));
  expect(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Copy instructions' }),
  ).toBeEnabled();
  expect(state.issue).not.toHaveBeenCalled();
});
it('connection setup never replaces an existing connection that is revoked while its panel is open', async () => {
  state.snapshot!.entries = [
    { id: 'old', name: 'Existing', client: null, enabled: true, generation: 1 },
  ];
  const user = userEvent.setup();
  const rendered = view();
  await user.click(screen.getByRole('button', { name: 'Edit Existing' }));
  state.snapshot!.entries[0]!.enabled = false;
  rendered.rerender(
    <ConfirmationProvider>
      <McpSettings footer={null} />
    </ConfirmationProvider>,
  );
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('button', { name: 'Copy instructions' })).toBeDisabled();
  expect(state.issue).not.toHaveBeenCalled();
});

it('access deletion requires confirmation and removes the revoked row and its count only after success', async () => {
  state.snapshot!.entries = [
    { id: 'active', name: 'Active access', client: null, enabled: true, generation: 1 },
    { id: 'old', name: 'Revoked access', client: null, enabled: false, generation: 2 },
  ];
  state.issue.mockImplementation(async (request: AgentUiRequest) => {
    if (request.action === 'deleteAccess')
      state.snapshot!.entries = state.snapshot!.entries.filter((e) => e.id !== request.id);
    return true;
  });
  const user = userEvent.setup();
  const rendered = view();
  await user.click(screen.getByRole('button', { name: 'Delete Revoked access' }));
  const dialog = await screen.findByRole('alertdialog');
  expect(dialog).toHaveTextContent(
    'Historical tasks, recordings, request receipts, and usage records will be kept',
  );
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  expect(state.issue).not.toHaveBeenCalled();
  expect(screen.getByText('Revoked access')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Delete Revoked access' }));
  await user.click(
    within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
  );
  await waitFor(() =>
    expect(state.issue).toHaveBeenCalledWith({ action: 'deleteAccess', id: 'old', generation: 2 }),
  );
  rendered.rerender(
    <ConfirmationProvider>
      <McpSettings footer={null} />
    </ConfirmationProvider>,
  );
  expect(screen.queryByText('Revoked access')).not.toBeInTheDocument();
  expect(screen.getByText('Active access')).toBeInTheDocument();
  expect(rendered.container.querySelector('.mcp-access-heading')).toHaveTextContent('1');
});

it('access deletion is unavailable for active entries and preserves a revoked row on failure', async () => {
  state.snapshot!.entries = [
    { id: 'active', name: 'Active access', client: null, enabled: true, generation: 1 },
    { id: 'old', name: 'Revoked access', client: null, enabled: false, generation: 2 },
  ];
  state.issue.mockResolvedValue(false);
  const user = userEvent.setup();
  const rendered = view();
  expect(screen.queryByRole('button', { name: 'Delete Active access' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Delete Revoked access' }));
  await user.click(
    within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
  );
  await waitFor(() => expect(state.issue).toHaveBeenCalledOnce());
  expect(screen.getByText('Revoked access')).toBeInTheDocument();
  expect(rendered.container.querySelector('.mcp-access-heading')).toHaveTextContent('2');
});

it.each([
  ['Task snapshot', 'Recording snapshot', 'Task snapshot'],
  ['', 'Recording snapshot', 'Recording snapshot'],
  ['', '', 'Historical access (retired-)'],
])(
  'access deletion history labels use retained names or an ID fallback: %s / %s',
  async (taskName, recordingName, expected) => {
    const task = {
      id: 'task',
      entryId: 'retired-entry-id',
      entryName: taskName,
      connectionId: 'connection',
      connectionName: 'Fixture',
      contextId: 'session',
      phase: 'succeeded',
      createdAt: Date.now(),
      tool: 'command_run.command',
    } as AgentTask;
    const recording = {
      id: 'session',
      entryId: task.entryId,
      entryName: recordingName,
      connectionId: 'connection',
      connectionName: 'Fixture',
      createdAt: Date.now(),
    } as RecordedSession;
    const request = vi.fn(async (value: AgentUiRequest) => {
      if (value.action === 'activitySummaries')
        return {
          ok: true,
          reply: {
            kind: 'activitySummaries',
            more: false,
            items: [
              {
                id: 'task',
                taskId: 'task',
                sessionId: 'session',
                entryName: expected,
                connectionName: 'Fixture',
                createdAt: 1,
                kind: task.tool,
                summary: 'command',
                phase: 'succeeded',
                uncertain: true,
              },
            ],
          },
        };
      if (value.action === 'tasks') return { ok: true, reply: { kind: 'tasks', tasks: [task] } };
      if (value.action === 'recordings')
        return { ok: true, reply: { kind: 'recordings', sessions: [recording] } };
      throw Error('History rendering must not issue mutations');
    });
    Object.defineProperty(window, 'vigilshell', {
      configurable: true,
      value: { mcp: { request, onChanged: () => () => {} } },
    });
    render(<AgentActivity onBack={() => {}} target={{ view: 'activity' }} />);
    expect(await screen.findAllByText(expected)).not.toHaveLength(0);
    expect(expected).toBeTruthy();
    expect(task.entryName).toBe(taskName);
    expect(recording.entryName).toBe(recordingName);
    expect(
      request.mock.calls.every(([value]) =>
        ['activitySources', 'activitySummaries', 'tasks', 'recordings', 'activity'].includes(
          value.action,
        ),
      ),
    ).toBe(true);
  },
);

it('saves activity values on Enter or blur without a save button or duplicate submission', async () => {
  const user = userEvent.setup();
  state.issue.mockImplementation(async (request: AgentUiRequest) => {
    if (request.action === 'updateSettings' && request.patch.group === 'activity') {
      state.snapshot!.settings.retentionDays = request.patch.retentionDays;
      state.snapshot!.settings.storageMib = request.patch.storageMib;
    }
    return true;
  });
  render(<AgentActivitySettings footer={null} register={() => {}} />);
  expect(screen.queryByRole('button', { name: 'Save settings' })).not.toBeInTheDocument();
  const [days, capacity] = screen.getAllByRole('spinbutton');
  await user.clear(days!);
  await user.type(days!, '45');
  expect(state.issue).not.toHaveBeenCalled();
  await user.keyboard('{Enter}');
  await user.click(capacity!);
  expect(state.issue).toHaveBeenCalledTimes(1);
  expect(state.issue.mock.calls[0]![0]).toMatchObject({
    action: 'updateSettings',
    patch: {
      group: 'activity',
      retentionDays: 45,
      storageMib: 256,
      baseRetentionDays: 30,
      baseStorageMib: 256,
    },
  });
  await user.clear(capacity!);
  await user.type(capacity!, '128');
  await user.tab();
  expect(state.issue).toHaveBeenCalledTimes(2);
  expect(state.issue.mock.calls[1]![0]).toMatchObject({
    patch: { retentionDays: 45, storageMib: 128, baseRetentionDays: 45, baseStorageMib: 256 },
  });
});
it('rejects invalid activity values and restores the saved value on Escape', async () => {
  const user = userEvent.setup();
  render(<AgentActivitySettings footer={null} register={() => {}} />);
  const days = screen.getAllByRole('spinbutton')[0]!;
  await user.clear(days);
  await user.type(days, '0{Enter}');
  expect(days).toHaveAttribute('aria-invalid', 'true');
  expect(state.issue).not.toHaveBeenCalled();
  await user.keyboard('{Escape}');
  await user.tab();
  expect(days).toHaveValue(30);
  expect(state.issue).not.toHaveBeenCalled();
});
it('connection setup consolidates copying and reset into configuration and keeps local names fixed', async () => {
  state.snapshot!.entries = [
    { id: 'existing', name: 'Existing', client: 'cursor', enabled: true, generation: 1 },
  ];
  const user = userEvent.setup();
  view();
  expect(
    screen.queryByRole('button', { name: 'Copy instructions for Existing' }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Revoke Existing' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Manage Existing' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Edit Existing' }));
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).queryByRole('textbox')).not.toBeInTheDocument();
  expect(within(dialog).getByText('Existing')).toBeInTheDocument();
  expect(
    within(dialog).queryByRole('button', { name: 'Reset access credential' }),
  ).not.toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Advanced settings' }));
  await user.click(within(dialog).getByRole('button', { name: 'Reset access credential' }));
  let confirmation = await screen.findByRole('alertdialog');
  await user.click(within(confirmation).getByRole('button', { name: 'Cancel' }));
  expect(state.issue).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole('button', { name: 'Reset access credential' }));
  confirmation = await screen.findByRole('alertdialog');
  await user.click(within(confirmation).getByRole('button', { name: 'Reset access credential' }));
  await waitFor(() =>
    expect(state.issue).toHaveBeenCalledWith({ action: 'rotateAccess', id: 'existing' }),
  );
  expect(within(dialog).getByRole('status')).toHaveTextContent('Access credentials reset');
  await user.click(within(dialog).getByRole('button', { name: 'Copy instructions' }));
  expect(state.issue).toHaveBeenLastCalledWith(
    { action: 'deliverAccess', id: 'existing', client: 'cursor', delivery: 'prompt' },
    expect.any(Function),
  );
});

it('connection setup saves generic names explicitly, preserves failed drafts and cancels unsaved edits', async () => {
  state.snapshot!.entries = [
    { id: 'existing', name: 'Original', client: null, enabled: true, generation: 1 },
  ];
  const user = userEvent.setup();
  view();
  await user.click(screen.getByRole('button', { name: 'Edit Original' }));
  let dialog = await screen.findByRole('dialog');
  const input = within(dialog).getByRole('textbox', { name: 'Connection name' });
  expect(input).toHaveValue('Original');
  await user.clear(input);
  expect(within(dialog).getByRole('button', { name: 'Save' })).toBeDisabled();
  await user.type(input, 'Renamed');
  expect(state.issue).not.toHaveBeenCalled();
  expect(within(dialog).getByRole('button', { name: 'Copy instructions' })).toBeDisabled();
  state.issue.mockResolvedValueOnce(false);
  await user.click(within(dialog).getByRole('button', { name: 'Save' }));
  expect(input).toHaveValue('Renamed');
  expect(state.snapshot!.entries[0]!.name).toBe('Original');
  state.issue.mockImplementation(async (request: AgentUiRequest) => {
    if (request.action === 'renameAccess') state.snapshot!.entries[0]!.name = request.name;
    return true;
  });
  await user.click(within(dialog).getByRole('button', { name: 'Save' }));
  expect(state.issue).toHaveBeenLastCalledWith({
    action: 'renameAccess',
    id: 'existing',
    name: 'Renamed',
  });
  expect(input).toHaveValue('Renamed');
  expect(within(dialog).getByRole('button', { name: 'Save' })).toBeDisabled();
  expect(within(dialog).getByRole('button', { name: 'Copy instructions' })).toBeEnabled();
  await user.type(input, ' unsaved');
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  await user.click(screen.getByRole('button', { name: 'Edit Renamed' }));
  dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('textbox', { name: 'Connection name' })).toHaveValue('Renamed');
  expect(state.issue).toHaveBeenCalledTimes(2);
});

it('access revocation uses one confirmed request and removes the row only on success', async () => {
  state.snapshot!.entries = [
    { id: 'active', name: 'Active', client: 'generic', enabled: true, generation: 1 },
    { id: 'legacy', name: 'Legacy', client: null, enabled: false, generation: 2 },
  ];
  const user = userEvent.setup();
  const rendered = view();
  const openRevoke = async () => {
    await user.click(screen.getByRole('button', { name: 'Revoke Active' }));
    return screen.findByRole('alertdialog');
  };
  let dialog = await openRevoke();
  expect(dialog).toHaveTextContent('freeing one slot');
  expect(dialog).toHaveTextContent(
    'Historical tasks, recordings, receipts, and usage records will be kept',
  );
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
  expect(state.issue).not.toHaveBeenCalled();
  state.issue.mockResolvedValueOnce(false);
  dialog = await openRevoke();
  await user.click(within(dialog).getByRole('button', { name: 'Revoke' }));
  await waitFor(() => expect(state.issue).toHaveBeenCalledOnce());
  expect(screen.getByText('Active')).toBeInTheDocument();
  state.issue.mockImplementation(async (request: AgentUiRequest) => {
    if (request.action === 'revokeAccess')
      state.snapshot!.entries = state.snapshot!.entries.filter((e) => e.id !== request.id);
    return true;
  });
  dialog = await openRevoke();
  await user.click(within(dialog).getByRole('button', { name: 'Revoke' }));
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(2));
  expect(state.issue).toHaveBeenLastCalledWith({ action: 'revokeAccess', id: 'active' });
  rendered.rerender(
    <ConfirmationProvider>
      <McpSettings footer={null} />
    </ConfirmationProvider>,
  );
  expect(screen.queryByText('Active')).not.toBeInTheDocument();
  expect(screen.getByText('Legacy')).toBeInTheDocument();
  expect(rendered.container.querySelector('.mcp-access-heading')).toHaveTextContent('1 / 32');
});

it('agent menu scans when opened, puts generic last and opens configuration only after selection', async () => {
  state.snapshot!.clients.push({
    client: 'dsh',
    installed: true,
    configurationFound: true,
    unavailable: false,
  });
  const user = userEvent.setup();
  view();
  expect(state.issue).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Add connection' }));
  const menu = await screen.findByRole('menu');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(state.issue).toHaveBeenCalledWith({ action: 'detectClients' });
  const items = within(menu).getAllByRole('menuitem');
  expect(items.at(-1)).toHaveTextContent('Generic Agent');
  const harness = within(menu).getByRole('menuitem', { name: 'DeepSeek Harness' });
  expect(harness.textContent).not.toContain('DSH');
  expect(harness.querySelector('.mcp-client-logo')).toBeInTheDocument();
  expect(within(menu).queryByText('Detect again')).not.toBeInTheDocument();
  await user.click(harness);
  const dialog = await screen.findByRole('dialog');
  expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  expect(
    within(dialog).queryByRole('textbox', { name: 'Connection name' }),
  ).not.toBeInTheDocument();
  expect(within(dialog).getByText('DeepSeek Harness')).toBeInTheDocument();
  expect(dialog.querySelector('.mcp-client-logo')).toBeInTheDocument();
  expect(
    within(dialog).queryByRole('button', { name: 'Back to Agent selection' }),
  ).not.toBeInTheDocument();
  expect(state.snapshot!.entries).toHaveLength(0);
  await user.click(within(dialog).getByRole('button', { name: 'Add and copy' }));
  expect(state.issue).toHaveBeenLastCalledWith(
    expect.objectContaining({ action: 'prepareAccess', client: 'dsh', name: 'DeepSeek Harness' }),
    expect.any(Function),
  );
  expect(state.snapshot!.entries).toHaveLength(1);
});
it('agent menu rescans on reopen after failure and keeps generic available', async () => {
  state.issue.mockResolvedValue(false);
  const user = userEvent.setup();
  view();
  await user.click(screen.getByRole('button', { name: 'Add connection' }));
  const menu = await screen.findByRole('menu');
  expect(
    await within(menu).findByText(
      'Client detection was incomplete. Reopen the menu to retry or use Generic Agent',
    ),
  ).toBeInTheDocument();
  expect(within(menu).queryByRole('menuitem', { name: 'Codex' })).not.toBeInTheDocument();
  await user.click(within(menu).getByRole('menuitem', { name: 'Generic Agent' }));
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('button', { name: 'Add and copy' })).toBeEnabled();
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  state.issue.mockResolvedValue(true);
  await user.click(screen.getByRole('button', { name: 'Add connection' }));
  expect(await screen.findByRole('menuitem', { name: 'Codex' })).toBeInTheDocument();
  expect(state.issue).toHaveBeenCalledTimes(2);
  expect(state.snapshot!.entries).toHaveLength(0);
});
it('agent menu shares a pending scan across reopening without stale choices or creating data', async () => {
  let finish!: (ok: boolean) => void;
  state.issue.mockImplementation(
    () =>
      new Promise<boolean>((resolve) => {
        finish = resolve;
      }),
  );
  const user = userEvent.setup();
  view();
  const trigger = screen.getByRole('button', { name: 'Add connection' });
  trigger.focus();
  await user.keyboard('{Enter}');
  expect(await screen.findByRole('menuitem', { name: 'Generic Agent' })).not.toHaveAttribute(
    'aria-disabled',
    'true',
  );
  expect(screen.queryByRole('menuitem', { name: 'Codex' })).not.toBeInTheDocument();
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
  await waitFor(() => expect(trigger).toHaveFocus());
  await user.keyboard('{Enter}');
  await screen.findByRole('menu');
  expect(state.issue).toHaveBeenCalledTimes(1);
  await act(async () => finish(true));
  expect(await screen.findByRole('menuitem', { name: 'Codex' })).toBeInTheDocument();
  await user.keyboard('{Escape}');
  expect(state.snapshot!.entries).toHaveLength(0);
});

it('restores activity retention inputs when saving fails', async () => {
  const user = userEvent.setup();
  state.issue.mockResolvedValue(false);
  render(<AgentActivitySettings footer={null} register={() => {}} />);
  const days = screen.getAllByRole('spinbutton')[0]!;
  await user.clear(days);
  await user.type(days, '45{Enter}');
  await waitFor(() => expect(screen.getAllByRole('spinbutton')[0]).toHaveValue(30));
  expect(state.issue).toHaveBeenCalledOnce();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
