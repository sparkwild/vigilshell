import type { AgentSnapshot } from '@contracts';
import type { McpApi } from '../../../contracts/mcp';

type SnapshotState = {
  snapshot: AgentSnapshot | null;
  error: string | null;
  selectedView: string | null;
  selectionRevision: number;
};

/** One snapshot owner for the workspace and MCP pages; late reads cannot undo a mutation. */
export class McpSnapshotStore {
  private state: SnapshotState = {
    snapshot: null,
    error: null,
    selectedView: null,
    selectionRevision: 0,
  };
  private listeners = new Set<() => void>();
  private unsubscribe: (() => void) | null = null;
  private sequence = 0;
  constructor(private readonly api: McpApi) {}
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    if (!this.unsubscribe) {
      this.unsubscribe = this.api.onChanged((event) => {
        if (event.kind === 'viewsChanged')
          this.publish({
            selectedView: event.selected_view,
            selectionRevision: this.state.selectionRevision + 1,
          });
        if (
          ['configuration', 'context', 'accessChanged', 'task', 'viewsChanged'].includes(event.kind)
        )
          void this.refresh();
      });
      void this.refresh();
    }
    return () => {
      this.listeners.delete(listener);
      if (!this.listeners.size) {
        this.unsubscribe?.();
        this.unsubscribe = null;
        this.sequence++;
      }
    };
  };
  private publish(update: Partial<SnapshotState>) {
    this.state = { ...this.state, ...update };
    for (const listener of this.listeners) listener();
  }
  accept(snapshot: AgentSnapshot) {
    this.sequence++;
    this.publish({ snapshot, error: null });
  }
  invalidate() {
    this.sequence++;
  }
  refresh = async () => {
    const sequence = ++this.sequence;
    this.publish({ error: null });
    try {
      const result = await this.api.request({ action: 'snapshot' });
      if (sequence !== this.sequence) return;
      if (result.ok && result.snapshot) this.publish({ snapshot: result.snapshot, error: null });
      else if (!result.ok) this.publish({ error: result.code });
    } catch {
      if (sequence === this.sequence) this.publish({ error: 'runtime_unavailable' });
    }
  };
}
const stores = new WeakMap<McpApi, McpSnapshotStore>();
export function mcpSnapshotStore(api: McpApi) {
  let store = stores.get(api);
  if (!store) {
    store = new McpSnapshotStore(api);
    stores.set(api, store);
  }
  return store;
}
