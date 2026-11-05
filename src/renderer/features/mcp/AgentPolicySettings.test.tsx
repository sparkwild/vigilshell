// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { AgentSnapshot, ConnectionPolicySettings, ConnectionSummary } from '@contracts';
import { translate } from '../../../shared/i18n/messages';
import { defaultPolicySettings, type useMcp } from './model';
import { PolicyEditor } from './policy/PolicyEditor';
import { validPolicy } from './policy/policy-model';
import { usePolicySettings } from './policy/use-policy-settings';

const mocks = vi.hoisted(() => ({ decide: vi.fn() }));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Parameters<typeof translate>[2]) =>
      translate(key, 'en', values),
  }),
}));
vi.mock('../../shared/ConfirmationProvider', () => ({ useDecision: () => mocks.decide }));
const connection = {
  id: 'one',
  name: 'Dev host',
  host: 'dev.example',
  environment: 'development',
  revision: 1,
  deleting: false,
} as ConnectionSummary;
const basePolicy = (): ConnectionPolicySettings => ({
  ...defaultPolicySettings('one'),
  connectionRevision: 1,
  execution: { ...defaultPolicySettings('one').execution, level: 'operate' },
});
const snapshot = (policy: ConnectionPolicySettings): AgentSnapshot => ({
  settings: {
    revision: 0,
    enabled: false,
    port: 51602,
    showTray: true,
    retentionDays: 30,
    storageMib: 256,
  },
  entries: [],
  policies: [policy],
  contexts: [],
  views: [],
  clients: [],
  observations: [],
  endpoint: 'http://127.0.0.1:51602/mcp',
  running: false,
  error: null,
});
function model(policy = basePolicy()) {
  return {
    snapshot: snapshot(policy),
    error: null,
    busy: false,
    refresh: vi.fn(),
    issue: vi.fn().mockResolvedValue(false),
  } satisfies ReturnType<typeof useMcp>;
}
afterEach(cleanup);
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
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      mcp: { request: vi.fn(async () => ({ ok: true, reply: { kind: 'tasks', tasks: [] } })) },
    },
  });
  mocks.decide.mockReset().mockResolvedValue('apply');
});

it('validates the simplified policy and local directory limits', () => {
  const p = basePolicy();
  expect(validPolicy(p)).toBe(true);
  p.execution.protectedPaths = [{ path: '/srv/keep', kind: 'directory' }];
  expect(validPolicy(p)).toBe(true);
  p.execution.uploadRoots = [{ id: 'local', path: '/' }];
  expect(validPolicy(p)).toBe(false);
});

it('locks fields while access is enabled and removes the batch apply action', () => {
  const p = { ...basePolicy(), accessEnabled: true };
  render(<PolicyEditor connection={connection} policy={p} model={model(p)} register={() => {}} />);
  expect(screen.getByRole('combobox', { name: 'Operation level' })).toBeDisabled();
  expect(screen.getByRole('switch', { name: 'Allow file transfers' })).toHaveAttribute(
    'aria-disabled',
    'true',
  );
  expect(screen.queryByRole('button', { name: 'Apply changes' })).not.toBeInTheDocument();
});
it('saves an individual switch while access remains closed', async () => {
  const p = basePolicy(),
    m = model(p);
  m.issue.mockResolvedValue(true);
  render(<PolicyEditor connection={connection} policy={p} model={m} register={() => {}} />);
  await userEvent.click(screen.getByRole('switch', { name: 'Allow file transfers' }));
  expect(m.issue).toHaveBeenCalledWith(
    expect.objectContaining({
      action: 'updatePolicySettings',
      expectedRevision: 0,
      patch: { field: 'fileTransfer', value: true },
    }),
    expect.any(Function),
  );
  expect(m.issue.mock.calls.some(([r]) => r.action === 'setPolicyAccess')).toBe(false);
});
it('retains failed changes and does not enable access until resolved', async () => {
  const p = basePolicy(),
    m = model(p);
  const { result } = renderHook(() => usePolicySettings(connection, p, m, () => {}));
  await act(async () => {
    await result.current.save({ field: 'fileTransfer', value: true });
  });
  expect(result.current.failed).toEqual({ field: 'fileTransfer', value: true });
  await act(async () => {
    await result.current.toggle(true);
  });
  expect(m.issue).toHaveBeenCalledTimes(1);
  expect(m.refresh).toHaveBeenCalled();
});
it('does not issue parallel saves or enable while a save is pending', async () => {
  const p = basePolicy(),
    m = model(p);
  let done!: (v: boolean) => void;
  m.issue.mockImplementation(
    () =>
      new Promise((resolve) => {
        done = resolve;
      }),
  );
  const { result } = renderHook(() => usePolicySettings(connection, p, m, () => {}));
  let save!: Promise<boolean>;
  act(() => {
    save = result.current.save({ field: 'verifyConnect', value: false });
  });
  await act(async () => {
    await result.current.toggle(true);
    await result.current.save({ field: 'fileTransfer', value: true });
  });
  expect(m.issue).toHaveBeenCalledTimes(1);
  await act(async () => {
    done(true);
    await save;
  });
});
it('waits for blur to save numeric input and rejects an invalid value', async () => {
  const p = basePolicy(),
    m = model(p),
    user = userEvent.setup();
  render(<PolicyEditor connection={connection} policy={p} model={m} register={() => {}} />);
  const input = screen.getByRole('spinbutton', {
    name: translate('policy_concurrent_tasks', 'en'),
  });
  await user.clear(input);
  await user.type(input, '3');
  expect(m.issue).not.toHaveBeenCalled();
  await user.tab();
  expect(m.issue).toHaveBeenCalledTimes(1);
});

