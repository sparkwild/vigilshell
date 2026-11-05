import type {
  NotificationFeedback,
  NotificationTarget,
  SystemNotificationResult,
} from '@contracts';
import type { SystemNotificationIntent } from './service';
export interface FeedbackIntent extends SystemNotificationIntent {
  target: NotificationTarget;
  runtimeId?: string;
  connectionId?: string;
  relatedTargets?: NotificationTarget[];
  cause?: 'connection' | 'runtime';
}
type Record = {
  view: NotificationFeedback;
  intent: FeedbackIntent;
  invalid: boolean;
  result?: SystemNotificationResult;
};
/** Only presentation metadata lives here; source services retain all business state. */
export class NotificationFeedbackCoordinator {
  private records = new Map<string, Record>();
  private stopped = false;
  private failedRuntimes = new Set<string>();
  private failedConnections = new Set<string>();
  failRuntime(id: string) {
    this.failedRuntimes.add(id);
    this.invalidate((i) => i.runtimeId === id && i.target.kind !== 'runtime');
  }
  failConnection(runtime: string, id: string) {
    this.failedConnections.add(`${runtime}:${id}`);
    this.invalidate(
      (i) => i.runtimeId === runtime && i.connectionId === id && i.cause === 'connection',
    );
  }
  connectionReady(runtime: string, id: string) {
    this.failedConnections.delete(`${runtime}:${id}`);
  }
  constructor(
    private ports: {
      publish(intent: SystemNotificationIntent): Promise<SystemNotificationResult>;
      changed(): void;
      activate(target: NotificationTarget): void;
      settle?: () => Promise<void>;
    },
  ) {}
  list(): NotificationFeedback[] {
    return [...this.records.values()].map((r) => ({ ...r.view, relevant: this.relevant(r) }));
  }
  present(ids: string[]) {
    // A visible renderer can report just after focus changes; retain that acknowledgement.
    for (const id of ids) {
      const r = this.records.get(id);
      if (r && this.relevant(r)) r.view.route = 'presented';
    }
    this.ports.changed();
  }
  activate(id: string) {
    const r = this.records.get(id);
    if (!r || this.stopped) return;
    this.ports.activate(r.view.target);
  }
  invalidate(matches: (intent: FeedbackIntent) => boolean) {
    for (const r of this.records.values()) if (matches(r.intent)) r.invalid = true;
    this.ports.changed();
  }
  refresh() {
    this.ports.changed();
  }
  private relevant(r: Record) {
    return !this.stopped && !r.invalid && (!r.intent.stillRelevant || r.intent.stillRelevant());
  }
  async publish(intent: FeedbackIntent): Promise<SystemNotificationResult> {
    const old = this.records.get(intent.eventId);
    if (old)
      return (
        old.result ?? { outcome: 'duplicate', status: { authorization: 'unknown', version: 0 } }
      );
    const r: Record = {
      intent,
      invalid:
        !!intent.runtimeId &&
        ((this.failedRuntimes.has(intent.runtimeId) && intent.target.kind !== 'runtime') ||
          (intent.cause === 'connection' &&
            !!intent.connectionId &&
            this.failedConnections.has(`${intent.runtimeId}:${intent.connectionId}`))),
      view: {
        id: intent.eventId,
        category: intent.category,
        title: intent.title,
        body: intent.body,
        target: intent.target,
        route: 'pending',
        relevant: true,
        relatedTargets: intent.relatedTargets ?? [intent.target],
      },
    };
    this.records.set(intent.eventId, r);
    // Retain bounded click targets; loading old source snapshots never registers notifications.
    while (this.records.size > 512) {
      const retired = [...this.records].find(([, item]) => item.view.route !== 'pending');
      if (!retired) break;
      this.records.delete(retired[0]);
    }
    this.ports.changed();
    // Let visible surfaces acknowledge the new event before native delivery.
    await (this.ports.settle?.() ?? new Promise<void>((resolve) => setTimeout(resolve, 150)));
    const result = await this.ports.publish({
      ...intent,
      stillRelevant: () => this.relevant(r) && r.view.route !== 'presented',
    });
    r.result = result;
    if (r.view.route !== 'presented')
      r.view.route =
        result.outcome === 'submitted'
          ? 'submitted'
          : result.outcome === 'unknown' || result.outcome === 'duplicate'
            ? 'unknown'
            : 'local';
    this.ports.changed();
    return result;
  }
  stop() {
    this.stopped = true;
    this.ports.changed();
  }
}
