import type {
  NotificationFeedback,
  NotificationFeedbackRequest,
  NotificationTarget,
  SystemNotificationCommand,
  SystemNotificationResult,
  SystemNotificationStatus,
} from './generated/runtime';

export type {
  NotificationAuthorization,
  SystemNotificationResult,
  SystemNotificationStatus,
} from './generated/runtime';

/** 系统权限状态与设置入口；不接受来自 Renderer 的通知正文或目标 URL。 */
export interface SystemNotificationsApi {
  getFeedback(): Promise<NotificationFeedback[]>;
  reportPresented(ids: string[]): Promise<void>;
  activate(id: string): Promise<void>;
  onFeedbackChanged(listener: () => void): () => void;
  onNavigate(listener: (target: NotificationTarget) => void): () => void;
  getStatus(): Promise<SystemNotificationStatus>;
  openSystemSettings(): Promise<SystemNotificationResult>;
  onStatusChanged(listener: (status: SystemNotificationStatus) => void): () => void;
}

export const notificationChannels = {
  feedback: 'desktop:notification-feedback',
  feedbackChanged: 'desktop:notification-feedback-changed',
  navigate: 'desktop:notification-navigate',
  request: 'desktop:system-notification',
  changed: 'desktop:system-notification-status',
} as const;

export function createSystemNotificationsApi(
  invoke: (command: SystemNotificationCommand) => Promise<SystemNotificationResult>,
  subscribe: SystemNotificationsApi['onStatusChanged'],
  feedback: (request: NotificationFeedbackRequest) => Promise<NotificationFeedback[]>,
  changed: SystemNotificationsApi['onFeedbackChanged'],
  navigate: SystemNotificationsApi['onNavigate'],
): SystemNotificationsApi {
  return {
    getFeedback: () => feedback({ action: 'list' }),
    reportPresented: async (ids) => {
      await feedback({ action: 'presented', ids });
    },
    activate: async (id) => {
      await feedback({ action: 'activate', id });
    },
    onFeedbackChanged: changed,
    onNavigate: navigate,
    getStatus: async () => (await invoke('status')).status,
    openSystemSettings: () => invoke('openSettings'),
    onStatusChanged: subscribe,
  };
}