it('allows editing and enabling without querying remote stop history', async () => {
  const p = basePolicy(),
    m = model(p);
  m.issue.mockResolvedValue(true);
  const { result } = renderHook(() => usePolicySettings(connection, p, m, () => {}));
  expect(result.current.editable).toBe(true);
  await act(async () => {
    await result.current.toggle(true);
  });
  expect(m.issue).toHaveBeenCalledWith(
    expect.objectContaining({ action: 'setPolicyAccess', enabled: true }),
    undefined,
    expect.any(Function),
  );
  expect(window.vigilshell.mcp.request).not.toHaveBeenCalled();
  expect(mocks.decide).not.toHaveBeenCalled();
});
it('closes access without a stop confirmation', async () => {
  const p = { ...basePolicy(), accessEnabled: true },
    m = model(p);
  m.issue.mockResolvedValue(true);
  const { result } = renderHook(() => usePolicySettings(connection, p, m, () => {}));
  await act(async () => {
    await result.current.toggle(false);
  });
  expect(m.issue).toHaveBeenCalledWith(
    expect.objectContaining({ action: 'setPolicyAccess', enabled: false }),
    undefined,
    expect.any(Function),
  );
  expect(mocks.decide).not.toHaveBeenCalled();
});

it('explains access locking from the header and setting row', async () => {
  const p = { ...basePolicy(), accessEnabled: true };
  const user = userEvent.setup();
  const { rerender } = render(
    <PolicyEditor connection={connection} policy={p} model={model(p)} register={() => {}} />,
  );
  const badge = screen.getByText('Configuration locked');
  await user.hover(badge);
  expect(await screen.findByRole('tooltip')).toHaveTextContent(
    translate('policy_locked_hint', 'en'),
  );
  await user.unhover(badge);
  const control = screen.getByRole('switch', { name: 'Allow file transfers' });
  await user.hover(control.closest('[data-slot="field"]')!);
  expect(await screen.findByRole('tooltip')).toHaveTextContent(
    translate('policy_locked_hint', 'en'),
  );
  expect(control).toHaveAttribute('aria-disabled', 'true');
  const closed = { ...p, accessEnabled: false };
  rerender(
    <PolicyEditor
      connection={connection}
      policy={closed}
      model={model(closed)}
      register={() => {}}
    />,
  );
  expect(screen.queryByText('Configuration locked')).not.toBeInTheDocument();
});

it('explains read-only script review separately from access locking', async () => {
  const p = basePolicy();
  p.execution.level = 'readOnly';
  const user = userEvent.setup();
  render(<PolicyEditor connection={connection} policy={p} model={model(p)} register={() => {}} />);
  const control = screen.getByRole('switch', { name: translate('policy_script_review', 'en') });
  await user.hover(control.closest('[data-slot="field"]')!);
  expect(await screen.findByRole('tooltip')).toHaveTextContent(
    translate('policy_requires_operate', 'en'),
  );
  expect(screen.queryByText('Configuration locked')).not.toBeInTheDocument();
});

it('requires one directory before saving the shared restriction', async () => {
  const p = basePolicy(),
    m = model(p),
    user = userEvent.setup();
  render(<PolicyEditor connection={connection} policy={p} model={m} register={() => {}} />);
  expect(
    screen.queryByRole('textbox', { name: 'Local transfer directory' }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole('switch', { name: 'Restrict local transfer directory' }));
  expect(m.issue).not.toHaveBeenCalled();
  const input = screen.getByRole('textbox', { name: 'Local transfer directory' });
  await user.type(input, 'relative');
  await user.tab();
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(m.issue).not.toHaveBeenCalled();
  await user.clear(input);
  await user.type(input, '/tmp/shared');
  await user.tab();
  expect(m.issue).toHaveBeenCalledWith(
    expect.objectContaining({ patch: { field: 'localDirectory', value: '/tmp/shared' } }),
    expect.any(Function),
  );
});

it('turns off both directional grants with a single settings update', async () => {
  const p = basePolicy(),
    user = userEvent.setup();
  p.execution.uploadRoots = [{ id: 'local', path: '/tmp/shared' }];
  p.execution.downloadRoots = [{ id: 'local', path: '/tmp/shared' }];
  const m = model(p);
  render(<PolicyEditor connection={connection} policy={p} model={m} register={() => {}} />);
  await user.click(screen.getByRole('switch', { name: 'Restrict local transfer directory' }));
  expect(m.issue).toHaveBeenCalledWith(
    expect.objectContaining({ patch: { field: 'localDirectory', value: null } }),
    expect.any(Function),
  );
});
