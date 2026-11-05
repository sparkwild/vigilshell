import type { UiStateNotice, UiStatePatch } from '../shared/ui-state';
import type { WorkspaceDesktop } from '../shared/workspace-api';
import type { DesktopSnapshot, UpdatePreferencesRequest } from '@contracts';
/** 设置具名接口；原始 RPC 和传输能力不向 Renderer 开放。 */
export interface SettingsBridge {
  chooseDownloadDirectory(): Promise<DesktopSnapshot>;
  getSnapshot(): Promise<DesktopSnapshot>;
  updatePreferences(request: UpdatePreferencesRequest): Promise<DesktopSnapshot>;
  retryRuntime(): Promise<DesktopSnapshot>;
  reconcilePreferences(): Promise<DesktopSnapshot>;
  restartBackend(): Promise<DesktopSnapshot>;
  rendererReady(): void;
  onSnapshotChanged(listener: (snapshot: DesktopSnapshot) => void): () => void;
  onOpenSettings(listener: (section?: 'notifications') => void): () => void;
}

export interface UiStateBridge {
  getUiState(scope: string): Promise<UiStateNotice>;
  updateUiState(scope: string, patch: UiStatePatch): Promise<UiStateNotice>;
  retryUiState(scope: string): Promise<UiStateNotice>;
  onUiStateChanged(listener: (notice: UiStateNotice) => void): () => void;
}
export interface DesktopApi extends SettingsBridge, UiStateBridge, WorkspaceDesktop {
  fileEditors: import('./file-editors').FileEditorsApi;
  notifications: import('./system-notifications').SystemNotificationsApi;
  mcp: import('./mcp').McpApi;
}
