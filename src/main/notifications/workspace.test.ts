import { expect, it, vi } from 'vitest';
import type { TerminalSessionView, WorkspaceOperation } from '@contracts';
import type { NotificationFeedbackCoordinator } from './feedback';
import { WorkspaceNotificationCoordinator } from './workspace';
function setup() {
  const publish = vi.fn().mockResolvedValue(undefined),
    failConnection = vi.fn(),
    refresh = vi.fn();
  const feedback = {
    publish,
    failConnection,
    connectionReady: vi.fn(),
    refresh,
  } as unknown as NotificationFeedbackCoordinator;
  return {
    publish,
    failConnection,
    refresh,
    c: new WorkspaceNotificationCoordinator({
      runtime: () => 'runtime',
      locale: () => 'en',
      feedback,
      planned: () => false,
    }),
  };
}
const session = (
  id: string,
  phase: TerminalSessionView['phase'],
  kind?: 'transportError' | 'userDisconnected',
) =>
  ({
    id,
    phase,
    connectionId: 'connection',
    groupId: 'group',
    endReason: kind ? { kind } : null,
  }) as TerminalSessionView;
it('notifies once per connection incident, excluding snapshots and user disconnections', () => {
  const f = setup();
  f.c.session(session('a', 'running'));
  f.c.session(session('b', 'running'));
  f.c.session(session('a', 'failed', 'transportError'));
  f.c.session(session('b', 'failed', 'transportError'));
  expect(f.publish).toHaveBeenCalledOnce();
  expect(f.failConnection).toHaveBeenCalledWith('runtime', 'connection');
  f.c.session(session('c', 'running'));
  f.c.session(session('c', 'closed', 'userDisconnected'));
  expect(f.publish).toHaveBeenCalledOnce();
  f.c.session(session('old', 'failed', 'transportError'), true);
  expect(f.publish).toHaveBeenCalledOnce();
});
it('only reports a previously running tunnel and permits new failures after recovery', () => {
  const f = setup();
  f.c.tunnel('connection', [{ ruleId: 't', phase: 'failed', error: 'x' }]);
  expect(f.publish).not.toHaveBeenCalled();
  for (let n = 0; n < 2; n++) {
    f.c.tunnel('connection', [{ ruleId: 't', phase: 'running', error: null }]);
    f.c.tunnel('connection', [{ ruleId: 't', phase: 'failed', error: 'x' }]);
  }
  expect(f.publish).toHaveBeenCalledTimes(2);
});
const op = (phase: WorkspaceOperation['phase'], elapsedMs = 9999, attempt = 0) =>
  ({
    id: 'job',
    kind: 'files',
    phase,
    sessionId: 'session',
    error: phase === 'failed' ? 'conflict' : null,
    fileSummary: {
      connectionId: 'connection',
      attempt,
      elapsedMs,
      completed: 3,
      failed: phase === 'failed' ? 1 : 0,
      skipped: 0,
    },
  }) as WorkspaceOperation;
it('keeps successes silent regardless of duration and still reports each new failure', () => {
  const f = setup();
  f.c.operation(op('success'));
  expect(f.publish).not.toHaveBeenCalled();
  f.c.operation(op('working', 0, 1));
  f.c.operation(op('failed', 1, 1));
  f.c.operation(op('failed', 1, 1));
  expect(f.publish).toHaveBeenCalledOnce();
  f.c.operation(op('working', 0, 2));
  f.c.operation(op('success', 10000, 2));
  expect(f.publish).toHaveBeenCalledOnce();
  expect(f.publish.mock.calls[0]![0].stillRelevant()).toBe(false);
  expect(f.refresh).toHaveBeenCalled();
  expect(f.publish.mock.calls[0]![0]).toMatchObject({
    category: 'executionIssue',
    target: { kind: 'fileOperation', operationId: 'job' },
  });
  f.c.operation(op('failed', 1, 1));
  expect(f.publish).toHaveBeenCalledOnce();
  f.c.operation(op('failed', 1, 3));
  expect(f.publish).toHaveBeenCalledTimes(2);
});
