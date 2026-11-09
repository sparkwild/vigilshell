import { createContext, useContext, useCallback, useEffect, useRef } from 'react';
import type { Resource } from './workspace-store';
export interface WorkspaceUiActions {
  active: boolean;
  beforeConnectionAction?: () => Promise<boolean>;
  navigationWidth: number;
  openConnection(connectionId: string, signal?: AbortSignal): Promise<string | null>;
  setToolDialogClose(close: (() => void) | null): void;
  showHostIdentity(connectionId: string): void;
  retryCredentialCleanup(connectionId: string): void;
  deleteResource(resource: Resource): void;
  paste(text: string): Promise<boolean>;
  setDirty(dirty: boolean): void;
  setCloseEditor(close: (() => Promise<void>) | null): void;
}
export const WorkspaceUiContext = createContext<WorkspaceUiActions | null>(null);
export function useWorkspaceUi() {
  const value = useContext(WorkspaceUiContext);
  if (!value) throw Error('missing_workspace_ui');
  return value;
}

export function useOpenConnection() {
  const { openConnection } = useWorkspaceUi();
  const source = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    source.current = controller;
    return () => controller.abort();
  }, []);
  return useCallback(
    (id: string) =>
      source.current && !source.current.signal.aborted
        ? openConnection(id, source.current.signal)
        : Promise.resolve(null),
    [openConnection],
  );
}
