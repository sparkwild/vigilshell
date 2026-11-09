import type { DesktopApi, SettingsBridge } from '../../../contracts/desktop';
import type { DesktopSnapshot, UpdatePreferencesRequest } from '@contracts';
export type { DesktopApi, SettingsBridge };
declare global {
  interface Window {
    vigilshell: DesktopApi;
  }
}
/** 在初始读取之前订阅，按 main 的序号拒绝迟到快照。 */
export function createDesktopStore(bridge: SettingsBridge) {
  let snapshot: DesktopSnapshot | undefined;
  let disposed = false;
  let submitting = false;
  const listeners = new Set<() => void>();
  let unsubscribe: (() => void) | undefined;
  const accept = (next: DesktopSnapshot) => {
    if (disposed) return;
    if (snapshot && next.version <= snapshot.version) return;
    snapshot = next;
    for (const listener of listeners) listener();
  };
  return {
    async initialize() {
      unsubscribe?.();
      disposed = false;
      unsubscribe = bridge.onSnapshotChanged(accept);
      accept(await bridge.getSnapshot());
    },
    getSnapshot: () => {
      if (!snapshot) throw Error('desktop_not_initialized');
      return snapshot;
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    async update(request: UpdatePreferencesRequest) {
      const result = await bridge.updatePreferences(request);
      accept(result);
      return result;
    },
    async updateFields(patch: import('@contracts').PreferencesPatch) {
      if (!snapshot?.preferences || snapshot.runtime.status !== 'ready')
        throw Error('runtime_unavailable');
      if (submitting || snapshot.save.phase !== 'idle') return { ...snapshot, error: 'busy' };
      const base: import('@contracts').PreferencesPatch = {};
      const changed: import('@contracts').PreferencesPatch = {};
      for (const key of Object.keys(patch) as (keyof typeof patch)[]) {
        if (JSON.stringify(patch[key]) === JSON.stringify(snapshot.preferences[key])) continue;
        Object.assign(base, { [key]: snapshot.preferences[key] });
        Object.assign(changed, { [key]: patch[key] });
      }
      if (!Object.keys(changed).length) return snapshot;
      submitting = true;
      try {
        const result = await bridge.updatePreferences({
          patch: changed,
          base,
          expectedRevision: snapshot.preferences.revision,
        });
        accept(result);
        return result;
      } finally {
        submitting = false;
      }
    },
    async chooseDownloadDirectory() {
      const result = await bridge.chooseDownloadDirectory();
      accept(result);
      return result;
    },
    async reconcile() {
      accept(await bridge.reconcilePreferences());
    },
    async restart() {
      accept(await bridge.restartBackend());
    },
    async retry() {
      accept(await bridge.retryRuntime());
    },
    dispose() {
      disposed = true;
      unsubscribe?.();
      unsubscribe = undefined;
      listeners.clear();
    },
  };
}
export type DesktopStore = ReturnType<typeof createDesktopStore>;
