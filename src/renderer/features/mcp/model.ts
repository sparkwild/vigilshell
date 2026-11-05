import { quietAuthorizationFailure } from './authorization-feedback';
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import type {
  AgentSnapshot,
  AgentUiRequest,
  ConnectionPolicySettings,
  ConnectionPolicy,
} from '@contracts';
import { mcpSnapshotStore } from './snapshot-store';
import type { McpNavigation } from '../../../contracts/mcp';

export function navigateMcp(target: McpNavigation) {
  window.dispatchEvent(new CustomEvent('vigilshell:mcp-navigation', { detail: target }));
}
export function subscribeMcpNavigation(listener: (target: McpNavigation) => void) {
  const receive = (event: Event) => listener((event as CustomEvent<McpNavigation>).detail);
  window.addEventListener('vigilshell:mcp-navigation', receive);
  const unsubscribe = window.vigilshell.mcp.onNavigate(listener);
  return () => {
    window.removeEventListener('vigilshell:mcp-navigation', receive);
    unsubscribe();
  };
}
export function defaultPolicy(connectionId: string): ConnectionPolicy {
  return {
    connectionId,
    connectionRevision: 0,
    revision: 0,
    verifyConnect: true,
    execution: {
      version: 2,
      level: 'readOnly',
      fileTransfer: false,
      protectSystem: true,
      scriptReview: false,
      protectedPaths: [],
      uploadRoots: [],
      downloadRoots: [],
      limits: {
        maxTaskSeconds: 1800,
        terminalSeconds: 7200,
        terminalIdleSeconds: null,
        concurrentTasks: 4,
        concurrentTransfers: 2,
        concurrentTerminals: 2,
        transferSeconds: null,
        transferStallSeconds: 300,
      },
    },
  };
}
export function defaultPolicySettings(connectionId: string): ConnectionPolicySettings {
  return { ...defaultPolicy(connectionId), accessEnabled: false };
}
export function useMcp() {
  const store = mcpSnapshotStore(window.vigilshell.mcp);
  const { snapshot, error: sharedError } = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const refresh = store.refresh;
  const issue = useCallback(
    async (
      request: AgentUiRequest,
      onSnapshot?: (value: AgentSnapshot) => void,
      onFailure?: (code: string) => boolean,
    ) => {
      const blocking = request.action !== 'detectClients';
      if (blocking) store.invalidate();
      if (blocking) setBusy(true);
      setError(null);
      try {
        const result = await window.vigilshell.mcp.request(request);
        if (mounted.current) {
          if (!result.ok) {
            if (!onFailure?.(result.code) && !quietAuthorizationFailure(request, result.code))
              setError(result.code);
          } else if (result.snapshot) {
            // 提交返回的快照优先于仍在途的读取，避免旧读取覆盖刚确认的策略。
            store.accept(result.snapshot);
            onSnapshot?.(result.snapshot);
          }
        }
        return result.ok;
      } catch {
        if (mounted.current) setError('runtime_unavailable');
        return false;
      } finally {
        if (mounted.current && blocking) setBusy(false);
      }
    },
    [store],
  );
  return { snapshot, error: error ?? sharedError, busy, refresh, issue };
}
