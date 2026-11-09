import { EventEmitter } from 'node:events';
import { expect, it, vi } from 'vitest';
import { registerMcp } from './electron';
import type { BackendClient } from '../backend/client';
import type { NotificationFeedbackCoordinator } from '../notifications/feedback';
vi.mock('electron', () => ({
  app: { once: vi.fn() },
  clipboard: {},
  dialog: {},
  safeStorage: {},
  ipcMain: { handle: vi.fn(), removeHandler: vi.fn() },
}));
it('serves history while key access is unlocking without configuration initialization', async () => {
  const host = Object.assign(new EventEmitter(), {
    instanceId: 'runtime',
    workspaceCall: vi.fn(() => ({
      result: Promise.resolve({
        kind: 'ready',
        version: 16,
        connectionAccess: { phase: 'unlocking', revision: 1, code: null },
      }),
    })),
    request: vi.fn(async (command) => ({
      kind: 'workspace',
      result: {
        kind: 'agent',
        result:
          command.params.request.operation === 'activitySummaries'
            ? { kind: 'activitySummaries', items: [], more: false }
            : { kind: 'done' },
      },
    })),
  });
  const controller = registerMcp({
    host: host as unknown as BackendClient,
    bridgePath: 'unused',
    trusted: () => true,
    window: () => undefined,
    locale: () => 'en',
    scrollback: () => 1000,
    notifications: {} as NotificationFeedbackCoordinator,
    tray: () => {},
  });
  expect(await controller.ui({ action: 'activitySummaries', filter: {} })).toMatchObject({
    ok: true,
    reply: { kind: 'activitySummaries' },
  });
  expect(host.workspaceCall).toHaveBeenCalledOnce();
  expect(host.request.mock.calls.map(([c]) => c.params.request.operation)).toEqual([
    'activitySummaries',
  ]);
  host.emit('configurationReady', 'runtime');
  await Promise.resolve();
  expect(host.request).toHaveBeenCalledOnce();
  await controller.stop();
});
