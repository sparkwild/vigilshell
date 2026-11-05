import type { SystemNotificationsApi } from '../../src/contracts/system-notifications';

export function notificationApiFixture(): SystemNotificationsApi {
  const status = { authorization: 'authorized', version: 1 } as const;
  return {
    getFeedback: async () => [],
    reportPresented: async () => {},
    activate: async () => {},
    onFeedbackChanged: () => () => {},
    onNavigate: () => () => {},
    getStatus: async () => status,
    openSystemSettings: async () => ({ outcome: 'opened', status }),
    onStatusChanged: () => () => {},
  };
}
