import { EventEmitter } from 'node:events';
import { expect, it, vi } from 'vitest';
import type { BackendClient } from '../backend/client';
import type { NotificationFeedbackCoordinator } from './feedback';
import { observeRuntimeNotifications } from './runtime';
it('captures affected work before source cleanup and emits one root event per runtime', () => {
  const host = Object.assign(new EventEmitter(), { instanceId: 'runtime' });
  let work = 4;
  host.on('unavailable', () => {
    work = 0;
  });
  const publish = vi.fn(),
    failRuntime = vi.fn();
  const dispose = observeRuntimeNotifications(
    host as unknown as BackendClient,
    { publish, failRuntime } as unknown as NotificationFeedbackCoordinator,
    { count: () => work, planned: () => false, locale: () => 'en', available: () => false },
  );
  host.emit('unavailable');
  host.emit('unavailable');
  expect(publish).toHaveBeenCalledOnce();
  expect(publish.mock.calls[0]![0].body).toContain('4');
  expect(failRuntime).toHaveBeenCalledWith('runtime');
  dispose();
  host.instanceId = 'next';
  host.emit('unavailable');
  expect(publish).toHaveBeenCalledOnce();
});
it('ignores planned stop and startup failures without affected work', () => {
  const host = Object.assign(new EventEmitter(), { instanceId: 'runtime' });
  let planned = true;
  const publish = vi.fn();
  observeRuntimeNotifications(
    host as unknown as BackendClient,
    { publish, failRuntime: vi.fn() } as unknown as NotificationFeedbackCoordinator,
    { count: () => 0, planned: () => planned, locale: () => 'en', available: () => false },
  );
  host.emit('unavailable');
  planned = false;
  host.emit('unavailable');
  expect(publish).not.toHaveBeenCalled();
});
