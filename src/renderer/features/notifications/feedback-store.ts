import type { NotificationFeedback, NotificationTarget } from '@contracts';
import type { SystemNotificationsApi } from '../../../contracts/system-notifications';
export function notificationSurfaceKeys(target: NotificationTarget): string[] {
  switch (target.kind) {
    case 'agentTask':
      return [`agentTask:${target.taskId}`];
    case 'agentContext':
      return [`agentContext:${target.contextId}`];
    case 'agentActivity':
      return [`agentActivity:${target.contextId}`];
    case 'fileEditor':
      return [`fileEditor:${target.id}`];
    case 'fileOperation':
      return [`fileOperation:${target.operationId}`];
    case 'connection':
      return [target.sessionId ? `connection:${target.sessionId}` : `tunnel:${target.tunnelId}`];
    case 'transfer':
      return [`transfer:${target.batchId}`];
    case 'runtime':
      return ['runtime:'];
    case 'mcp':
      return ['mcp:'];
  }
}

/** Application-owned presentation state; subscribers never own its lifetime. */
export class NotificationFeedbackStore {
  private snapshot: NotificationFeedback[] = [];
  private listeners = new Set<() => void>();
  private anchors = new Map<string, string>();
  private reported = new Set<string>();
  private stop: (() => void) | null = null;
  private sequence = 0;
  constructor(private readonly api: SystemNotificationsApi) {}
  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private notify() {
    for (const listener of this.listeners) listener();
  }
  private present = (ids: string[]) => {
    if (!this.stop) return;
    ids.forEach((id) => this.reported.add(id));
    for (let offset = 0; offset < ids.length; offset += 100) {
      const batch = ids.slice(offset, offset + 100);
      void this.api.reportPresented(batch).catch(() => {
        batch.forEach((id) => this.reported.delete(id));
      });
    }
  };
  private acknowledgeVisible() {
    if (!document.hasFocus() || document.visibilityState !== 'visible') return;
    const visible = new Set(this.anchors.values());
    this.present(
      this.snapshot
        .filter(
          (item) =>
            item.relevant &&
            !this.reported.has(item.id) &&
            (visible.has(`event:${item.id}`) ||
              item.relatedTargets.every((target) =>
                notificationSurfaceKeys(target).some((key) => visible.has(key)),
              )),
        )
        .map((item) => item.id),
    );
  }
  setAnchor = (id: string, owner: string, visible: boolean) => {
    if (visible) this.anchors.set(id, owner);
    else this.anchors.delete(id);
    this.acknowledgeVisible();
  };
  refresh = async () => {
    if (!this.stop) return;
    const sequence = ++this.sequence;
    try {
      const items = await this.api.getFeedback();
      if (!this.stop || sequence !== this.sequence) return;
      this.snapshot = items;
      const known = new Set(items.map((item) => item.id));
      for (const id of this.reported) if (!known.has(id)) this.reported.delete(id);
      this.acknowledgeVisible();
      this.notify();
    } catch {
      /* Preserve confirmed state until the next event or foreground refresh. */
    }
  };
  start() {
    if (this.stop) return;
    const changed = this.api.onFeedbackChanged(this.refresh);
    window.addEventListener('focus', this.refresh);
    document.addEventListener('visibilitychange', this.refresh);
    this.stop = () => {
      changed();
      window.removeEventListener('focus', this.refresh);
      document.removeEventListener('visibilitychange', this.refresh);
    };
    void this.refresh();
  }
  dispose() {
    this.stop?.();
    this.stop = null;
    this.sequence++;
    this.anchors.clear();
  }
}
