import { expect, it, vi } from 'vitest';
import type { MenuItemConstructorOptions } from 'electron';
import { translate } from '../../shared/i18n/messages';
import { applicationMenuLayout, trayMenuLayout, type MenuActions } from './layout';

function fixture(locale: 'en' | 'zh-CN' = 'en') {
  const actions: MenuActions = {
    show: vi.fn(),
    settings: vi.fn(),
    about: vi.fn(),
    updates: vi.fn(),
    closeWindow: vi.fn(),
    editors: vi.fn(),
    project: vi.fn(),
    issues: vi.fn(),
    quit: vi.fn(),
    command: vi.fn(),
  };
  const menu = applicationMenuLayout(
    'VigilShell Dev',
    'darwin',
    (key) => translate(key, locale),
    actions,
    {
      enabled: ['new', 'connect', 'toggleSidebar', 'close'],
      checked: ['toggleSidebar'],
      closeTarget: 'tab',
    },
  );
  const items = menu.flatMap((group) => group.submenu as MenuItemConstructorOptions[]);
  return { menu, actions, items };
}

it('separates connection commands, focused editing, view navigation and window actions', () => {
  const { menu, items, actions } = fixture();
  expect(menu.map((group) => group.label)).toEqual([
    'VigilShell Dev',
    'Connections',
    'Edit',
    'View',
    'Window',
    'Help',
  ]);
  const connections = menu[1]!.submenu as MenuItemConstructorOptions[];
  expect(connections.find((item) => item.id === 'new')?.accelerator).toBe('CmdOrCtrl+N');
  expect((menu[2]!.submenu as MenuItemConstructorOptions[]).some((item) => item.id === 'new')).toBe(
    false,
  );
  expect(items.find((item) => item.id === 'connect')?.enabled).toBe(true);
  expect(items.find((item) => item.id === 'connectNew')?.enabled).toBe(false);
  expect(items.find((item) => item.id === 'toggleSidebar')?.checked).toBe(true);
  const toggled = { checked: false } as Electron.MenuItem;
  (items.find((item) => item.id === 'toggleSidebar')!.click as (item: Electron.MenuItem) => void)(
    toggled,
  );
  expect(toggled.checked).toBe(true);
  (items.find((item) => item.id === 'connect')!.click as () => void)();
  expect(actions.command).toHaveBeenLastCalledWith('connect');
  expect(actions.closeWindow).not.toHaveBeenCalled();
  expect(items.find((item) => item.id === 'close')?.label).toBe('Close Tab');
  expect(items.find((item) => item.id === 'closeWindow')?.accelerator).toBe('CmdOrCtrl+Shift+W');
});

it('keeps native edit shortcuts and translates menu labels without changing the command identity', () => {
  const { menu, items } = fixture('zh-CN');
  expect(menu.map((group) => group.label)).toEqual([
    'VigilShell Dev',
    '连接',
    '编辑',
    '视图',
    '窗口',
    '帮助',
  ]);
  expect(items.find((item) => item.id === 'close')?.label).toBe('关闭标签页');
  expect(
    (menu[2]!.submenu as MenuItemConstructorOptions[])
      .filter((item) => item.role)
      .map((item) => item.role),
  ).toEqual(['undo', 'redo', 'cut', 'copy', 'paste', 'selectAll']);
});

it('disables business actions during installation and keeps the tray layout independent', () => {
  const { actions } = fixture();
  const menu = applicationMenuLayout(
    'VigilShell',
    'darwin',
    (key) => translate(key, 'en'),
    actions,
    { enabled: ['new', 'close'], checked: [], closeTarget: 'page' },
    true,
  );
  const items = menu.flatMap((group) => group.submenu as MenuItemConstructorOptions[]);
  for (const id of ['new', 'close', 'closeWindow', 'settings', 'updates', 'editors', 'quit'])
    expect(items.find((item) => item.id === id)?.enabled).toBe(false);
  expect(items.find((item) => item.id === 'close')?.label).toBe('Return to Workbench');
  const tray = trayMenuLayout((key) => translate(key, 'zh-CN'), actions);
  expect(tray.map((item) => item.label).filter(Boolean)).toEqual(['显示主窗口', '设置', '退出']);
});
