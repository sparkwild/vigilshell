import { beforeEach, expect, it, vi } from 'vitest';
import type { MenuItemConstructorOptions } from 'electron';
const fixture = vi.hoisted(() => ({
  receive: undefined as undefined | ((event: unknown, value: unknown) => void),
  templates: [] as MenuItemConstructorOptions[][],
  setMenu: vi.fn(),
}));
vi.mock('electron', () => ({
  Menu: {
    buildFromTemplate: (template: MenuItemConstructorOptions[]) => {
      fixture.templates.push(template);
      return template;
    },
    setApplicationMenu: fixture.setMenu,
  },
  ipcMain: {
    on: (_name: string, listener: typeof fixture.receive) => {
      fixture.receive = listener;
    },
  },
}));
import { NativeMenus } from './controller';
import type { MenuActions } from './layout';
beforeEach(() => {
  fixture.templates = [];
  fixture.setMenu.mockClear();
});
it('rejects invalid and foreign renderer state and does not replay a stale enabled command', () => {
  const command = vi.fn();
  const actions: MenuActions = {
    command,
    show: vi.fn(),
    settings: vi.fn(),
    about: vi.fn(),
    updates: vi.fn(),
    closeWindow: vi.fn(),
    editors: vi.fn(),
    project: vi.fn(),
    issues: vi.fn(),
    quit: vi.fn(),
  };
  const menus = new NativeMenus('VigilShell', actions, (event) => event === trusted);
  const trusted = {} as Electron.IpcMainEvent;
  menus.update('en');
  const initial = fixture.templates.length;
  fixture.receive!({}, { enabled: ['connect'], checked: [], closeTarget: 'tab' });
  fixture.receive!(trusted, { enabled: ['runArbitraryCommand'], checked: [], closeTarget: 'tab' });
  expect(fixture.templates).toHaveLength(initial);
  fixture.receive!(trusted, { enabled: ['connect'], checked: [], closeTarget: 'tab' });
  const connect = fixture.templates
    .at(-1)!
    .flatMap((group) => group.submenu as MenuItemConstructorOptions[])
    .find((item) => item.id === 'connect')!;
  (connect.click as () => void)();
  expect(command).toHaveBeenCalledExactlyOnceWith('connect');
  menus.reset();
  (connect.click as () => void)();
  expect(command).toHaveBeenCalledTimes(1);
  fixture.receive!(trusted, { enabled: ['connect'], checked: [], closeTarget: 'tab' });
  menus.update('zh-CN', true);
  (connect.click as () => void)();
  expect(command).toHaveBeenCalledTimes(1);
  const count = fixture.templates.length;
  menus.update('zh-CN', true);
  expect(fixture.templates).toHaveLength(count);
});
