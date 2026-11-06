// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import type { UpdateSnapshot } from '@contracts';
import { ApplicationSettings, UpdateBadge } from './ApplicationSettings';
import { ApplicationProvider } from '../../app/ApplicationProvider';
const save = vi.fn().mockResolvedValue(false);
vi.mock('../../app/DesktopProvider', () => ({ useDesktop: () => ({ t: (key: string) => key }) }));
vi.mock('./use-preferences', () => ({
  usePreferences: () => ({
    preferences: { autoCheckUpdates: true },
    disabled: false,
    save,
    error: null,
  }),
}));
let current: UpdateSnapshot;
let receive: (state: UpdateSnapshot) => void;
const open = vi.fn().mockResolvedValue(undefined);
const check = vi.fn();
function publish(state: UpdateSnapshot) {
  current = state;
  receive(state);
}
function page(props: Parameters<typeof ApplicationSettings>[0] = {}) {
  return (
    <ApplicationProvider>
      <UpdateBadge />
      <ApplicationSettings {...props} />
    </ApplicationProvider>
  );
}
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
  current = { phase: 'idle', release: null };
  receive = () => {};
  open.mockClear();
  save.mockClear();
  check.mockReset().mockImplementation(async () => current);
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      application: {
        info: async () => ({
          name: 'VigilShell',
          version: '0.1.0-beta.1',
        }),
        state: async () => current,
        onChanged: (listener: typeof receive) => {
          receive = listener;
          return () => {
            receive = () => {};
          };
        },
        open,
        check,
      },
    },
  });
});
afterEach(cleanup);
it('keeps background discovery non-blocking and shows plain release notes only on demand', async () => {
  render(page());
  await screen.findByText('v0.1.0-beta.1');
  await act(async () =>
    publish({
      phase: 'available',
      release: { version: '0.1.0-beta.2', tag: 'v0.1.0-beta.2', notes: '<script>unsafe</script>' },
    }),
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(check).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'updates_view' }));
  const dialog = screen.getByRole('dialog', { name: 'updates_check' });
  expect(within(dialog).getByText('<script>unsafe</script>')).toBeVisible();
  expect(dialog.querySelector('script')).toBeNull();
  expect(within(dialog).getByText('v0.1.0-beta.2')).toBeVisible();
  await userEvent.click(within(dialog).getByRole('button', { name: 'updates_download' }));
  expect(open).toHaveBeenCalledWith('openRelease');
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.queryByRole('switch')).not.toBeInTheDocument();
  expect(save).not.toHaveBeenCalled();
});
it('reopens a running check without a second request and keeps its result after closing', async () => {
  let finish!: () => void;
  check.mockImplementation(() => {
    publish({ ...current, phase: 'checking' });
    return new Promise<void>((resolve) => {
      finish = () => {
        publish({ phase: 'current', release: null });
        resolve();
      };
    });
  });
  render(page());
  await screen.findByText('v0.1.0-beta.1');
  await userEvent.click(screen.getByRole('button', { name: 'updates_check' }));
  expect(within(screen.getByRole('dialog')).getByText('updates_checking')).toBeVisible();
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  await userEvent.click(screen.getByRole('button', { name: 'updates_check' }));
  expect(check).toHaveBeenCalledOnce();
  await act(async () => finish());
  expect(within(screen.getByRole('dialog')).getByText('updates_current')).toBeVisible();
  expect(
    within(screen.getByRole('dialog')).queryByRole('button', { name: 'updates_download' }),
  ).toBeNull();
});
it.each(['empty', 'networkError', 'rateLimited', 'invalidResponse'] as const)(
  'shows one generic failure for %s and dismisses without another request',
  async (phase) => {
    check.mockImplementation(async () => publish({ phase, release: null }));
    render(page());
    await userEvent.click(screen.getByRole('button', { name: 'updates_check' }));
    const dialog = screen.getByRole('dialog');
    await within(dialog).findByText('updates_failed');
    expect(within(dialog).queryByText(`updates_${phase}`)).toBeNull();
    expect(within(dialog).queryByRole('button', { name: 'retry' })).toBeNull();
    expect(within(dialog).queryByText('updates_current')).toBeNull();
    await userEvent.click(within(dialog).getByRole('button', { name: 'updates_dismiss' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(check).toHaveBeenCalledOnce();
  },
);
it('handles a menu check request once and reports IPC failure in the dialog', async () => {
  const handled = vi.fn();
  check.mockRejectedValue(Error('unavailable'));
  const view = render(page({ checkRequest: 1, onCheckRequestHandled: handled }));
  await within(await screen.findByRole('dialog')).findByText('updates_failed');
  expect(handled).toHaveBeenCalledOnce();
  expect(check).toHaveBeenCalledOnce();
  view.rerender(page({ checkRequest: 0, onCheckRequestHandled: handled }));
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  view.rerender(page({ checkRequest: 2, onCheckRequestHandled: handled }));
  await screen.findByRole('dialog');
  expect(check).toHaveBeenCalledTimes(2);
});

it('does not present a cached release as a successful result after a failed check', async () => {
  const release = { version: '0.1.0-beta.2', tag: 'v0.1.0-beta.2', notes: 'Cached notes' };
  current = { phase: 'available', release };
  check.mockImplementation(async () => publish({ phase: 'networkError', release }));
  render(page({ checkRequest: 1 }));
  const dialog = await screen.findByRole('dialog');
  await within(dialog).findByText('updates_failed');
  expect(within(dialog).queryByText('Cached notes')).toBeNull();
  expect(within(dialog).queryByRole('button', { name: 'updates_download' })).toBeNull();
  await userEvent.click(within(dialog).getByRole('button', { name: 'updates_dismiss' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  await userEvent.click(screen.getByRole('button', { name: 'updates_check' }));
  expect(check).toHaveBeenCalledTimes(2);
});
