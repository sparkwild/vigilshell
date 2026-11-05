import { app, BrowserWindow, ipcMain, Notification, shell } from 'electron';
import { createRequire } from 'node:module';
import Ajv from 'ajv';
import schema from '../../contracts/generated/runtime.schema.json';
import type {
  NotificationAuthorization,
  NotificationOutcome,
  SystemNotificationCommand,
} from '../../contracts/generated/runtime';
import { notificationChannels } from '../../contracts/system-notifications';
import { NotificationFeedbackCoordinator } from './feedback';
import { SystemNotificationService, type NotificationPorts } from './service';

interface Options {
  bridgePath: string;
  bundleId: string;
  applicationName: string;
  preferences: NotificationPorts['preferences'];
  locale(): 'en' | 'zh-CN';
  window(): BrowserWindow | null | undefined;
  trusted(event: Electron.IpcMainInvokeEvent): boolean;
  activate(eventId?: string): void;
  activateFeedback?(target: import('@contracts').NotificationTarget): void;
}
interface AuthorizationBridge {
  query(callback: (status: NotificationAuthorization) => void): void;
  request(callback: (status: NotificationAuthorization) => void): void;
}

export function registerSystemNotifications(options: Options) {
  let stopped = false;
  let bridge: AuthorizationBridge | undefined;
  const cancel = new Set<() => void>();
  const live = new Set<Notification>();
  const ajv = new Ajv({ strict: false, validateFormats: false });
  ajv.addSchema(schema, 'contract');
  const valid = ajv.compile<SystemNotificationCommand>({
    $ref: 'contract#/definitions/SystemNotificationCommand',
  });
  const supported = () => process.platform === 'darwin' && Notification.isSupported();
  const service = new SystemNotificationService({
    supported,
    preferences: options.preferences,
    focused: () => BrowserWindow.getAllWindows().some((window) => window.isFocused()),
    authorization(request) {
      return new Promise((resolve) => {
        if (stopped) {
          resolve('unknown');
          return;
        }
        const finish = (status: NotificationAuthorization) => {
          if (!cancel.delete(dispose)) return;
          clearTimeout(timer);
          resolve(status);
        };
        const dispose = () => finish('unknown');
        const timer = setTimeout(dispose, request ? 60_000 : 5000);
        timer.unref();
        cancel.add(dispose);
        try {
          bridge ??= createRequire(import.meta.url)(options.bridgePath) as AuthorizationBridge;
          bridge[request ? 'request' : 'query']((value) => {
            const states: NotificationAuthorization[] = [
              'notDetermined',
              'authorized',
              'denied',
              'provisional',
              'unknown',
            ];
            finish(states.includes(value) ? value : 'unknown');
          });
        } catch {
          finish('unknown');
        }
      });
    },
    openSettings: () =>
      shell.openExternal(
        `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=${encodeURIComponent(options.bundleId)}`,
      ),
    deliver(message) {
      return new Promise<NotificationOutcome>((resolve) => {
        if (stopped) {
          resolve('stopped');
          return;
        }
        const notification = new Notification({ ...message, silent: false });
        live.add(notification);
        const finish = (outcome: NotificationOutcome) => {
          if (!cancel.delete(dispose)) return;
          clearTimeout(timer);
          resolve(outcome);
        };
        const dispose = () => finish('stopped');
        const timer = setTimeout(() => finish('unknown'), 5000);
        timer.unref();
        cancel.add(dispose);
        const release = () => {
          live.delete(notification);
          notification.removeAllListeners();
        };
        notification.once('show', () => finish('submitted'));
        notification.once('failed', () => {
          finish('failed');
          release();
        });
        notification.once('click', () => {
          if (!stopped) {
            if (feedback.list().some((item) => item.id === message.id))
              feedback.activate(message.id);
            else options.activate(message.id);
          }
        });
        notification.once('close', () => {
          finish('unknown');
          release();
        });
        try {
          notification.show();
        } catch {
          finish('failed');
          release();
        }
        // 防止无人关闭的系统通知无限保留 JS 对象；已提交通知仍由系统拥有。
        if (live.size > 256) {
          const oldest = live.values().next().value!;
          oldest.removeAllListeners();
          live.delete(oldest);
        }
      });
    },
    dispose() {
      stopped = true;
      for (const stop of [...cancel]) stop();
      for (const notification of live) notification.removeAllListeners();
      live.clear();
    },
  });
  const feedback = new NotificationFeedbackCoordinator({
    publish: (intent) => service.publish(intent),
    changed: () => {
      const window = options.window();
      if (window && !window.isDestroyed() && !window.webContents.isDestroyed())
        try {
          window.webContents.send(notificationChannels.feedbackChanged);
        } catch {
          /* Closing renderers do not own delivery. */
        }
    },
    activate: (target) => options.activateFeedback?.(target),
  });
  const validFeedback = ajv.compile<import('@contracts').NotificationFeedbackRequest>({
    $ref: 'contract#/definitions/NotificationFeedbackRequest',
  });
  ipcMain.handle(notificationChannels.feedback, (event, payload: unknown) => {
    if (!options.trusted(event) || !validFeedback(payload)) throw Error('invalid_request');
    if (payload.action === 'presented') feedback.present(payload.ids);
    if (payload.action === 'activate') feedback.activate(payload.id);
    return feedback.list();
  });
  ipcMain.handle(notificationChannels.request, async (event, value: unknown) => {
    if (!options.trusted(event) || !valid(value)) throw new Error('invalid_request');
    switch (value) {
      case 'status':
        return { outcome: 'submitted', status: await service.getStatus() };
      case 'openSettings':
        return service.openSystemSettings();
    }
  });
  service.on('changed', (status) => {
    const window = options.window();
    if (
      window &&
      !window.isDestroyed() &&
      !window.webContents.isDestroyed() &&
      !window.webContents.isCrashed()
    ) {
      try {
        window.webContents.send(notificationChannels.changed, status);
      } catch {
        /* 关闭中的 Renderer 不拥有通知服务。 */
      }
    }
  });
  const refresh = () => {
    void service.getStatus();
  };
  app.on('browser-window-focus', refresh);
  const dispose = () => {
    app.removeListener('browser-window-focus', refresh);
    app.removeListener('will-quit', dispose);
    feedback.stop();
    ipcMain.removeHandler(notificationChannels.feedback);
    ipcMain.removeHandler(notificationChannels.request);
    service.stop();
  };
  app.once('will-quit', dispose);
  return Object.assign(service, { feedback });
}
