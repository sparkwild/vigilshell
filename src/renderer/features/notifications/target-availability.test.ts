import { expect, it } from 'vitest';
import type { WorkspaceSnapshot } from '../connections/workspace-store';
import type { NotificationTarget } from '@contracts';
import { hasNotificationTarget } from './target-availability';
const workspace = () =>
  ({
    runtimeId: 'runtime',
    sessions: [{ id: 'terminal', connectionId: 'connection' }],
    agentViews: [{ context: { id: 'agent' } }],
    operations: new Map([['operation', {}]]),
    transfers: new Map([['transfer', { connectionId: 'connection', batchId: 'batch' }]]),
    tunnelStates: new Map([['connection', [{ ruleId: 'tunnel' }]]]),
  }) as unknown as WorkspaceSnapshot;
const targets: NotificationTarget[] = [
  {
    kind: 'transfer',
    runtimeId: 'runtime',
    connectionId: 'connection',
    batchId: 'batch',
    phase: 'failed',
  },
  { kind: 'fileOperation', runtimeId: 'runtime', sessionId: 'terminal', operationId: 'operation' },
  {
    kind: 'connection',
    runtimeId: 'runtime',
    connectionId: 'connection',
    sessionId: 'terminal',
    tunnelId: null,
  },
  {
    kind: 'connection',
    runtimeId: 'runtime',
    connectionId: 'connection',
    sessionId: null,
    tunnelId: 'tunnel',
  },
];
it.each(targets)(
  'requires an existing terminal for $kind even when its result is retained',
  (target) => {
    const state = workspace();
    expect(hasNotificationTarget(target, state)).toBe(true);
    state.sessions = [];
    expect(hasNotificationTarget(target, state)).toBe(false);
  },
);
it('rejects an expired runtime or removed batch without attempting reconstruction', () => {
  const state = workspace();
  state.runtimeId = 'new-runtime';
  expect(hasNotificationTarget(targets[0]!, state)).toBe(false);
  state.runtimeId = 'runtime';
  state.transfers = new Map();
  expect(hasNotificationTarget(targets[0]!, state)).toBe(false);
});
it('does not reopen closed agent tabs', () => {
  const state = workspace();
  const target: NotificationTarget = {
    kind: 'agentTask',
    contextId: 'agent',
    taskId: 'task',
    review: true,
  };
  expect(hasNotificationTarget(target, state)).toBe(true);
  state.agentViews = [];
  expect(hasNotificationTarget(target, state)).toBe(false);
});
