import type { ActivityRecord, ActivitySummary, ActivitySummaryFilter, AgentTask } from '@contracts';

export function taskSummary(task: AgentTask): ActivitySummary {
  return {
    id: `t:${task.id}`,
    entryId: task.entryId,
    sessionId: task.contextId,
    taskId: task.id,
    sequence: null,
    createdAt: task.createdAt,
    connectionName: task.connectionName,
    entryName: task.entryName ?? '',
    kind: task.tool,
    summary: task.command || task.targets.map((target) => target.path).join(', '),
    phase: task.phase,
    startedAt:
      task.executionState?.dispatch.kind === 'attempted'
        ? task.executionState.dispatch.attemptedAt
        : null,
    finishedAt: task.finishedAt,
    uncertain:
      task.phase === 'interruptedUnknown' || task.executionState?.outcome?.kind === 'unknown',
  };
}

export function eventSummary(record: ActivityRecord): ActivitySummary | null {
  const body = record.body;
  if (
    !['session', 'policy', 'decision', 'control', 'fileChanged', 'gap', 'historical'].includes(
      body.kind,
    )
  )
    return null;
  if (body.kind === 'fileChanged' && record.taskId) return null;
  const summary =
    body.kind === 'session'
      ? `session:${body.phase[0]!.toUpperCase()}${body.phase.slice(1)}`
      : body.kind === 'control'
        ? body.action
        : body.kind === 'decision'
          ? body.allowed
            ? 'allowed'
            : 'denied'
          : body.kind === 'fileChanged'
            ? body.paths.join(', ')
            : '';
  return {
    id: `e:${record.sessionId}:${record.sequence}`,
    entryId: body.kind === 'session' ? body.entryId : undefined,
    sessionId: record.sessionId,
    taskId: null,
    sequence: record.sequence,
    createdAt: record.timestampMs,
    connectionName: '',
    entryName: body.kind === 'session' ? body.entryName : '',
    kind: body.kind,
    summary,
    phase: null,
    startedAt: null,
    finishedAt: null,
    uncertain: false,
  };
}

export function mergeActivity(
  base: ActivitySummary[],
  changes: ActivitySummary[],
  filter: ActivitySummaryFilter,
) {
  const rows = new Map(base.map((item) => [item.id, item]));
  for (const item of changes) {
    const old = rows.get(item.id);
    rows.set(item.id, {
      ...item,
      entryId: item.entryId || old?.entryId,
      connectionName: item.connectionName || old?.connectionName || '',
      entryName: item.entryName || old?.entryName || '',
    });
  }
  const query = filter.query?.toLocaleLowerCase() ?? '';
  return [...rows.values()]
    .filter((item) => {
      const active = ['queued', 'waitingReview', 'running', 'stopping'].includes(item.phase ?? '');
      return (
        (filter.active == null || filter.active === active) &&
        (!filter.sessionId || filter.sessionId === item.sessionId) &&
        (!query ||
          `${item.entryName} ${item.connectionName} ${item.kind} ${item.summary}`
            .toLocaleLowerCase()
            .includes(query))
      );
    })
    .sort((a, b) => b.createdAt - a.createdAt || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0));
}
