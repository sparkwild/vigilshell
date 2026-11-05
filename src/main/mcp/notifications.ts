import type { AgentEvent } from '@contracts';
import type { FeedbackIntent } from '../notifications/feedback';
import { translate } from '../../shared/i18n/messages';

export function agentNotificationIntent(
  event: Extract<AgentEvent, { kind: 'attention' | 'task' }>,
  locale: 'en' | 'zh-CN',
): FeedbackIntent | null {
  if (
    event.kind === 'task' &&
    [
      'queued',
      'waitingReview',
      'running',
      'stopping',
      'denied',
      'cancelled',
      'requestAccepted',
      'succeeded',
    ].includes(event.task.phase)
  )
    return null;
  const id = event.kind === 'attention' ? event.id : event.task.id;
  const target =
    event.kind === 'attention' && event.code === 'connection_approval_required'
      ? 'context'
      : 'task';
  return {
    // 同一任务的待确认与异常是两个通知；前缀继续供点击路由识别目标。
    eventId: `agent:${target}:${id}:${event.kind === 'attention' ? event.code : event.task.phase}`,
    category: event.kind === 'attention' ? 'attentionRequired' : 'executionIssue',
    target:
      event.kind === 'task'
        ? {
            kind: 'agentTask',
            contextId: event.task.contextId,
            taskId: event.task.id,
            review: false,
          }
        : event.code === 'connection_approval_required'
          ? { kind: 'agentContext', contextId: event.id }
          : { kind: 'agentTask', contextId: '', taskId: event.id, review: true },
    connectionId: event.kind === 'task' ? event.task.connectionId : event.connection_id,
    title: 'VigilShell MCP',
    body: translate(
      event.kind === 'attention'
        ? event.code === 'connection_approval_required'
          ? 'notification_connection_approval'
          : 'notification_script_review'
        : event.task.phase === 'interruptedUnknown'
          ? 'notification_task_unknown'
          : event.task.phase === 'timedOut'
            ? 'notification_task_timeout'
            : 'notification_task_failed',
      locale,
    ),
  };
}

/** 聚合同一会话、类别及事件类型；任务状态仍由 MCP 控制器持有。 */
export class AgentNotificationCoordinator {
  private groups = new Map<
    string,
    { timer: ReturnType<typeof setTimeout>; intents: Map<string, FeedbackIntent> }
  >();
  private revisions = new Map<string, number>();
  private phases = new Map<string, string>();
  private epoch = 0;
  private taskContexts = new Map<string, string>();
  private contexts = new Map<string, string>();
  observeContext(id: string, phase: string) {
    this.contexts.set(id, phase);
  }
  constructor(
    private ports: {
      locale(): 'en' | 'zh-CN';
      runtime?(): string | null;
      publish(intent: FeedbackIntent): Promise<unknown>;
    },
  ) {}
  observe(event: Extract<AgentEvent, { kind: 'attention' | 'task' }>) {
    if (event.kind === 'task') {
      const revision = event.task.revision ?? 0;
      if ((this.revisions.get(event.task.id) ?? -1) >= revision) return;
      this.revisions.set(event.task.id, revision);
      this.phases.set(event.task.id, event.task.phase);
      this.taskContexts.set(event.task.id, event.task.contextId);
    }
    const intent = agentNotificationIntent(event, this.ports.locale());
    if (!intent) return;
    const epoch = this.epoch;
    const runtime = this.ports.runtime?.();
    intent.runtimeId = runtime ?? undefined;
    const context =
      event.kind === 'task' ? event.task.contextId : (this.taskContexts.get(event.id) ?? event.id);
    if (intent.target.kind === 'agentTask' && !intent.target.contextId)
      intent.target.contextId = context;
    const kind = event.kind === 'task' ? event.task.phase : event.code;
    const key = `${context}:${intent.category}:${kind}`;
    const relevant = () =>
      epoch === this.epoch &&
      (!this.ports.runtime || this.ports.runtime() === runtime) &&
      (event.kind === 'attention'
        ? (event.code === 'connection_approval_required' &&
            this.contexts.get(event.id) === 'waitingUser') ||
          (event.code !== 'connection_approval_required' &&
            (event.code !== 'script_review_required' ||
              !this.phases.has(event.id) ||
              this.phases.get(event.id) === 'waitingReview'))
        : this.phases.get(event.task.id) === event.task.phase);
    intent.stillRelevant = relevant;
    let group = this.groups.get(key);
    if (!group) {
      group = {
        intents: new Map(),
        timer: setTimeout(() => {
          this.groups.delete(key);
          const current = [...group!.intents.values()].filter((item) => item.stillRelevant?.());
          if (!current.length) return;
          const first = current[0]!;
          void this.ports.publish(
            current.length === 1
              ? first
              : {
                  ...first,
                  target: { kind: 'agentActivity', contextId: context },
                  relatedTargets: current.map((item) => item.target),
                  eventId: `agent:activity:${context}:${epoch}:${crypto.randomUUID()}`,
                  body: `${first.body} · ${current.length}`,
                  stillRelevant: () => current.some((item) => item.stillRelevant?.()),
                },
          );
        }, 2000),
      };
      this.groups.set(key, group);
    }
    group.intents.set(intent.eventId, intent);
  }
  reset() {
    this.epoch++;
    for (const group of this.groups.values()) clearTimeout(group.timer);
    this.groups.clear();
    this.revisions.clear();
    this.phases.clear();
    this.contexts.clear();
    this.taskContexts.clear();
  }
}
