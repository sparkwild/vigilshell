// @vitest-environment jsdom
import { defaultPreferences } from '../../../shared/preferences';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import type { DesktopSnapshot } from '@contracts';
import { DesktopProvider } from '../../app/DesktopProvider';
import { createDesktopStore, type SettingsBridge } from '../../shared/platform/desktop-store';
import { SettingsView } from './SettingsView';
vi.mock('../terminal/terminal-fonts', async () => {
  const actual = await vi.importActual<typeof import('../terminal/terminal-fonts')>(
    '../terminal/terminal-fonts',
  );
  return {
    ...actual,
    availableTerminalFonts: () => actual.terminalFonts.slice(0, 2),
  };
});
const stores = new Set<ReturnType<typeof createDesktopStore>>();
afterEach(() => {
  cleanup();
  for (const store of stores) store.dispose();
  stores.clear();
});
beforeEach(() => {
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      mcp: {
        request: async () => ({
          ok: true,
          snapshot: {
            settings: {
              revision: 0,
              enabled: false,
              port: 51602,
              showTray: true,
              retentionDays: 30,
              storageMib: 256,
            },
            entries: [],
            policies: [],
            contexts: [],
            clients: [],
            observations: [],
            endpoint: 'http://127.0.0.1:51602/mcp',
            running: false,
            error: null,
          },
        }),
        onChanged: () => () => {},
        onNavigate: () => () => {},
      },
    },
  });

  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn().mockImplementation(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
it('changes the whole view immediately without losing focus; shows failed saves without changing preferences', async () => {
  let current: DesktopSnapshot = {
    version: 1,
    save: { phase: 'idle', error: null },
    error: null,
    preferences: {
      ...defaultPreferences,
      themeMode: 'light',
      localeMode: 'en',
      revision: 0,
    },
    runtime: { status: 'ready', instanceId: 'runtime' },
    appearance: { theme: 'light', locale: 'en', reducedTransparency: false },
  };
  let receive: (snapshot: DesktopSnapshot) => void = () => {},
    fail = false;
  let completeFailure: (() => void) | undefined;
  const update = vi.fn(async (request): Promise<DesktopSnapshot> => {
    if (fail)
      return new Promise((resolve) => {
        completeFailure = () =>
          resolve({
            ...current,
            error: 'storage_unavailable',
            save: { phase: 'idle', error: 'storage_unavailable' },
          });
      });
    const themeMode = request.patch.themeMode ?? current.preferences!.themeMode;
    const localeMode = request.patch.localeMode ?? current.preferences!.localeMode;
    current = {
      ...current,
      version: current.version + 1,
      preferences: {
        ...defaultPreferences,
        themeMode,
        localeMode,
        revision: current.preferences!.revision + 1,
      },
      appearance: {
        theme: themeMode === 'system' ? 'light' : themeMode,
        locale: localeMode === 'system' ? 'en' : localeMode,
        reducedTransparency: false,
      },
    };
    receive(current);
    return current;
  });
  const bridge: SettingsBridge = {
    chooseDownloadDirectory: async () => current,
    getSnapshot: async () => current,
    onSnapshotChanged: (listener) => {
      receive = listener;
      return () => {};
    },
    updatePreferences: update,
    retryRuntime: async () => current,
    reconcilePreferences: async () => current,
    restartBackend: async () => current,
    rendererReady: () => {},
    onOpenSettings: () => () => {},
  };
  const store = createDesktopStore(bridge);
  stores.add(store);
  await store.initialize();
  render(
    <DesktopProvider store={store}>
      <SettingsView onBack={() => {}} />
    </DesktopProvider>,
  );
  const user = userEvent.setup();
  const language = screen.getByRole('combobox', { name: 'Language' });
  await user.click(language);
  await user.click(await screen.findByRole('option', { name: '简体中文' }));
  expect(await screen.findByRole('heading', { name: '通用' })).toBeVisible();
  await waitFor(() => expect(language).toHaveFocus());
  expect(document.documentElement.lang).toBe('zh-CN');
  expect(update).toHaveBeenCalledTimes(1);
  fail = true;
  await user.click(screen.getByRole('combobox', { name: '主题' }));
  await user.click(await screen.findByRole('option', { name: '深色' }));
  expect(screen.getByRole('combobox', { name: '主题' })).toBeDisabled();
  expect(language).toBeDisabled();
  await act(async () => completeFailure?.());
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('设置未保存'));
  expect(screen.getByRole('button', { name: '重试' })).toBeEnabled();
  expect(
    screen.getByRole('combobox', { name: '主题' }).querySelector('[data-slot="select-value"]'),
  ).toHaveTextContent('浅色');
  await user.click(screen.getByRole('combobox', { name: '主题' }));
  await user.click(await screen.findByRole('option', { name: '浅色' }));
  expect(update).toHaveBeenCalledTimes(2);
  expect(document.documentElement).not.toHaveClass('dark');
  expect(screen.queryByRole('button', { name: '重试' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('combobox', { name: '主题' }));
  await user.click(await screen.findByRole('option', { name: '深色' }));
  await act(async () => completeFailure?.());
  fail = false;
  await user.click(screen.getByRole('button', { name: '重试' }));
  await waitFor(() => expect(update).toHaveBeenCalledTimes(4));
  expect(document.documentElement).toHaveClass('dark');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

async function renderSettings() {
  let current: DesktopSnapshot = {
    version: 1,
    save: { phase: 'idle', error: null },
    error: null,
    preferences: { ...defaultPreferences },
    runtime: { status: 'ready', instanceId: 'runtime' },
    appearance: { theme: 'light', locale: 'zh-CN', reducedTransparency: false },
  };
  let cancelled = true;
  let monitorSaveFails = false;
  const chooseDownloadDirectory = vi.fn(async () => {
    if (!cancelled)
      current = {
        ...current,
        version: current.version + 1,
        preferences: {
          ...current.preferences!,
          downloadMode: 'directory',
          downloadDirectory: '/tmp/Download files',
          revision: current.preferences!.revision + 1,
        },
      };
    return current;
  });
  const bridge: SettingsBridge = {
    getSnapshot: async () => current,
    onSnapshotChanged: () => () => {},
    updatePreferences: async (request) => {
      if (monitorSaveFails)
        return {
          ...current,
          error: 'storage_unavailable',
          save: { phase: 'idle', error: 'storage_unavailable' },
        };
      current = {
        ...current,
        version: current.version + 1,
        preferences: {
          ...current.preferences!,
          ...request.patch,
          revision: current.preferences!.revision + 1,
        },
      };
      return current;
    },
    chooseDownloadDirectory,
    retryRuntime: async () => current,
    reconcilePreferences: async () => current,
    restartBackend: async () => current,
    rendererReady: () => {},
    onOpenSettings: () => () => {},
  };
  const store = createDesktopStore(bridge);
  stores.add(store);
  await store.initialize();
  render(
    <DesktopProvider store={store}>
      <SettingsView onBack={() => {}} />
    </DesktopProvider>,
  );
  const user = userEvent.setup();
  return {
    user,
    chooseDownloadDirectory,
    get current() {
      return current;
    },
    set cancelled(value: boolean) {
      cancelled = value;
    },
    set monitorSaveFails(value: boolean) {
      monitorSaveFails = value;
    },
  };
}

it('shows a destination only for fixed-folder downloads and preserves ask mode when the picker is cancelled', async () => {
  const fixture = await renderSettings();
  const { user, chooseDownloadDirectory } = fixture;
  await user.click(screen.getByRole('button', { name: '文件与传输' }));
  expect(screen.queryByRole('button', { name: '更改…' })).not.toBeInTheDocument();
  expect(screen.queryByLabelText('保存文件夹')).not.toBeInTheDocument();
  const mode = screen.getByRole('combobox', { name: '下载位置' });
  await user.click(mode);
  await user.click(await screen.findByRole('option', { name: '保存到指定文件夹' }));
  await waitFor(() => expect(chooseDownloadDirectory).toHaveBeenCalledTimes(1));
  expect(mode).toHaveTextContent('每次下载时询问');
  expect(screen.queryByLabelText('保存文件夹')).not.toBeInTheDocument();
  fixture.cancelled = false;
  await user.click(mode);
  await user.click(await screen.findByRole('option', { name: '保存到指定文件夹' }));
  const folder = await screen.findByLabelText('保存文件夹');
  expect(folder).toHaveValue('/tmp/Download files');
  expect(folder).toHaveAttribute('readonly');
  expect(screen.getByRole('button', { name: '更改…' })).toBeVisible();
  await user.click(mode);
  await user.click(await screen.findByRole('option', { name: '每次下载时询问' }));
  await waitFor(() => expect(screen.queryByLabelText('保存文件夹')).not.toBeInTheDocument());
  expect(fixture.current.preferences!.downloadDirectory).toBe('/tmp/Download files');
});

it('persists cursor settings and rejects invalid terminal font sizes', async () => {
  const fixture = await renderSettings();
  const { user } = fixture;
  await user.click(screen.getByRole('button', { name: '终端' }));
  const blink = screen.getByRole('switch', { name: '光标闪烁' });
  expect(blink).toHaveAccessibleDescription('开启后，终端输入光标会闪烁');
  await user.click(blink);
  await waitFor(() => expect(fixture.current.preferences!.terminalCursorBlink).toBe(false));
  expect(blink).not.toBeChecked();
  const cursor = screen.getByRole('combobox', { name: '光标样式' });
  expect(cursor).toHaveAccessibleDescription('选择终端输入光标的形状');
  const fontSize = screen.getByRole('spinbutton', { name: /^终端字号/ });
  await user.clear(fontSize);
  await user.type(fontSize, '99{Enter}');
  expect(fontSize).toHaveAttribute('aria-invalid', 'true');
  expect(fontSize).toHaveAccessibleDescription(/10.*24/);
  expect(fixture.current.preferences!.terminalFontSize).toBe(12);
  await user.keyboard('{Escape}');
  expect(fontSize).toHaveValue(12);
  expect(fontSize).toHaveAttribute('aria-invalid', 'false');

  await user.click(cursor);
  await user.click(await screen.findByRole('option', { name: '竖线' }));
  await waitFor(() => expect(fixture.current.preferences!.terminalCursorStyle).toBe('bar'));
  expect(screen.getByRole('combobox', { name: '回滚缓冲行数' })).toHaveTextContent('10000');
});

it('preserves required file columns and switches remote directory opening behavior', async () => {
  const fixture = await renderSettings();
  const { user } = fixture;
  await user.click(screen.getByRole('button', { name: '文件与传输' }));
  expect(screen.getByRole('switch', { name: '显示隐藏文件' })).toBeChecked();
  expect(screen.getByRole('switch', { name: '目录树' })).toBeChecked();
  expect(screen.getByRole('button', { name: '名称', pressed: true })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: '权限', pressed: true }));
  await waitFor(() =>
    expect(fixture.current.preferences!.filesColumns).not.toContain('permissions'),
  );
  expect(fixture.current.preferences!.filesColumns).toContain('name');
  await user.click(screen.getByRole('button', { name: '权限', pressed: false }));
  await waitFor(() => expect(fixture.current.preferences!.filesColumns).toContain('permissions'));
  await user.click(screen.getByRole('switch', { name: '文件区默认展开' }));
  expect(fixture.current.preferences!.filesAutoOpen).toBe(false);
  const openingLocation = screen.getByRole('combobox', { name: '远程目录打开位置' });
  expect(openingLocation).toHaveTextContent('初始目录');
  await user.click(openingLocation);
  expect((await screen.findAllByRole('option')).map((option) => option.textContent)).toEqual([
    '初始目录',
    '最后打开',
  ]);
  await user.click(screen.getByRole('option', { name: '最后打开' }));
  expect(fixture.current.preferences!.remoteDirectoryMode).toBe('last');
  await user.click(openingLocation);
  await user.click(await screen.findByRole('option', { name: '初始目录' }));
  expect(fixture.current.preferences!.remoteDirectoryMode).toBe('initial');
});

it('persists sidebar hover preview preferences', async () => {
  const fixture = await renderSettings();
  const { user } = fixture;
  await user.click(screen.getByRole('button', { name: '通用' }));
  await user.click(screen.getByRole('switch', { name: '侧栏悬停预览' }));
  expect(fixture.current.preferences!.sidebarHoverPreview).toBe(false);
});

it('keeps terminal appearance preferences independent from the application theme', async () => {
  const fixture = await renderSettings();
  const { user } = fixture;
  await user.click(screen.getByRole('button', { name: '终端' }));
  await user.click(screen.getByRole('switch', { name: '选中即复制' }));
  expect(fixture.current.preferences!.terminalCopyOnSelect).toBe(true);
  await user.click(screen.getByRole('combobox', { name: '终端明暗主题' }));
  await user.click(await screen.findByRole('option', { name: '深色' }));
  expect(fixture.current.preferences!.terminalThemeMode).toBe('dark');
  expect(fixture.current.preferences!.themeMode).toBe('system');
  await user.click(screen.getByRole('combobox', { name: '终端字体' }));
  await user.click(await screen.findByRole('option', { name: 'Menlo' }));
  expect(fixture.current.preferences!.terminalFontFamily).toBe('menlo');
});

it('persists monitor speed and display and restores the saved speed after a failed save', async () => {
  const fixture = await renderSettings();
  const { user } = fixture;
  await user.click(screen.getByRole('button', { name: '服务器监控' }));
  expect(screen.queryByRole('combobox', { name: '刷新方式' })).not.toBeInTheDocument();
  const speed = screen.getByRole('slider', { name: '监控更新速度' });
  expect(speed).toHaveAttribute('aria-valuetext', '一般');
  for (const [key, label, sample, full] of [
    ['{Home}', '实时', 1, 60],
    ['{End}', '较慢', 15, 120],
    ['{ArrowLeft}', '一般', 5, 60],
  ] as const) {
    speed.focus();
    await user.keyboard(key);
    await waitFor(() => expect(fixture.current.preferences!.monitorSampleSeconds).toBe(sample));
    expect(fixture.current.preferences!.monitorFullSeconds).toBe(full);
    expect(speed).toHaveAttribute('aria-valuetext', label);
  }
  expect(screen.getByRole('tab', { name: '条形', selected: true })).toBeVisible();
  await user.click(screen.getByRole('tab', { name: '图表' }));
  await waitFor(() => expect(fixture.current.preferences!.monitorDisplay).toBe('chart'));
  expect(screen.getByRole('tab', { name: '图表', selected: true })).toBeVisible();
  await user.click(screen.getByRole('tab', { name: '条形' }));
  await waitFor(() => expect(fixture.current.preferences!.monitorDisplay).toBe('bar'));
  fixture.monitorSaveFails = true;
  speed.focus();
  await user.keyboard('{Home}');
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('设置未保存'));
  expect(fixture.current.preferences!.monitorSampleSeconds).toBe(5);
  expect(speed).toHaveAttribute('aria-valuetext', '一般');
  expect(speed).toHaveAccessibleDescription('兼顾更新及时性与资源开销，适合日常监控');
});
