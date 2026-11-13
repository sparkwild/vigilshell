// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import type { UpdateSnapshot } from '@contracts';
import { ApplicationSettings } from './ApplicationSettings';
import { UpdateBadge } from './UpdateIndicators';
import { UpdateController, useOpenUpdate } from './UpdateController';
import { ApplicationProvider } from '../../app/ApplicationProvider';
const display = vi.hoisted(() => ({ locale: 'en' }));
const save = vi.fn().mockResolvedValue(false);
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: string, inputs?: Record<string, string>) =>
      key === 'updates_versions' ? `${inputs!.current} → ${inputs!.next}` : key,
    snapshot: { appearance: { locale: display.locale } },
  }),
}));
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
const download = vi.fn().mockResolvedValue(undefined);
const cancel = vi.fn().mockResolvedValue(undefined);
const install = vi.fn().mockResolvedValue(undefined);
function publish(state: UpdateSnapshot) {
  current = state;
  receive(state);
}
function ExtraEntry() {
  const open = useOpenUpdate();
  return <button onClick={open}>workspace-update</button>;
}
function page() {
  return (
    <ApplicationProvider>
      <UpdateController>
        <UpdateBadge />
        <ExtraEntry />
        <ApplicationSettings />
      </UpdateController>
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
  display.locale = 'en';
  current = { phase: 'idle', release: null };
  receive = () => {};
  open.mockClear();
  save.mockClear();
  download.mockClear();
  cancel.mockClear();
  install.mockClear();
  check.mockReset().mockImplementation(async () => current);
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      application: {
        info: async () => ({
          name: 'VigilShell',
          version: '0.1.0',
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
        download,
        cancel,
        install,
      },
    },
  });
});
afterEach(cleanup);
it('keeps a download running after dismissing and shows the restored ready action', async () => {
  render(page());
  const release = { version: '0.1.2', tag: 'v0.1.2', notes: 'Notes' };
  await act(async () => publish({ phase: 'available', release, canDownload: true }));
  await userEvent.click(screen.getByRole('button', { name: 'updates_view' }));
  await userEvent.click(screen.getByRole('button', { name: 'updates_download_now' }));
  expect(download).toHaveBeenCalledOnce();
  expect(install).not.toHaveBeenCalled();
  await act(async () =>
    publish({
      phase: 'downloading',
      release,
      canDownload: true,
      progress: { transferred: 5, total: 10, percent: 50 },
    }),
  );
  expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
  await userEvent.click(screen.getByRole('button', { name: 'updates_later' }));
  expect(cancel).not.toHaveBeenCalled();
  await act(async () => publish({ phase: 'ready', release, canDownload: true }));
  expect(install).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'updates_view' }));
  await userEvent.click(screen.getByRole('button', { name: 'updates_install' }));
  expect(install).toHaveBeenCalledOnce();
  await act(async () => publish({ phase: 'preparing', release, canDownload: true }));
  await userEvent.keyboard('{Escape}');
  expect(screen.getByRole('dialog')).toBeVisible();
  expect(within(screen.getByRole('dialog')).queryByRole('button')).toBeNull();
});
it('keeps background discovery non-blocking and shows plain release notes only on demand', async () => {
  render(page());
  await screen.findByText('v0.1.0');
  await act(async () =>
    publish({
      phase: 'available',
      release: {
        version: '0.1.1',
        tag: 'v0.1.1',
        notes: '## Features\n- Safe copy\n<script>unsafe</script>\n## Downloads\nHidden installers',
      },
    }),
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
  expect(screen.getByText('updates_available')).toHaveAttribute('data-variant', 'update');
  expect(check).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'updates_view' }));
  const dialog = screen.getByRole('dialog', { name: 'updates_discovered' });
  expect(within(dialog).getByText('Safe copy')).toBeVisible();
  expect(within(dialog).queryByText('Hidden installers')).toBeNull();
  expect(dialog.querySelector('script')).toBeNull();
  expect(await within(dialog).findByText('v0.1.0 → v0.1.1')).toBeVisible();
  await userEvent.click(within(dialog).getByRole('button', { name: 'updates_download' }));
  expect(open).toHaveBeenCalledWith('openRelease');
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('switch')).toBeChecked();
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
  await screen.findByText('v0.1.0');
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
it('shares workspace and about entries without duplicating the dialog or checking a known release', async () => {
  current = {
    phase: 'available',
    release: { version: '0.1.1', tag: 'v0.1.1', notes: '## Features\n- Cached notes' },
    checkPhase: 'networkError',
  };
  render(page());
  await screen.findByText('v0.1.0');
  await userEvent.click(screen.getByRole('button', { name: 'workspace-update' }));
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(within(screen.getByRole('dialog')).getByText('Cached notes')).toBeVisible();
  expect(check).not.toHaveBeenCalled();
  await userEvent.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(screen.getByRole('button', { name: 'workspace-update' })).toHaveFocus();
  await userEvent.click(screen.getByRole('button', { name: 'updates_view' }));
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(check).not.toHaveBeenCalled();
});
it('shows a generic IPC failure and allows closing the shared dialog', async () => {
  check.mockRejectedValue(Error('unavailable'));
  render(page());
  await userEvent.click(screen.getByRole('button', { name: 'workspace-update' }));
  await within(await screen.findByRole('dialog')).findByText('updates_failed');
  await userEvent.click(screen.getByRole('button', { name: 'updates_dismiss' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
it('keeps the saved switch value when saving fails', async () => {
  render(page());
  await userEvent.click(screen.getByRole('switch'));
  expect(save).toHaveBeenCalledWith({ autoCheckUpdates: false });
  expect(screen.getByRole('switch')).toBeChecked();
});

it('switches release note language during a download without restarting the operation', async () => {
  current = {
    phase: 'downloading',
    canDownload: true,
    progress: { percent: 50, transferred: 5, total: 10 },
    release: {
      version: '0.2.0',
      tag: 'v0.2.0',
      notes: '',
      localizedNotes: {
        en: { features: ['English feature'], upgradeNotes: ['Back up'] },
        'zh-CN': { features: ['中文功能'], upgradeNotes: ['请备份'] },
        fr: { features: ['Fonction'], upgradeNotes: ['Sauvegarder'] },
      },
    },
  };
  const view = render(page());
  await userEvent.click(screen.getByRole('button', { name: 'workspace-update' }));
  expect(await screen.findByText('English feature')).toBeVisible();
  display.locale = 'fr';
  view.rerender(page());
  expect(screen.getByText('Fonction')).toBeVisible();
  expect(screen.getByText('Sauvegarder')).toBeVisible();
  display.locale = 'de';
  view.rerender(page());
  expect(screen.getByText('English feature')).toBeVisible();
  expect(screen.getByText('Back up')).toBeVisible();
  display.locale = 'zh-CN';
  view.rerender(page());
  expect(screen.getByText('中文功能')).toBeVisible();
  expect(screen.getByText('请备份')).toBeVisible();
  expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50');
  expect(check).not.toHaveBeenCalled();
  expect(download).not.toHaveBeenCalled();
  await act(async () =>
    publish({
      ...current,
      release: {
        ...current.release!,
        localizedNotes: {
          en: { features: ['English fallback'], upgradeNotes: ['Back up'] },
          'zh-CN': { features: ['不完整'] },
        },
      },
    }),
  );
  expect(screen.getByText('English fallback')).toBeVisible();
  expect(screen.getByText('Back up')).toBeVisible();
  expect(screen.queryByText('updates_notes_fallback')).not.toBeInTheDocument();
  expect(screen.queryByText('不完整')).toBeNull();
});

it('keeps the release link and separate download/install actions in the compact footer', async () => {
  const release = { version: '0.2.0', tag: 'v0.2.0', notes: '## Features\n- Feature' };
  current = { phase: 'available', release, canDownload: true };
  render(page());
  await userEvent.click(screen.getByRole('button', { name: 'workspace-update' }));
  expect(screen.getByRole('heading', { name: 'updates_discovered' })).toBeVisible();
  expect(await screen.findByText('v0.1.0 → v0.2.0')).toBeVisible();
  const actions = within(screen.getByTestId('update-actions'));
  const link = actions.getByRole('button', { name: 'updates_release_page' });
  expect(link.querySelector('svg[data-icon="inline-end"]')).not.toBeNull();
  await userEvent.click(link);
  expect(open).toHaveBeenCalledWith('openRelease');
  expect(download).not.toHaveBeenCalled();
  await act(async () =>
    publish({
      phase: 'downloading',
      release,
      canDownload: true,
      progress: { percent: 100, transferred: 10, total: 10 },
    }),
  );
  expect(actions.getByRole('button', { name: 'ws_cancel' })).toBeDisabled();
  await act(async () =>
    publish({
      phase: 'downloading',
      release,
      canDownload: true,
      progress: { percent: 50, transferred: 5, total: 10 },
    }),
  );
  await userEvent.click(actions.getByRole('button', { name: 'ws_cancel' }));
  expect(cancel).toHaveBeenCalledOnce();
  await act(async () => publish({ phase: 'downloadError', release, canDownload: true }));
  await userEvent.click(actions.getByRole('button', { name: 'updates_retry_download' }));
  expect(download).toHaveBeenCalledOnce();
  expect(install).not.toHaveBeenCalled();
  await act(async () =>
    publish({ phase: 'installError', release, canDownload: true, issue: 'readOnlyLocation' }),
  );
  expect(screen.getByRole('heading', { name: 'updates_install_failed_title' })).toBeVisible();
  expect(screen.getByText('updates_readOnlyLocation')).toBeVisible();
  await userEvent.click(actions.getByRole('button', { name: 'updates_install' }));
  expect(install).toHaveBeenCalledOnce();
});
