import type {
  TransferView,
  NotificationFeedbackDisposition,
  TransferNotificationTarget,
  ResolvedLocale,
} from '../../contracts/generated/runtime';
import { connectionFailureCodes } from './workspace';
import { transferFeedbackKey } from '../../shared/transfer-feedback';
import { translate } from '../../shared/i18n/messages';
import type { SystemNotificationService } from './service';

type Member = Pick<TransferView, 'id' | 'attempt' | 'revision' | 'phase' | 'conflictVersion'> & {
  skippedChildren: number;
  error?: string | null;
};
type Batch = {
  size: number;
  connectionId: string;
  direction: string;
  members: Map<string, Member>;
  round: Set<string>;
  initial: boolean;
  sequence: number;
  waiting: boolean;
  episode: number;
  conflictRoute?: NotificationFeedbackDisposition;
  finish?: ReturnType<typeof setTimeout>;
  conflict?: ReturnType<typeof setTimeout>;
};
const terminal = (t: Member) => ['complete', 'failed', 'cancelled', 'skipped'].includes(t.phase);
/** Stores only aggregation metadata; WorkspaceIpc owns the task bodies. */
export class TransferNotificationCoordinator {
  private batches = new Map<string, Batch>();
  private presented = new Set<string>();
  private routes = new Map<string, NotificationFeedbackDisposition>();
  private targets = new Map<string, TransferNotificationTarget>();
  private results = new Map<string, NotificationFeedbackDisposition>();
  private epoch = 0;
  constructor(
    private ports: {
      runtime(): string;
      focused(): boolean;
      locale(): ResolvedLocale;
      publish(
        intent: Parameters<SystemNotificationService['publish']>[0],
        cause?: 'connection' | 'runtime',
      ): ReturnType<SystemNotificationService['publish']>;
    },
  ) {}
  reset() {
    this.epoch++;
    for (const b of this.batches.values()) {
      clearTimeout(b.finish);
      clearTimeout(b.conflict);
    }
    this.batches.clear();
    this.presented.clear();
    this.routes.clear();
    this.results.clear();
    // Retain bounded targets so late clicks can display an unavailable batch.
  }
  target(id: string) {
    return (
      this.targets.get(id) ?? {
        runtimeId: '',
        connectionId: '',
        batchId: '',
        phase: 'failed' as const,
      }
    );
  }
  report(keys: string[]) {
    for (const key of keys) {
      this.presented.add(key);
    }
  }
  private setRoute(keys: string[], route: NotificationFeedbackDisposition) {
    if (!keys.length) return;
    for (const key of keys) this.routes.set(key, route);
  }
  observe(task: TransferView, snapshot = false) {
    let b = this.batches.get(task.batchId);
    if (!b) {
      b = {
        size: task.batchSize,
        connectionId: task.connectionId,
        direction: task.direction,
        members: new Map(),
        round: new Set(),
        initial: !snapshot,
        sequence: 0,
        waiting: false,
        episode: 0,
      };
      this.batches.set(task.batchId, b);
    }
    const previous = b.members.get(task.id);
    if (previous && previous.revision >= task.revision) return;
    b.members.set(task.id, {
      id: task.id,
      attempt: task.attempt,
      revision: task.revision,
      phase: task.phase,
      conflictVersion: task.conflictVersion,
      skippedChildren: task.details?.skipped ?? 0,
      error: task.error,
    });
    if (previous && transferFeedbackKey(previous) !== transferFeedbackKey(task)) {
      const oldKey = transferFeedbackKey(previous);
      if (oldKey) {
        this.routes.delete(oldKey);
        this.presented.delete(oldKey);
      }
    }
    if (!snapshot && previous && !terminal(previous) && b.sequence === 0 && !b.round.size) {
      b.initial = true;
      for (const id of b.members.keys()) b.round.add(id);
    }
    if (snapshot && b.initial) b.round.add(task.id);
    if (!snapshot && (b.initial || !previous || task.attempt > previous.attempt))
      b.round.add(task.id);
    const key = transferFeedbackKey(task);
    if (!snapshot && key && !this.routes.has(key))
      this.setRoute(
        [key],
        task.phase === 'awaitingDecision' && b.waiting && !b.conflict
          ? (b.conflictRoute ?? 'pending')
          : 'pending',
      );
    if (snapshot) {
      if (key && !this.routes.has(key)) this.setRoute([key], 'local');
      if (!b.round.size) return;
    }
    const members = [...b.members.values()];
    const waiting = members.some((t) => t.phase === 'awaitingDecision');
    if (!waiting) {
      b.waiting = false;
      b.conflictRoute = undefined;
      clearTimeout(b.conflict);
      b.conflict = undefined;
    } else if (!b.waiting) {
      b.waiting = true;
      const episode = ++b.episode;
      b.conflict = setTimeout(() => {
        b.conflict = undefined;
        const tasks = [...b.members.values()].filter((t) => t.phase === 'awaitingDecision');
        if (tasks.length) void this.send(task.batchId, b, tasks, 'awaitingDecision', episode);
      }, 1000);
    } else if (!b.conflict && key && task.phase === 'awaitingDecision' && !this.routes.has(key)) {
      // A continuing conflict episode is not a new notification.
      this.setRoute([key], b.conflictRoute ?? 'pending');
    }
    clearTimeout(b.finish);
    b.finish = undefined;
    if (b.members.size !== b.size || !b.round.size) return;
    const round = [...b.round].map((id) => b.members.get(id)!);
    if (!round.every(terminal)) return;
    b.finish = setTimeout(() => {
      b.finish = undefined;
      const tasks = [...b.round].map((id) => b.members.get(id)!);
      b.round.clear();
      b.initial = false;
      if (tasks.some((t) => t.phase === 'failed'))
        void this.send(task.batchId, b, tasks, 'failed', ++b.sequence);
      this.trim();
    }, 1000);
  }
  private trim() {
    const finished = [...this.batches].filter(
      ([, b]) => !b.round.size && !b.waiting && [...b.members.values()].every(terminal),
    );
    for (const [id, b] of finished.slice(0, Math.max(0, finished.length - 256))) {
      this.batches.delete(id);
      for (const t of b.members.values()) {
        const key = transferFeedbackKey(t);
        if (key) {
          this.presented.delete(key);
          this.routes.delete(key);
        }
      }
    }
    while (this.targets.size > 256) this.targets.delete(this.targets.keys().next().value!);
  }
  private async send(
    batchId: string,
    b: Batch,
    tasks: Member[],
    phase: TransferNotificationTarget['phase'],
    sequence: number,
  ) {
    const keys = tasks.flatMap((t) => {
      const k = transferFeedbackKey(t);
      return k ? [k] : [];
    });
    const unseen = keys.filter((k) => !this.presented.has(k));
    if (!unseen.length) {
      if (phase === 'awaitingDecision') b.conflictRoute = 'local';
      return;
    }
    const runtimeId = this.ports.runtime(),
      epoch = this.epoch;
    const eventId = `transfer:${epoch}:${batchId}:${phase}:${sequence}`;
    const locale = this.ports.locale();
    const counts = Object.fromEntries(
      ['complete', 'failed', 'cancelled', 'skipped'].map((p) => [
        p,
        tasks.filter((t) => t.phase === p).length,
      ]),
    );
    const skippedChildren = tasks.reduce((count, t) => count + t.skippedChildren, 0);
    const resultBody =
      phase === 'awaitingDecision'
        ? translate('transfer_notification_waiting', locale, { count: tasks.length })
        : translate('transfer_notification_result', locale, {
            direction: translate(
              b.direction === 'upload'
                ? 'transfer_notification_upload'
                : 'transfer_notification_download',
              locale,
            ),
            ...counts,
          });
    const body =
      phase !== 'awaitingDecision' && skippedChildren
        ? `${resultBody} · ${translate('transfer_child_skipped', locale, { count: skippedChildren })}`
        : resultBody;
    this.targets.set(eventId, { runtimeId, connectionId: b.connectionId, batchId, phase });
    while (this.targets.size > 256) this.targets.delete(this.targets.keys().next().value!);
    this.setRoute(unseen, 'pending');
    const result = await this.ports.publish(
      {
        eventId,
        category: phase === 'failed' ? 'executionIssue' : 'attentionRequired',
        stillRelevant: () =>
          epoch === this.epoch &&
          tasks.every((t) => {
            const current = b.members.get(t.id);
            return (
              current?.attempt === t.attempt &&
              current.phase === t.phase &&
              current.conflictVersion === t.conflictVersion
            );
          }) &&
          unseen.some((key) => !this.presented.has(key)),
        title: translate(
          phase === 'awaitingDecision'
            ? 'transfer_notification_attention_title'
            : counts.failed === tasks.length
              ? 'transfer_notification_failed_title'
              : 'transfer_notification_partial_title',
          locale,
        ),
        body,
      },
      phase === 'failed' &&
        tasks.filter((t) => t.phase === 'failed').every((t) => t.error === 'runtime_unavailable')
        ? 'runtime'
        : phase === 'failed' &&
            tasks
              .filter((t) => t.phase === 'failed')
              .every((t) => connectionFailureCodes.has(t.error ?? ''))
          ? 'connection'
          : undefined,
    );
    if (epoch !== this.epoch) return;
    if (result.outcome === 'duplicate') {
      this.setRoute(unseen, this.results.get(eventId) ?? 'unknown');
      return;
    }
    const route =
      result.outcome === 'submitted'
        ? 'submitted'
        : result.outcome === 'unknown'
          ? 'unknown'
          : 'local';
    this.results.set(eventId, route);
    while (this.results.size > 256) this.results.delete(this.results.keys().next().value!);
    if (phase === 'awaitingDecision' && b.episode === sequence) {
      b.conflictRoute = route;
      const keys = [...b.members.values()].flatMap((t) =>
        t.phase === 'awaitingDecision' && !this.presented.has(transferFeedbackKey(t)!)
          ? [transferFeedbackKey(t)!]
          : [],
      );
      this.setRoute(keys, route);
    }
    this.setRoute(
      unseen.filter((k) => !this.presented.has(k)),
      route,
    );
    this.trim();
  }
}
