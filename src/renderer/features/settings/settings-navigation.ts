import {
  BellIcon,
  HistoryIcon,
  Settings2Icon,
  TerminalIcon,
  NetworkIcon,
  FolderIcon,
  ActivityIcon,
} from 'lucide-react';

export const categories = [
  { id: 'general', label: 'general', icon: Settings2Icon },
  { id: 'notifications', label: 'system_notification_title', icon: BellIcon },
  { id: 'terminal', label: 'settings_terminal', icon: TerminalIcon },
  { id: 'connection', label: 'settings_connection', icon: NetworkIcon },
  { id: 'files', label: 'settings_files', icon: FolderIcon },
  { id: 'monitor', label: 'settings_monitor', icon: ActivityIcon },
  { id: 'mcp', label: 'mcp_title', logo: 'mcp' },
  { id: 'agentActivity', label: 'mcp_activity', icon: HistoryIcon },
] as const;

export const columnLabels = {
  name: 'tools_filename',
  size: 'files_size',
  kind: 'files_type',
  modified: 'files_modified',
  permissions: 'files_permissions',
  owner: 'files_owner',
} as const;

export type SettingsCategory = (typeof categories)[number]['id'];
