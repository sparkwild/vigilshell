import { defaultPreferences } from '../../../shared/preferences';
import { it, expect, vi } from 'vitest';
import type { DesktopSnapshot } from '@contracts';
import { createDesktopStore, type SettingsBridge } from './desktop-store';
const snapshot = (version: number): DesktopSnapshot => ({
  version,
  save: { phase: 'idle', error: null },
  error: null,
  preferences: {
    ...defaultPreferences,
    themeMode: 'system',
    localeMode: 'system',
    revision: 0,
  },
  runtime: { status: 'ready', instanceId: 'runtime' },
  appearance: { theme: 'light', locale: 'en', reducedTransparency: false },
});

it('subscribes before reading, ignores stale snapshots and releases subscriptions', async () => {
  let send: (value: DesktopSnapshot) => void = () => {};
  const off = vi.fn();
  const bridge = {
    chooseDownloadDirectory: async () => snapshot(4),
    onSnapshotChanged: (listener: typeof send) => {
      send = listener;
      return off;
    },
    getSnapshot: async () => {
      send(snapshot(3));
      return snapshot(1);
    },
    updatePreferences: async () => snapshot(4),
    retryRuntime: async () => snapshot(5),
    reconcilePreferences: async () => snapshot(6),
    restartBackend: async () => snapshot(7),
    rendererReady: vi.fn(),
    onOpenSettings: () => vi.fn(),
  } satisfies SettingsBridge;
  const store = createDesktopStore(bridge);
  const listener = vi.fn();
  store.subscribe(listener);
  await store.initialize();
  expect(store.getSnapshot().version).toBe(3);
  send(snapshot(2));
  expect(listener).toHaveBeenCalledTimes(1);
  send(snapshot(4));
  expect(listener).toHaveBeenCalledTimes(2);
  store.dispose();
  expect(off).toHaveBeenCalledOnce();
  send(snapshot(9));
  expect(store.getSnapshot().version).toBe(4);
  expect(listener).toHaveBeenCalledTimes(2);
});
