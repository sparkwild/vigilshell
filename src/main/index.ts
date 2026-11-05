import { observeFileEditingNotifications } from './notifications/file-editing';
import { observeRuntimeNotifications } from './notifications/runtime';
import { registerFileEditors } from './file-editing/electron';
import type { FileEditingService } from './file-editing/service';
import { registerMcp } from './mcp/electron';
import type { McpController } from './mcp/controller';
import { registerSystemNotifications } from './notifications/electron';
import { UiStateService } from './platform/ui-state';
import { WorkspaceIpc } from './ipc/workspace';
import {
  app,
  BrowserWindow,
  ipcMain,
  Menu,
  Tray,
  nativeImage,
  nativeTheme,
  session,
  powerMonitor,
  dialog,
} from 'electron';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { readFileSync, mkdirSync, realpathSync } from 'node:fs';
import type { DesktopSnapshot } from '@contracts';
import { BackendClient } from './backend/client';
import { DesktopState } from './platform/desktop-state';
import { resolveLaunchContext } from './platform/launch-context';
import { RendererStartup } from './windows/renderer-startup';
import { translate, type MessageKey } from '../shared/i18n/messages';

const launch = resolveLaunchContext({
  appPath: app.getAppPath(),
  resourcesPath: process.resourcesPath,
  appDataPath: app.getPath('appData'),
  isPackaged: app.isPackaged,
  developmentPackage: process.env.VIGILSHELL_DEV_PACKAGE_BUILD === '1',
  developmentBuild: process.env.VIGILSHELL_DEVELOPMENT_BUILD === '1',
  environment: process.env,
});
app.setName(launch.applicationName);
mkdirSync(launch.browserDirectory, { recursive: true });
const dataDirectory = realpathSync(launch.dataDirectory);
app.setPath('userData', dataDirectory);
app.setPath('sessionData', join(dataDirectory, 'browser'));
if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  let window: BrowserWindow | undefined,
    tray: Tray | undefined,
    state: DesktopState | undefined,
    workspace: WorkspaceIpc | undefined,
    uiState: UiStateService | undefined,
    mcp: McpController | undefined;
  let confirmingQuit = false;
  let quitting = false,
    pendingSettings: boolean | 'notifications' = false,
    startup: RendererStartup | undefined;
  const icons = launch.iconDirectory;
  const applicationIcon = join(icons, 'app-icon-source.png');
  const page = join(import.meta.dirname, '../renderer/index.html');
  const developmentUrl = launch.developmentUrl;
  const allowedUrl = developmentUrl ?? pathToFileURL(page).href;
  const senderAllowed = (event: Electron.IpcMainEvent | Electron.IpcMainInvokeEvent) => {
    if (
      !window ||
      event.sender !== window.webContents ||
      event.senderFrame !== window.webContents.mainFrame
    )
      return false;
    try {
      const url = new URL(event.senderFrame.url),
        expected = new URL(allowedUrl);
      return (
        url.origin === expected.origin &&
        (developmentUrl ? url.pathname === expected.pathname : url.href === expected.href)
      );
    } catch {
      return false;
    }
  };
  function setTrayVisible(visible: boolean) {
    if (!visible) {
      tray?.destroy();
      tray = undefined;
      return;
    }
    if (tray || quitting) return;
    const icon = nativeImage
      .createFromPath(join(icons, 'tray-icon.png'))
      .resize({ width: 18, height: 18 });
    if (process.platform === 'darwin') icon.setTemplateImage(true);
    tray = new Tray(icon);
    tray.on('double-click', () => showWindow());
    if (state) menus(state.getSnapshot());
  }
  let pendingNotificationNavigation: import('@contracts').NotificationTarget | undefined;
  let pendingTransferNavigation: import('@contracts').TransferNotificationTarget | undefined;
  function showWindow(settings: boolean | 'notifications' = false) {
    if (quitting) return;
    if (settings) pendingSettings = settings;
    if (!window) {
      void createWindow();
      return;
    }
    startup?.requestShow();
  }
  function revealWindow() {
    if (quitting || !window || window.isDestroyed()) return;
    if (window.isMinimized()) window.restore();
    window.show();
    window.focus();
    if (pendingNotificationNavigation) {
      window.webContents.send('desktop:notification-navigate', pendingNotificationNavigation);
      pendingNotificationNavigation = undefined;
    }
    if (pendingTransferNavigation) {
      window.webContents.send('workspace:transfer-navigation', pendingTransferNavigation);
      pendingTransferNavigation = undefined;
    }
    if (pendingSettings) {
      window.webContents.send(
        'desktop:open-settings',
        pendingSettings === 'notifications' ? 'notifications' : undefined,
      );
      pendingSettings = false;
    }
  }
  async function createWindow() {
    if (quitting || window || !state) return;
    const snapshot = state.getSnapshot();
    const next = new BrowserWindow({
      // 两产品共同骨架尺寸；调整时同步 Watchdog 与 docs/UI_DESIGN.md。
      width: 1280,
      height: 820,
      minWidth: 960,
      minHeight: 640,
      show: false,
      title: 'VigilShell',
      backgroundColor:
        process.platform === 'darwin' && !nativeTheme.prefersReducedTransparency
          ? '#00000000'
          : snapshot.appearance.theme === 'dark'
            ? '#262626'
            : '#ffffff',
      vibrancy:
        process.platform === 'darwin' && !nativeTheme.prefersReducedTransparency
          ? 'sidebar'
          : undefined,
      visualEffectState: 'active',
      titleBarStyle: 'hidden',
      ...(process.platform === 'darwin'
        ? { trafficLightPosition: { x: 16, y: 13 } }
        : { titleBarOverlay: { height: 40 } }),
      icon: applicationIcon,
      webPreferences: {
        additionalArguments: [
          `--vigilshell-startup=${encodeURIComponent(
            JSON.stringify({
              theme: snapshot.appearance.theme,
              locale: snapshot.appearance.locale,
              label: translate('starting', snapshot.appearance.locale),
            }),
          )}`,
        ],
        backgroundThrottling: false,
        preload: join(import.meta.dirname, '../preload/index.cjs'),
        contextIsolation: true,
        sandbox: true,
        nodeIntegration: false,
        webviewTag: false,
        webSecurity: true,
        spellcheck: false,
      },
    });
    next.webContents.on('did-start-navigation', (_event, _url, isInPlace, isMainFrame) => {
      if (isMainFrame && !isInPlace) void workspace?.navigation();
    });
    next.webContents.on('render-process-gone', () => void workspace?.navigation());
    next.on('hide', () => {
      lifecycle.hidden();
      workspace?.cancelPrompts();
    });
    window = next;
    const lifecycle = new RendererStartup({
      revealLoading: () => {
        if (quitting || next.isDestroyed()) return;
        if (next.isMinimized()) next.restore();
        next.show();
      },
      reveal: revealWindow,
      load: () => next.loadURL(allowedUrl),
      report: (failure) => console.error('renderer_startup_failed', failure),
      prompt: async (failure, signal) => {
        const t = (key: MessageKey) => translate(key, state!.getSnapshot().appearance.locale);
        // 模块导入或 Preload 失败时页面无法绘制提示，因此恢复入口由 Main 原生提供。
        if (next.isMinimized()) next.restore();
        next.show();
        const result = await dialog.showMessageBox(next, {
          type: 'error',
          title: launch.applicationName,
          message: t(
            failure.code === 'renderer_startup_timeout'
              ? 'renderer_startup_timeout'
              : failure.code === 'renderer_process_gone'
                ? 'renderer_interrupted'
                : 'renderer_startup_failed',
          ),
          detail: t('renderer_startup_failed_body'),
          buttons: [t('renderer_reload')],
          defaultId: 0,
          cancelId: 0,
          noLink: true,
          // 有父窗口的原生 sheet 不阻塞 Main，且支持取消过期提示。
          signal,
        });
        return result.response === 0 ? 'retry' : 'dismiss';
      },
    });
    startup = lifecycle;
    next.once('ready-to-show', () => lifecycle.windowReady());
    next.webContents.on('did-start-loading', () => lifecycle.begin());
    next.webContents.on('preload-error', (_event, _path, error) => {
      lifecycle.fail('renderer_preload_failed', error.message);
    });
    next.webContents.on('console-message', (event) => {
      if (event.level === 'error') lifecycle.fail('renderer_script_failed', event.message);
    });
    next.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    next.webContents.on('will-navigate', (event, url) => {
      if (url !== allowedUrl) event.preventDefault();
    });
    next.on('close', (event) => {
      if (!quitting) {
        event.preventDefault();
        next.hide();
      }
    });
    next.on('closed', () => {
      if (window === next) {
        window = undefined;
        startup = undefined;
      }
      lifecycle.dispose();
    });
    next.on('focus', () => state?.refreshSystem());
    next.webContents.on('render-process-gone', (_event, details) => {
      if (!quitting) lifecycle.crashed(details.reason);
    });
    next.webContents.on('did-fail-load', (_event, code, description, _url, isMainFrame) => {
      if (isMainFrame && code !== -3)
        lifecycle.fail('renderer_load_failed', `${code}: ${description}`);
    });
    lifecycle.begin();
    try {
      if (developmentUrl) await next.loadURL(developmentUrl);
      else await next.loadFile(page);
    } catch (error) {
      lifecycle.fail('renderer_load_failed', error instanceof Error ? error.message : '');
    }
  }
  function menus(snapshot: DesktopSnapshot) {
    if (quitting) return;
    const t = (key: MessageKey) => translate(key, snapshot.appearance.locale);
    const settings: Electron.MenuItemConstructorOptions = {
      label: t('settings') + '…',
      accelerator: 'CmdOrCtrl+,',
      click: () => showWindow(true),
    };
    const quit: Electron.MenuItemConstructorOptions = {
      label: t('quit'),
      accelerator: 'CmdOrCtrl+Q',
      click: () => app.quit(),
    };
    const application: Electron.MenuItemConstructorOptions[] = [
      { label: t('show_window'), click: () => showWindow() },
      settings,
      { type: 'separator' },
    ];
    if (process.platform === 'darwin')
      application.push(
        { label: t('hide'), role: 'hide' },
        { label: t('hide_others'), role: 'hideOthers' },
        { label: t('show_all'), role: 'unhide' },
        { type: 'separator' },
      );
    application.push(quit);
    Menu.setApplicationMenu(
      Menu.buildFromTemplate([
        { label: launch.applicationName, submenu: application },
        {
          label: t('edit'),
          submenu: [
            {
              label: t('ws_new_connection'),
              accelerator: 'CmdOrCtrl+N',
              click: () => {
                showWindow();
                workspace?.command('new');
              },
            },
            {
              label: t('ws_find'),
              accelerator: 'CmdOrCtrl+F',
              click: () => workspace?.command('find'),
            },
            { type: 'separator' },
            { label: t('undo'), role: 'undo' },
            { label: t('redo'), role: 'redo' },
            { type: 'separator' },
            { label: t('cut'), role: 'cut' },
            { label: t('copy'), role: 'copy' },
            { label: t('paste'), role: 'paste' },
            { label: t('select_all'), role: 'selectAll' },
          ],
        },
        {
          label: t('window'),
          submenu: [
            {
              label: t('file_editors_title'),
              click: () => {
                showWindow();
                fileEditors?.show();
              },
            },
            { label: t('minimize'), role: 'minimize' },
            { label: t('zoom'), role: 'zoom' },
            {
              label: t('close'),
              accelerator: 'CmdOrCtrl+W',
              click: () => workspace?.command('close'),
            },
          ],
        },
      ]),
    );
    tray?.setContextMenu(
      Menu.buildFromTemplate([
        { label: t('show_window'), click: () => showWindow() },
        settings,
        { type: 'separator' },
        quit,
      ]),
    );
    tray?.setToolTip(launch.applicationName);
  }
  let fileEditors: FileEditingService | undefined;
  let stopNotifications: (() => void) | undefined;
  async function quit() {
    if (quitting) return;
    if (confirmingQuit) return;
    if (
      workspace &&
      (workspace.activeCount ||
        workspace.activeTransferCount ||
        workspace.ui.dirty ||
        mcp?.activeCount ||
        fileEditors?.activeCount)
    ) {
      confirmingQuit = true;
      showWindow();
      const locale = state?.getSnapshot().appearance.locale ?? 'en';
      const t = (key: MessageKey) => translate(key, locale);
      const options: Electron.MessageBoxOptions = {
        type: 'question',
        message: t('ws_quit_title'),
        detail:
          t('ws_quit_body') +
          '\n' +
          t('ws_affected_sessions') +
          ': ' +
          workspace.activeCount +
          '\n' +
          t('tools_transfers') +
          ': ' +
          workspace.activeTransferCount +
          '\n' +
          t('file_editors_title') +
          ': ' +
          (fileEditors?.activeCount ?? 0) +
          '\nAgent MCP: ' +
          (mcp?.activeCount ?? 0),
        buttons: [
          t('ws_cancel'),
          workspace.activeTransferCount ? t('tools_cancel_and_quit') : t('quit'),
        ],
        defaultId: 0,
        cancelId: 0,
        noLink: true,
      };
      try {
        const result = window
          ? await dialog.showMessageBox(window, options)
          : await dialog.showMessageBox(options);
        if (result.response !== 1) return;
        if (workspace.activeTransferCount) await workspace.cancelTransfers();
      } catch {
        const failure = {
          type: 'error' as const,
          message: t('tools_cancel_failed'),
          buttons: [t('ws_confirm')],
        };
        if (window) await dialog.showMessageBox(window, failure);
        else await dialog.showMessageBox(failure);
        return;
      } finally {
        confirmingQuit = false;
      }
    }
    quitting = true;
    stopNotifications?.();
    await fileEditors?.stop();
    startup?.dispose();
    const mcpStopping = mcp?.stop();
    await uiState?.flush();
    await Promise.allSettled([mcpStopping, state?.stop()]);
    tray?.destroy();
    app.exit(0);
  }
  app.on('before-quit', (event) => {
    event.preventDefault();
    void quit();
  });
  if (launch.nativeDevelopment)
    for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP'] as const)
      process.on(signal, () => app.quit());
  app.on('window-all-closed', () => {});
  app.on('activate', () => {
    state?.refreshSystem();
    showWindow();
  });
  app.on('second-instance', () => showWindow());
  app
    .whenReady()
    .then(async () => {
      if (launch.developmentDockIcon && process.platform === 'darwin')
        app.dock?.setIcon(launch.developmentDockIcon);
      const binary = launch.backendExecutable;
      if (launch.verifyBackendManifest) {
        try {
          const manifest = JSON.parse(
            readFileSync(join(process.resourcesPath, 'backend', 'manifest.json'), 'utf8'),
          );
          if (manifest.arch !== process.arch || manifest.platform !== process.platform)
            throw Error('host_platform_mismatch');
        } catch (error) {
          throw new Error('Packaged Host manifest is missing or incompatible', {
            cause: error,
          });
        }
      }
      const host = new BackendClient(binary, dataDirectory);
      uiState = new UiStateService(host);
      workspace = new WorkspaceIpc(host, () => window, senderAllowed, {
        state: () => state?.getSnapshot(),
        ui: uiState,
      });
      uiState.on('changed', (notice) => window?.webContents.send('desktop:ui-changed', notice));
      state = new DesktopState(host, {
        preferredLanguages: () => app.getPreferredSystemLanguages(),
        setThemeMode: (mode) => {
          if (nativeTheme.themeSource !== mode) nativeTheme.themeSource = mode;
        },
        currentTheme: () => (nativeTheme.shouldUseDarkColors ? 'dark' : 'light'),
        reducedTransparency: () => nativeTheme.prefersReducedTransparency,
      });
      fileEditors = registerFileEditors({
        host,
        workspace,
        state,
        dataDirectory,
        window: () => window,
        trusted: senderAllowed,
      });
      const notifications = registerSystemNotifications({
        bridgePath: launch.notificationBridgePath,
        bundleId: launch.notificationBundleId,
        applicationName: launch.applicationName,
        preferences: () => state?.getSnapshot().preferences ?? null,
        locale: () => state?.getSnapshot().appearance.locale ?? 'en',
        window: () => window,
        trusted: senderAllowed,
        activateFeedback: (target) => {
          if (target.kind === 'agentTask' && target.review) {
            void mcp?.ui({ action: 'task', taskId: target.taskId }).then((result) => {
              pendingNotificationNavigation =
                result.ok && result.reply?.kind === 'task'
                  ? {
                      ...target,
                      contextId: result.reply.task.contextId,
                      review: result.reply.task.phase === 'waitingReview',
                    }
                  : { ...target, review: false };
              showWindow(false);
            });
            return;
          }
          pendingNotificationNavigation = target;
          showWindow(false);
        },
        activate: (eventId) => {
          if (eventId?.startsWith('transfer:'))
            pendingTransferNavigation = workspace?.transferNotificationTarget(eventId);
          if (eventId?.startsWith('agent:')) {
            const [, kind, id] = eventId.split(':');
            if (!id) {
              showWindow(false);
              return;
            }
            if (kind === 'activity') {
              pendingNotificationNavigation = { kind: 'agentActivity', contextId: id };
              showWindow(false);
              return;
            }
            if (kind === 'context')
              pendingNotificationNavigation = { kind: 'agentContext', contextId: id };
            else {
              void mcp?.ui({ action: 'task', taskId: id }).then((result) => {
                pendingNotificationNavigation = {
                  kind: 'agentTask',
                  taskId: id,
                  contextId:
                    result.ok && result.reply?.kind === 'task' ? result.reply.task.contextId : '',
                  review:
                    result.ok &&
                    result.reply?.kind === 'task' &&
                    result.reply.task.phase === 'waitingReview',
                };
                showWindow(false);
              });
              return;
            }
          }
          showWindow(false);
        },
      });
      stopNotifications = () => {
        notifications.feedback.stop();
        notifications.stop();
      };
      workspace?.setTransferNotifications(
        notifications,
        notifications.feedback,
        () => quitting || host.isStopping,
      );
      observeFileEditingNotifications(
        fileEditors,
        notifications.feedback,
        () => state?.getSnapshot().appearance.locale ?? 'en',
      );
      mcp = registerMcp({
        host,
        bridgePath: launch.mcpIdentityBridgePath,
        trusted: senderAllowed,
        window: () => window,
        locale: () => state?.getSnapshot().appearance.locale ?? 'en',
        scrollback: () => state?.getSnapshot().preferences?.terminalScrollback ?? 10000,
        notifications: notifications.feedback,
        tray: setTrayVisible,
      });
      observeRuntimeNotifications(host, notifications.feedback, {
        count: () =>
          (workspace?.activeCount ?? 0) +
          (workspace?.activeTransferCount ?? 0) +
          (workspace?.activeFileOperationCount ?? 0) +
          (mcp?.activeCount ?? 0) +
          (fileEditors?.activeCount ?? 0),
        planned: () => quitting || host.isStopping,
        locale: () => state?.getSnapshot().appearance.locale ?? 'en',
        available: () => state?.getSnapshot().runtime.status === 'ready',
      });
      // 只加载随包资源；开发连接白名单仅限本次 Vite origin。
      session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) =>
        callback(false),
      );
      session.defaultSession.setPermissionCheckHandler(() => false);
      session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
        const dev = developmentUrl
          ? ` ${new URL(developmentUrl).origin} ${new URL(developmentUrl).origin.replace('http', 'ws')}`
          : '';
        callback({
          responseHeaders: {
            ...details.responseHeaders,
            'Content-Security-Policy': [
              `default-src 'self'; script-src 'self'${developmentUrl ? " 'unsafe-inline'" : ''}; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self'${dev}; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'`,
            ],
          },
        });
      });
      for (const channel of [
        'desktop:snapshot',
        'desktop:update',
        'desktop:retry',
        'desktop:reconcile',
        'desktop:restart',
      ])
        ipcMain.handle(channel, async (event, request) => {
          if (!senderAllowed(event)) throw Error('invalid_sender');
          if (channel === 'desktop:update') return state!.update(request);
          if (channel === 'desktop:reconcile') return state!.reconcilePreferences();
          if (channel === 'desktop:restart') return state!.restartBackend();
          if (channel === 'desktop:retry') return state!.recover();
          return state!.getSnapshot();
        });
      ipcMain.handle('desktop:ui-get', async (event, scope) => {
        if (!senderAllowed(event) || scope === 'pickers') throw Error('invalid_sender');
        return uiState!.get(scope);
      });
      ipcMain.handle('desktop:ui-update', async (event, value) => {
        if (!senderAllowed(event) || value?.scope === 'pickers') throw Error('invalid_sender');
        return uiState!.set(value.scope, value.patch);
      });
      ipcMain.handle('desktop:ui-retry', async (event, scope) => {
        if (!senderAllowed(event)) throw Error('invalid_sender');
        return uiState!.retry(scope);
      });
      ipcMain.handle('desktop:download-directory', async (event) => {
        if (!senderAllowed(event)) throw Error('invalid_sender');
        const before = state!.getSnapshot();
        if (before.save.phase !== 'idle' || !before.preferences) return before;
        const runtimeId = before.runtime.instanceId;
        const picked = await dialog.showOpenDialog(window!, {
          properties: ['openDirectory', 'createDirectory'],
          defaultPath: before.preferences.downloadDirectory || undefined,
        });
        if (
          picked.canceled ||
          !picked.filePaths[0] ||
          !senderAllowed(event) ||
          runtimeId !== state!.getSnapshot().runtime.instanceId
        )
          return state!.getSnapshot();
        const p = before.preferences;
        return state!.update(
          {
            patch: {
              downloadDirectory: picked.filePaths[0],
              downloadMode: 'directory',
            },
            base: {
              downloadDirectory: p.downloadDirectory,
              downloadMode: p.downloadMode,
            },
            expectedRevision: p.revision,
          },
          true,
        );
      });
      ipcMain.on('desktop:renderer-ready', (event) => {
        if (!senderAllowed(event)) return;
        startup?.rendererReady();
      });
      state.on('changed', (snapshot: DesktopSnapshot) => {
        mcp?.displaySettingsChanged();
        workspace?.context(snapshot.appearance);
        if (snapshot.runtime.status === 'ready') void mcp?.refresh().catch(() => undefined);
        if (quitting) return;
        menus(snapshot);
        if (window && !window.isDestroyed()) {
          if (process.platform === 'darwin')
            window.setVibrancy(nativeTheme.prefersReducedTransparency ? null : 'sidebar');
          window.setBackgroundColor(
            process.platform === 'darwin' && !nativeTheme.prefersReducedTransparency
              ? '#00000000'
              : snapshot.appearance.theme === 'dark'
                ? '#262626'
                : '#ffffff',
          );
          window.webContents.send('desktop:changed', snapshot);
        }
      });
      nativeTheme.on('updated', () => {
        if (window && process.platform === 'darwin') {
          window.setVibrancy(nativeTheme.prefersReducedTransparency ? null : 'sidebar');
          window.setBackgroundColor(
            nativeTheme.prefersReducedTransparency
              ? nativeTheme.shouldUseDarkColors
                ? '#262626'
                : '#ffffff'
              : '#00000000',
          );
        }
        state?.refreshSystem();
      });
      powerMonitor.on('resume', () => state?.refreshSystem());
      await state.recover();
      await mcp.refresh().catch(() => undefined);
      setTrayVisible(mcp.showTray);
      if (quitting) return;
      menus(state.getSnapshot());
      await createWindow();
    })
    .catch((error) => {
      console.error(error);
      app.exit(1);
    });
}
