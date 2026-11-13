import {
  createSystemNotificationsApi,
  notificationChannels,
} from '../contracts/system-notifications';
import { workspaceMethods, type WorkspaceApi } from '../shared/workspace-api';
import type { WorkspaceNotification } from '@contracts';
import { contextBridge, ipcRenderer } from 'electron';
import { prepareStartup, finishStartup } from './startup';
import type { DesktopApi } from '../contracts/desktop';
import type { DesktopSnapshot } from '@contracts';
prepareStartup(process.argv);
ipcRenderer.on('workspace:terminal-port', (event, value) => {
  if (event.ports.length === 1) window.postMessage(value, '*', event.ports);
});
const bridge: DesktopApi = {
  application: {
    info: () => ipcRenderer.invoke('application:request', { action: 'info' }),
    state: () => ipcRenderer.invoke('application:request', { action: 'state' }),
    check: () => ipcRenderer.invoke('application:request', { action: 'check' }),
    download: () => ipcRenderer.invoke('application:request', { action: 'download' }),
    cancel: () => ipcRenderer.invoke('application:request', { action: 'cancel' }),
    install: () => ipcRenderer.invoke('application:request', { action: 'install' }),
    open: (action) => ipcRenderer.invoke('application:request', { action }),
    onChanged: (listener) => {
      const receive = (_: Electron.IpcRendererEvent, value: import('@contracts').UpdateSnapshot) =>
        listener(value);
      ipcRenderer.on('application:changed', receive);
      return () => ipcRenderer.removeListener('application:changed', receive);
    },
  },
  fileEditors: {
    request: (request) => ipcRenderer.invoke('file-editors:request', request),
    onChanged: (listener) => {
      const receive = () => listener();
      ipcRenderer.on('file-editors:changed', receive);
      return () => ipcRenderer.removeListener('file-editors:changed', receive);
    },
    onShow: (listener) => {
      const receive = () => listener();
      ipcRenderer.on('file-editors:show', receive);
      return () => ipcRenderer.removeListener('file-editors:show', receive);
    },
  },
  mcp: {
    request: (request) => ipcRenderer.invoke('mcp:request', request),
    onChanged: (listener) => {
      const receive = (_event: Electron.IpcRendererEvent, value: import('@contracts').AgentEvent) =>
        listener(value);
      ipcRenderer.on('mcp:changed', receive);
      return () => ipcRenderer.removeListener('mcp:changed', receive);
    },
    onNavigate: (listener) => {
      const receive = (
        _event: Electron.IpcRendererEvent,
        value: import('../contracts/mcp').McpNavigation,
      ) => listener(value);
      ipcRenderer.on('mcp:navigate', receive);
      return () => ipcRenderer.removeListener('mcp:navigate', receive);
    },
  },
  notifications: createSystemNotificationsApi(
    (command) => ipcRenderer.invoke(notificationChannels.request, command),
    (listener) => {
      const receive = (
        _event: Electron.IpcRendererEvent,
        value: import('../contracts/system-notifications').SystemNotificationStatus,
      ) => listener(value);
      ipcRenderer.on(notificationChannels.changed, receive);
      return () => ipcRenderer.removeListener(notificationChannels.changed, receive);
    },
    (request) => ipcRenderer.invoke(notificationChannels.feedback, request),
    (listener) => {
      const receive = () => listener();
      ipcRenderer.on(notificationChannels.feedbackChanged, receive);
      return () => ipcRenderer.removeListener(notificationChannels.feedbackChanged, receive);
    },
    (listener) => {
      const receive = (
        _event: Electron.IpcRendererEvent,
        target: import('@contracts').NotificationTarget,
      ) => listener(target);
      ipcRenderer.on(notificationChannels.navigate, receive);
      return () => ipcRenderer.removeListener(notificationChannels.navigate, receive);
    },
  ),
  chooseDownloadDirectory: () => ipcRenderer.invoke('desktop:download-directory'),
  getUiState: (scope) => ipcRenderer.invoke('desktop:ui-get', scope),
  updateUiState: (scope, patch) => ipcRenderer.invoke('desktop:ui-update', { scope, patch }),
  retryUiState: (scope) => ipcRenderer.invoke('desktop:ui-retry', scope),
  onUiStateChanged(listener) {
    const receive = (
      _event: Electron.IpcRendererEvent,
      notice: import('../shared/ui-state').UiStateNotice,
    ) => listener(notice);
    ipcRenderer.on('desktop:ui-changed', receive);
    return () => ipcRenderer.removeListener('desktop:ui-changed', receive);
  },

  workspace: Object.freeze(
    Object.fromEntries(
      Object.keys(workspaceMethods).map((name) => [
        name,
        (params: unknown) => ipcRenderer.invoke(`workspace:${name}`, params),
      ]),
    ),
  ) as WorkspaceApi,
  getConnectionAccess: () => ipcRenderer.invoke('workspace:connection-access'),
  onConnectionAccessChanged(listener) {
    const receive = (
      _event: Electron.IpcRendererEvent,
      value: import('@contracts').ConnectionAccessSnapshot,
    ) => listener(value);
    ipcRenderer.on('workspace:connection-access-changed', receive);
    return () => ipcRenderer.removeListener('workspace:connection-access-changed', receive);
  },
  onWorkspaceEvent(listener) {
    const receive = (_event: Electron.IpcRendererEvent, value: WorkspaceNotification) =>
      listener(value);
    ipcRenderer.on('workspace:event', receive);
    return () => ipcRenderer.removeListener('workspace:event', receive);
  },
  onWorkspaceCommand(listener) {
    const receive = (_event: Electron.IpcRendererEvent, value: 'new' | 'close' | 'find') =>
      listener(value);
    ipcRenderer.on('workspace:command', receive);
    return () => ipcRenderer.removeListener('workspace:command', receive);
  },
  reportTransferFeedback: (value) => ipcRenderer.send('workspace:transfer-feedback', value),
  onTransferNavigation(listener) {
    const receive = (
      _event: Electron.IpcRendererEvent,
      value: import('@contracts').TransferNotificationTarget,
    ) => listener(value);
    ipcRenderer.on('workspace:transfer-navigation', receive);
    return () => ipcRenderer.removeListener('workspace:transfer-navigation', receive);
  },
  attachTerminalPort: (id) => ipcRenderer.send('workspace:terminal-port', id),
  setWorkspaceUi: (value) => ipcRenderer.send('workspace:ui', value),
  copyText: (text) => ipcRenderer.invoke('workspace:copy', text),
  readClipboard: () => ipcRenderer.invoke('workspace:clipboard'),
  hideWindow: () => ipcRenderer.send('workspace:hide'),
  getSnapshot: () => ipcRenderer.invoke('desktop:snapshot'),
  updatePreferences: (request) => ipcRenderer.invoke('desktop:update', request),
  retryRuntime: () => ipcRenderer.invoke('desktop:retry'),
  reconcilePreferences: () => ipcRenderer.invoke('desktop:reconcile'),
  restartBackend: () => ipcRenderer.invoke('desktop:restart'),
  rendererReady: () => {
    finishStartup();
    ipcRenderer.send('desktop:renderer-ready');
  },
  onSnapshotChanged(listener) {
    const receive = (_event: Electron.IpcRendererEvent, value: DesktopSnapshot) => listener(value);
    ipcRenderer.on('desktop:changed', receive);
    return () => ipcRenderer.removeListener('desktop:changed', receive);
  },
  onOpenSettings(listener) {
    const receive = (
      _event: Electron.IpcRendererEvent,
      section?: 'notifications' | 'about' | 'updates',
    ) => listener(section);
    ipcRenderer.on('desktop:open-settings', receive);
    return () => ipcRenderer.removeListener('desktop:open-settings', receive);
  },
};
contextBridge.exposeInMainWorld('vigilshell', Object.freeze(bridge));
