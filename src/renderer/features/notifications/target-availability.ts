import type { NotificationTarget } from '@contracts';
import type { WorkspaceSnapshot } from '../connections/workspace-store';
/** Notification navigation only uses targets in the current workspace. */
export function hasNotificationTarget(target: NotificationTarget, state: WorkspaceSnapshot) {
  switch (target.kind) {
    case 'runtime':
    case 'mcp':
      return true;
    case 'fileEditor':
      return true; // The editing service validates the session on click.
    case 'agentTask':
    case 'agentContext':
    case 'agentActivity':
      return state.agentViews.some((view) => view.context.id === target.contextId);
    case 'transfer':
      return (
        target.runtimeId === state.runtimeId &&
        state.sessions.some((session) => session.connectionId === target.connectionId) &&
        [...state.transfers.values()].some(
          (task) => task.connectionId === target.connectionId && task.batchId === target.batchId,
        )
      );
    case 'fileOperation':
      return (
        target.runtimeId === state.runtimeId &&
        state.sessions.some((session) => session.id === target.sessionId) &&
        state.operations.has(target.operationId)
      );
    case 'connection':
      return (
        target.runtimeId === state.runtimeId &&
        (target.sessionId
          ? state.sessions.some(
              (session) =>
                session.id === target.sessionId && session.connectionId === target.connectionId,
            )
          : state.sessions.some((session) => session.connectionId === target.connectionId) &&
            !!state.tunnelStates
              .get(target.connectionId)
              ?.some((tunnel) => tunnel.ruleId === target.tunnelId))
      );
  }
}
