import type { MenuItemConstructorOptions } from 'electron';
import type { NativeMenuCommand, NativeMenuState } from '@contracts';
import type { MessageKey } from '../../shared/i18n/messages';

export interface MenuActions {
  show(): void;
  settings(): void;
  about(): void;
  updates(): void;
  closeWindow(): void;
  editors(): void;
  project(): void;
  issues(): void;
  quit(): void;
  command(command: NativeMenuCommand): void;
}
export const emptyMenuState: NativeMenuState = { enabled: [], checked: [], closeTarget: 'window' };
const closeLabels: Record<NativeMenuState['closeTarget'], MessageKey> = {
  dialog: 'menu_close_dialog',
  page: 'menu_close_page',
  tab: 'menu_close_tab',
  window: 'menu_close_window',
};

export function applicationMenuLayout(
  applicationName: string,
  platform: string,
  t: (key: MessageKey) => string,
  actions: MenuActions,
  state: NativeMenuState = emptyMenuState,
  blocked = false,
): MenuItemConstructorOptions[] {
  const separator: MenuItemConstructorOptions = { type: 'separator' };
  const command = (
    id: NativeMenuCommand,
    key: MessageKey,
    checkbox = false,
    accelerator?: string,
  ): MenuItemConstructorOptions => ({
    id,
    label: t(key),
    type: checkbox ? 'checkbox' : 'normal',
    enabled: !blocked && state.enabled.includes(id),
    ...(checkbox ? { checked: state.checked.includes(id) } : {}),
    ...(accelerator ? { accelerator } : {}),
    click: (item) => {
      // Renderer owns the checked state; clicking the current view must not uncheck it.
      if (checkbox) item.checked = state.checked.includes(id);
      actions.command(id);
    },
  });
  return [
    {
      label: applicationName,
      submenu: [
        { id: 'about', label: t('about_title'), enabled: !blocked, click: actions.about },
        separator,
        {
          id: 'settings',
          label: t('settings') + '…',
          accelerator: 'CmdOrCtrl+,',
          enabled: !blocked,
          click: actions.settings,
        },
        { id: 'updates', label: t('updates_check'), enabled: !blocked, click: actions.updates },
        separator,
        ...(platform === 'darwin'
          ? [
              { label: t('hide'), role: 'hide' as const },
              { label: t('hide_others'), role: 'hideOthers' as const },
              { label: t('show_all'), role: 'unhide' as const },
              separator,
            ]
          : []),
        {
          id: 'quit',
          label: t('quit'),
          accelerator: 'CmdOrCtrl+Q',
          enabled: !blocked,
          click: actions.quit,
        },
      ],
    },
    {
      label: t('menu_connections'),
      submenu: [
        {
          ...command('new', 'ws_new_connection', false, 'CmdOrCtrl+N'),
          label: t('ws_new_connection') + '…',
        },
        command('newGroup', 'menu_new_group'),
        separator,
        command('connect', 'menu_connect_selected'),
        command('connectNew', 'ws_connect_new'),
        command('editConnection', 'menu_edit_selected'),
      ],
    },
    {
      label: t('edit'),
      submenu: [
        { label: t('undo'), role: 'undo' },
        { label: t('redo'), role: 'redo' },
        separator,
        { label: t('cut'), role: 'cut' },
        { label: t('copy'), role: 'copy' },
        { label: t('paste'), role: 'paste' },
        { label: t('select_all'), role: 'selectAll' },
        separator,
        { ...command('find', 'ws_find', false, 'CmdOrCtrl+F'), label: t('ws_find') + '…' },
      ],
    },
    {
      label: t('menu_view'),
      submenu: [
        command('workbench', 'menu_workbench', true),
        command('agentPolicy', 'agent_policy_navigation', true),
        command('agentActivity', 'mcp_activity', true),
        separator,
        command('toggleSidebar', 'menu_sidebar', true),
        command('toggleTools', 'menu_tools', true),
        separator,
        command('showFiles', 'menu_remote_files', true),
        command('showMonitor', 'menu_monitor', true),
        command('showTransfers', 'tools_transfers', true),
        {
          id: 'editors',
          label: t('file_editors_title') + '…',
          enabled: !blocked,
          click: actions.editors,
        },
        separator,
        { label: t('menu_fullscreen'), role: 'togglefullscreen' },
      ],
    },
    {
      label: t('window'),
      role: 'window',
      submenu: [
        { label: t('show_window'), click: actions.show },
        separator,
        command('close', closeLabels[state.closeTarget], false, 'CmdOrCtrl+W'),
        {
          id: 'closeWindow',
          label: t('menu_close_main_window'),
          accelerator: 'CmdOrCtrl+Shift+W',
          enabled: !blocked,
          click: actions.closeWindow,
        },
        separator,
        { label: t('minimize'), role: 'minimize' },
        { label: t('zoom'), role: 'zoom' },
        { label: t('menu_front'), role: 'front' },
      ],
    },
    {
      label: t('menu_help'),
      role: 'help',
      submenu: [
        { label: t('menu_project'), click: actions.project },
        { label: t('menu_issues'), click: actions.issues },
      ],
    },
  ];
}

export function trayMenuLayout(
  t: (key: MessageKey) => string,
  actions: MenuActions,
  blocked = false,
): MenuItemConstructorOptions[] {
  return [
    { label: t('show_window'), click: actions.show },
    {
      label: t('settings'),
      accelerator: 'CmdOrCtrl+,',
      enabled: !blocked,
      click: actions.settings,
    },
    { type: 'separator' },
    { label: t('tray_quit'), accelerator: 'CmdOrCtrl+Q', enabled: !blocked, click: actions.quit },
  ];
}
