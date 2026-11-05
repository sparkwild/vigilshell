import type { TerminalSessionView, TunnelStatus, WorkspaceOperation } from '@contracts';
import type { NotificationFeedbackCoordinator } from './feedback';
import { translate } from '../../shared/i18n/messages';
export const connectionFailureCodes = new Set([
  'disconnected',
  'tools_unavailable',
  'connection_interrupted',
  'connection_closed',
  'connection_io',
  'connection_timeout',
  'transport_error',
]);
export class WorkspaceNotificationCoordinator {
  private sessions = new Map<string, { phase: string; connectionId: string; groupId: string }>();
  private tunnels = new Map<string, string>();
  private attempts = new Map<string, { attempt: number; phase: string }>();
  private incidents = new Set<string>();
  private sequence = 0;
  constructor(
    private ports: {
      runtime(): string;
      locale(): 'en' | 'zh-CN';
      feedback: NotificationFeedbackCoordinator;
      planned(): boolean;
    },
  ) {}
  session(session: TerminalSessionView, snapshot = false) {
    const old = this.sessions.get(session.id);
    if (snapshot && old) return;
    this.sessions.set(session.id, {
      phase: session.phase,
      connectionId: session.connectionId,
      groupId: session.groupId,
    });
    if (session.phase === 'running') {
      this.incidents.delete(session.groupId);
      this.ports.feedback.connectionReady(this.ports.runtime(), session.connectionId);
    }
    if (snapshot || !old || old.phase !== 'running' || this.ports.planned()) return;
    const unexpected =
      session.phase === 'failed' ||
      session.endReason?.kind === 'transportError' ||
      session.endReason?.kind === 'remoteClosed' ||
      (session.endReason?.kind === 'remoteExit' && session.exitCode !== 0);
    if (
      !unexpected ||
      ['userDisconnected', 'cancelled'].includes(session.endReason?.kind ?? '') ||
      this.incidents.has(session.groupId)
    )
      return;
    this.incidents.add(session.groupId);
    const runtime = this.ports.runtime();
    this.ports.feedback.failConnection(runtime, session.connectionId);
    void this.ports.feedback.publish({
      eventId: `connection:${runtime}:${session.groupId}:${++this.sequence}`,
      category: 'executionIssue',
      runtimeId: runtime,
      connectionId: session.connectionId,
      target: {
        kind: 'connection',
        runtimeId: runtime,
        connectionId: session.connectionId,
        sessionId: session.id,
        tunnelId: null,
      },
      title: translate('notification_connection_lost', this.ports.locale()),
      body: translate('notification_connection_lost_body', this.ports.locale()),
      stillRelevant: () =>
        this.ports.runtime() === runtime && this.sessions.get(session.id)?.phase === session.phase,
    });
  }
  tunnel(connectionId: string, states: TunnelStatus[], snapshot = false) {
    const runtime = this.ports.runtime();
    for (const state of states) {
      const key = `${connectionId}:${state.ruleId}`,
        old = this.tunnels.get(key);
      this.tunnels.set(key, state.phase);
      if (snapshot || this.ports.planned() || old !== 'running' || state.phase !== 'failed')
        continue;
      void this.ports.feedback.publish({
        eventId: `tunnel:${runtime}:${state.ruleId}:${++this.sequence}`,
        runtimeId: runtime,
        connectionId,
        cause: 'connection',
        category: 'executionIssue',
        target: {
          kind: 'connection',
          runtimeId: runtime,
          connectionId,
          sessionId: null,
          tunnelId: state.ruleId,
        },
        title: translate('notification_tunnel_failed', this.ports.locale()),
        body: translate('notification_tunnel_failed_body', this.ports.locale()),
        stillRelevant: () => this.ports.runtime() === runtime && this.tunnels.get(key) === 'failed',
      });
    }
  }
  operation(operation: WorkspaceOperation, snapshot = false) {
    const summary = operation.fileSummary;
    if (operation.kind !== 'files' || !summary || !operation.sessionId) return;
    const old = this.attempts.get(operation.id);
    if (
      old &&
      (summary.attempt < old.attempt ||
        (summary.attempt === old.attempt && old.phase === operation.phase))
    )
      return;
    this.attempts.set(operation.id, { attempt: summary.attempt, phase: operation.phase });
    this.ports.feedback.refresh();
    if (snapshot || this.ports.planned() || operation.phase !== 'failed') return;
    const runtime = this.ports.runtime();
    void this.ports.feedback.publish({
      eventId: `file-operation:${runtime}:${operation.id}:${summary.attempt}:${operation.phase}`,
      runtimeId: runtime,
      connectionId: summary.connectionId,
      cause:
        operation.error === 'runtime_unavailable'
          ? 'runtime'
          : connectionFailureCodes.has(operation.error ?? '')
            ? 'connection'
            : undefined,
      category: 'executionIssue',
      target: {
        kind: 'fileOperation',
        runtimeId: runtime,
        sessionId: operation.sessionId,
        operationId: operation.id,
      },
      title: translate('notification_file_operation_failed', this.ports.locale()),
      body: translate('notification_file_operation_result', this.ports.locale(), {
        completed: summary.completed,
        failed: summary.failed,
        skipped: summary.skipped,
      }),
      stillRelevant: () =>
        this.ports.runtime() === runtime &&
        this.attempts.get(operation.id)?.attempt === summary.attempt &&
        this.attempts.get(operation.id)?.phase === operation.phase,
    });
  }
  reset() {
    this.sessions.clear();
    this.tunnels.clear();
    this.attempts.clear();
    this.incidents.clear();
    this.ports.feedback.refresh();
  }
}
