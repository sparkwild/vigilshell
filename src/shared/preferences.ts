import type { Preferences } from '@contracts';
/** 与 Rust 默认值对应；后端未就绪时只用于展示，不作为已保存值。 */
export const defaultPreferences: Preferences = {
  systemNotificationMode: 'unfocused',
  attentionRequiredNotificationEnabled: true,
  executionIssueNotificationEnabled: true,
  autoCheckUpdates: true,
  themeMode: 'system',
  localeMode: 'system',
  revision: 0,
  terminalFontSize: 12,
  terminalLineHeight: 14,
  terminalScrollback: 10000,
  terminalCursorStyle: 'block',
  terminalCursorBlink: true,
  connectionOpenBehavior: 'ask',
  filesShowHidden: true,
  filesShowTree: true,
  filesColumns: ['name', 'size', 'kind', 'modified', 'permissions', 'owner'],
  downloadMode: 'ask',
  downloadDirectory: '',
  monitorDisplay: 'bar',
  monitorSampleSeconds: 5,
  monitorFullSeconds: 60,
  terminalCopyOnSelect: false,
  terminalThemeMode: 'inherit',
  terminalFontFamily: 'jetbrains-mono',
  sidebarHoverPreview: true,
  filesAutoOpen: true,
  remoteDirectoryMode: 'initial',
  fileEditorApplication: '',
};
export const fileColumns = ['name', 'size', 'kind', 'modified', 'permissions', 'owner'] as const;

export const scrollbackOptions = [1000, 5000, 10000, 20000, 50000] as const;
export const monitorPresets = [
  {
    value: 'realtime',
    sample: 1,
    full: 60,
    label: 'settings_monitor_realtime',
    description: 'settings_monitor_realtime_description',
  },
  {
    value: 'normal',
    sample: 5,
    full: 60,
    label: 'settings_monitor_normal',
    description: 'settings_monitor_normal_description',
  },
  {
    value: 'slow',
    sample: 15,
    full: 120,
    label: 'settings_monitor_slow',
    description: 'settings_monitor_slow_description',
  },
] as const;
