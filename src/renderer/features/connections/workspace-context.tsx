import { userErrorKey } from '../../../shared/i18n/operation-errors';
import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { mcpSnapshotStore } from '../mcp/snapshot-store';
import { useDesktop } from '../../app/DesktopProvider';
import { WorkspaceStore, WorkspaceFailure } from './workspace-store';
import type { MessageKey } from '../../../shared/i18n/messages';
const Context = createContext<WorkspaceStore | null>(null);
export function WorkspaceProvider({
  children,
  store,
}: {
  children: ReactNode;
  store: WorkspaceStore;
}) {
  const { snapshot } = useDesktop();
  useEffect(() => {
    store.backend(snapshot.runtime.instanceId, snapshot.runtime.status === 'ready');
    if (snapshot.runtime.status !== 'ready') return;
    let alive = true;
    const unsubscribe = window.vigilshell.onConnectionAccessChanged((value) => {
      if (alive) store.acceptConnectionAccess(value);
    });
    void window.vigilshell
      .getConnectionAccess()
      .then((value) => {
        if (alive) store.acceptConnectionAccess(value);
      })
      .catch(() => {
        /* Runtime feedback owns transport failures; never infer a key failure. */
      });
    return () => {
      alive = false;
      unsubscribe();
    };
  }, [store, snapshot.runtime.instanceId, snapshot.runtime.status]);

  const mcp = mcpSnapshotStore(window.vigilshell.mcp);
  const agent = useSyncExternalStore(mcp.subscribe, mcp.getSnapshot);
  const selectionRevision = useRef(agent.selectionRevision);
  useEffect(() => {
    if (!agent.snapshot) return;
    const selectable =
      agent.selectedView &&
      agent.snapshot.views.some((view) => view.context.id === agent.selectedView);
    const selected =
      selectable && selectionRevision.current !== agent.selectionRevision
        ? agent.selectedView
        : null;
    if (selectable || !agent.selectedView) selectionRevision.current = agent.selectionRevision;
    store.observeAgentViews(agent.snapshot.views, selected);
  }, [store, agent]);

  return <Context.Provider value={store}>{children}</Context.Provider>;
}
export function useWorkspace() {
  const store = useContext(Context);
  if (!store) throw Error('missing_workspace_provider');
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const operationState = useSyncExternalStore(store.feedback.subscribe, store.feedback.getSnapshot);
  return { store, state, operationState };
}
export function errorCode(error: unknown) {
  return error instanceof WorkspaceFailure ? error.code : 'generic';
}
export function errorKey(code: string): MessageKey {
  return userErrorKey(code);
}
export function phaseKey(phase: string): MessageKey {
  return `ws_${['connecting', 'authenticating', 'opening', 'running', 'closed', 'failed', 'cancelled', 'working'].includes(phase) ? phase : phase === 'trust' ? 'not_trusted' : 'working'}` as MessageKey;
}
