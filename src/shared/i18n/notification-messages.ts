import * as m from './generated/messages';
const messages = {
  mode: m.system_notification_mode,
  issue: m.system_notification_issue,
  issueDescription: m.system_notification_issueDescription,

  permissionRequired: m.system_notification_permissionRequired,
  permissionDescription: m.system_notification_permissionDescription,
  title: m.system_notification_title,
  currentScope: m.system_notification_currentScope,
  off: m.system_notification_off,
  unfocused: m.system_notification_unfocused,
  always: m.system_notification_always,
  attention: m.system_notification_attention,
  attentionDescription: m.system_notification_attentionDescription,
  systemSettings: m.system_notification_systemSettings,
  failed: m.system_notification_failed,
} as const;
export function notificationText(key: keyof typeof messages, locale: 'en' | 'zh-CN'): string {
  return messages[key]({}, { locale });
}
