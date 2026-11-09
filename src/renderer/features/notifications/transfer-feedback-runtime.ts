import type { WorkspaceStore } from '../connections/workspace-store';
import { transferFeedbackKey } from '../../../shared/transfer-feedback';
/** Visible task bodies acknowledge transfer events independently of notification views. */
export function startTransferFeedback(workspace: WorkspaceStore) {
  const reported = new Set<string>();
  let runtime: string | null = null;
  const refresh = () => {
    const state = workspace.getSnapshot();
    if (runtime !== state.runtimeId) {
      runtime = state.runtimeId;
      reported.clear();
    }
    if (!runtime || !document.hasFocus() || document.visibilityState !== 'visible') return;
    const current = new Set([...state.transfers.values()].map(transferFeedbackKey).filter(Boolean));
    for (const key of reported) if (!current.has(key)) reported.delete(key);
    const anchors = workspace.feedback.getSnapshot().anchors;
    const keys = [...state.transfers.values()].flatMap((task) => {
      const key = transferFeedbackKey(task);
      return key && anchors.has(key) && !reported.has(key) ? [key] : [];
    });
    for (let offset = 0; offset < keys.length; offset += 100) {
      const batch = keys.slice(offset, offset + 100);
      window.vigilshell.reportTransferFeedback({ runtimeId: runtime, keys: batch });
      batch.forEach((key) => reported.add(key));
    }
  };
  const state = workspace.subscribe(refresh);
  const anchors = workspace.feedback.subscribe(refresh);
  window.addEventListener('focus', refresh);
  document.addEventListener('visibilitychange', refresh);
  refresh();
  return () => {
    state();
    anchors();
    reported.clear();
    window.removeEventListener('focus', refresh);
    document.removeEventListener('visibilitychange', refresh);
  };
}
