// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import type { DesktopSnapshot } from '@contracts';
import { DesktopProvider } from './DesktopProvider';
import { RuntimeFeedback, SettingsRuntimeFeedback } from './RuntimeFeedback';
import { createDesktopStore, type SettingsBridge } from '../shared/platform/desktop-store';
import { UiProvider } from '../shared/UiProvider';

beforeEach(() => {
  vi.stubGlobal('vigilshell', {});
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function fixture() {
  let current: DesktopSnapshot = {
    version: 1,
    runtime: { status: 'starting', instanceId: null },
    preferences: null,
    error: null,
    save: { phase: 'idle', error: null },
    appearance: { theme: 'dark', locale: 'zh-CN', reducedTransparency: false },
  };
  let receive: (value: DesktopSnapshot) => void = () => {};
  const retry = vi.fn(async () => current);
  const store = createDesktopStore({
    getSnapshot: async () => current,
    onSnapshotChanged: (listener) => {
      receive = listener;
      return () => {};
    },
    retryRuntime: retry,
    chooseDownloadDirectory: async () => current,
    updatePreferences: async () => current,
    reconcilePreferences: async () => current,
    restartBackend: async () => current,
    rendererReady: () => {},
    onOpenSettings: () => () => {},
  } satisfies SettingsBridge);
  await store.initialize();
  const view = (settingsOpen: boolean) => (
    <DesktopProvider store={store}>
      <UiProvider>
        <RuntimeFeedback settingsOpen={settingsOpen} />
        {settingsOpen ? <SettingsRuntimeFeedback /> : null}
      </UiProvider>
    </DesktopProvider>
  );
  const result = render(view(false));
  return {
    retry,
    switchView: (settingsOpen: boolean) => result.rerender(view(settingsOpen)),
    async publish(patch: Partial<DesktopSnapshot>) {
      await act(async () => {
        current = { ...current, ...patch, version: current.version + 1 };
        receive(current);
      });
    },
  };
}

it('keeps startup silent and shows a persistent local failure without guessing a cause', async () => {
  const f = await fixture();
  expect(document.querySelector('[data-slot="toast"]')).toBeNull();
  await f.publish({
    runtime: { status: 'unavailable', instanceId: null },
    error: 'storage_unavailable',
  });
  expect(await screen.findByText('设置读取失败')).toBeVisible();
  expect(screen.queryByText(/磁盘空间|访问权限/)).toBeNull();
  expect(screen.getAllByRole('alert')).toHaveLength(1);
  await f.publish({});
  expect(screen.getByText('设置读取失败')).toBeVisible();
  await f.publish({ runtime: { status: 'starting', instanceId: null } });
  await f.publish({ runtime: { status: 'unavailable', instanceId: null } });
  expect(await screen.findByText('设置读取失败')).toBeVisible();
  await f.publish({ runtime: { status: 'ready', instanceId: 'runtime' }, error: null });
  await waitFor(() => expect(screen.queryByText('设置读取失败')).toBeNull());
});

it('shows local feedback in settings without duplicating the global notification', async () => {
  const f = await fixture();
  f.switchView(true);
  await f.publish({
    runtime: { status: 'unavailable', instanceId: null },
    error: 'storage_unavailable',
  });
  expect(screen.getByRole('alert')).toHaveTextContent('设置读取失败');
  expect(document.querySelector('[data-slot="toast"]')).toBeNull();
  await userEvent.setup().click(screen.getByRole('button', { name: '重试' }));
  expect(f.retry).toHaveBeenCalledTimes(1);
  f.switchView(false);
  expect(document.querySelector('[data-slot="toast"]')).toBeNull();
});

it('blocks duplicate retry requests and retains the failure when the bridge rejects', async () => {
  const f = await fixture();
  let rejectRetry: (error: Error) => void = () => {};
  f.retry.mockImplementation(
    () =>
      new Promise((_, reject) => {
        rejectRetry = reject;
      }),
  );
  await f.publish({
    runtime: { status: 'unavailable', instanceId: null },
    error: 'protocol_mismatch',
  });
  expect(await screen.findByText('应用服务暂不可用')).toBeVisible();
  const user = userEvent.setup();
  const button = screen.getByRole('button', { name: '重试' });
  await user.dblClick(button);
  expect(f.retry).toHaveBeenCalledTimes(1);
  expect(button).toBeDisabled();
  await act(async () => rejectRetry(new Error('bridge unavailable')));
  await waitFor(() => expect(button).toBeEnabled());
  expect(screen.getByText('应用服务暂不可用')).toBeVisible();
});
