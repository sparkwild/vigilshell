import { EventEmitter } from 'node:events';
import { afterEach, expect, it, vi } from 'vitest';
import type { NotificationAuthorization } from '../../contracts/generated/runtime';

const mocks = vi.hoisted(() => ({
  query: vi.fn(),
  request: vi.fn(),
  handle: vi.fn(),
  removeHandler: vi.fn(),
  openExternal: vi.fn().mockResolvedValue(undefined),
  send: vi.fn(),
}));
vi.mock('node:module', () => ({
  createRequire: () => () => ({ query: mocks.query, request: mocks.request }),
}));
vi.mock('electron', async () => {
  const { EventEmitter } = await import('node:events');
  return {
    app: new EventEmitter(),
    BrowserWindow: { getAllWindows: () => [{ isFocused: () => true }] },
    ipcMain: { handle: mocks.handle, removeHandler: mocks.removeHandler },
    shell: { openExternal: mocks.openExternal },
    Notification: class extends EventEmitter {
      static isSupported = () => true;
      static instances: EventEmitter[] = [];
      constructor(public options: unknown) {
        super();
        (
          this.constructor as typeof import('electron').Notification & { instances: EventEmitter[] }
        ).instances.push(this);
      }
      show() {
        this.emit('show');
      }
    },
  };
});
import { app, Notification } from 'electron';
import { registerSystemNotifications } from './electron';
import { notificationChannels } from '../../contracts/system-notifications';

afterEach(() => {
  app.emit('will-quit');
  vi.clearAllMocks();
});

function setup(initial: NotificationAuthorization = 'authorized') {
  let permission = initial;
  mocks.query.mockImplementation((callback: (status: NotificationAuthorization) => void) =>
    callback(permission),
  );
  mocks.request.mockImplementation((callback: (status: NotificationAuthorization) => void) => {
    permission = 'authorized';
    callback(permission);
  });
  const activate = vi.fn();
  const service = registerSystemNotifications({
    bridgePath: '/test/authorization.node',
    bundleId: 'test.product.dev',
    applicationName: 'Product Dev',
    locale: () => 'en',
    preferences: () => ({
      systemNotificationMode: 'always',
      attentionRequiredNotificationEnabled: false,
      executionIssueNotificationEnabled: true,
    }),
    window: () =>
      ({
        isDestroyed: () => false,
        webContents: { isDestroyed: () => false, isCrashed: () => false, send: mocks.send },
      }) as never,
    trusted: (event) => (event as unknown) === 'trusted',
    activate,
  });
  const handler = mocks.handle.mock.calls.find(
    ([channel]) => channel === notificationChannels.request,
  )![1] as (event: unknown, command: unknown) => Promise<unknown>;
  return {
    service,
    activate,
    invoke: (command: unknown, sender = 'trusted') => handler(sender, command),
  };
}

it.skipIf(process.platform !== 'darwin')(
  'rejects untrusted senders and arbitrary notification content',
  async () => {
    const { invoke } = setup();
    await expect(invoke('test', 'foreign')).rejects.toThrow('invalid_request');
    await expect(invoke({ title: 'Injected', body: 'Injected' })).rejects.toThrow(
      'invalid_request',
    );
    expect(mocks.query).not.toHaveBeenCalled();
    expect(mocks.openExternal).not.toHaveBeenCalled();
  },
);

it.skipIf(process.platform !== 'darwin')(
  'click routes to notification settings and cleanup detaches late events',
  async () => {
    const { service, activate } = setup();
    await expect(
      service.publish({
        eventId: 'task:1',
        category: 'executionIssue',
        title: 'Finished',
        body: '1 task',
      }),
    ).resolves.toMatchObject({ outcome: 'submitted' });
    const notification = (
      Notification as typeof Notification & { instances: EventEmitter[] }
    ).instances.at(-1)!;
    notification.emit('click');
    expect(activate).toHaveBeenCalledExactlyOnceWith('task:1');
    app.emit('will-quit');
    notification.emit('click');
    expect(activate).toHaveBeenCalledOnce();
    expect(mocks.removeHandler).toHaveBeenCalledWith(notificationChannels.request);
  },
);

it.skipIf(process.platform !== 'darwin')(
  'requests permission only explicitly and opens the fixed app settings destination',
  async () => {
    const { invoke } = setup('notDetermined');
    await expect(invoke('status')).resolves.toMatchObject({
      status: { authorization: 'notDetermined' },
    });
    expect(mocks.request).not.toHaveBeenCalled();
    await expect(invoke('requestPermission')).rejects.toThrow('invalid_request');
    await expect(invoke('test')).rejects.toThrow('invalid_request');
    expect(mocks.request).not.toHaveBeenCalled();
    await expect(invoke('openSettings')).resolves.toMatchObject({ outcome: 'opened' });
    expect(mocks.openExternal).toHaveBeenCalledWith(
      'x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=test.product.dev',
    );
  },
);

it('validates feedback messages and only activates registered targets', async () => {
  const { service } = setup();
  const handle = mocks.handle.mock.calls.find(
    ([channel]) => channel === notificationChannels.feedback,
  )![1] as (event: unknown, value: unknown) => unknown;
  expect(() => handle('foreign', { action: 'list' })).toThrow('invalid_request');
  expect(() =>
    handle('trusted', { action: 'activate', id: 'x', url: 'https://untrusted.invalid' }),
  ).toThrow('invalid_request');
  expect(handle('trusted', { action: 'activate', id: 'missing' })).toEqual([]);
  expect(service.feedback.list()).toEqual([]);
});
