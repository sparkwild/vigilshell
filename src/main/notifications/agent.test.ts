import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { AgentTask } from '@contracts';
import { AgentNotificationCoordinator, agentNotificationIntent } from '../mcp/notifications';
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());
const task = (id: string, phase: AgentTask['phase'], revision = 1) =>
  ({ id, phase, revision, contextId: 'context' }) as AgentTask;
it('aggregates only matching session, category and result type in two seconds', async () => {
  const publish = vi.fn().mockResolvedValue(undefined);
  const coordinator = new AgentNotificationCoordinator({ publish, locale: () => 'en' });
  coordinator.observe({ kind: 'task', task: task('a', 'succeeded') });
  coordinator.observe({ kind: 'task', task: task('b', 'failed') });
  coordinator.observe({ kind: 'task', task: task('c', 'failed') });
  await vi.advanceTimersByTimeAsync(2000);
  expect(publish).toHaveBeenCalledOnce();
  expect(publish.mock.calls[0]![0]).toMatchObject({
    category: 'executionIssue',
    body: expect.stringContaining('2'),
    eventId: expect.stringContaining('agent:activity:context:'),
  });
});
it.each(['succeeded', 'cancelled', 'denied', 'requestAccepted'] as const)(
  'does not announce %s as success',
  (phase) => {
    expect(agentNotificationIntent({ kind: 'task', task: task('a', phase) }, 'en')).toBeNull();
  },
);
it('invalidates stale review and runtime timers without replaying events', async () => {
  const publish = vi.fn().mockResolvedValue(undefined);
  const coordinator = new AgentNotificationCoordinator({ publish, locale: () => 'en' });
  coordinator.observe({
    kind: 'attention',
    id: 'a',
    connection_id: 'c',
    code: 'script_review_required',
  });
  coordinator.observe({ kind: 'task', task: task('a', 'running') });
  await vi.advanceTimersByTimeAsync(2000);
  expect(publish).not.toHaveBeenCalled();
  coordinator.observe({ kind: 'task', task: task('a', 'failed', 2) });
  coordinator.reset();
  await vi.advanceTimersByTimeAsync(2000);
  expect(publish).not.toHaveBeenCalled();
});
it('does not send an old aggregation after a planned backend generation change', async () => {
  let runtime = 'first';
  const publish = vi.fn().mockResolvedValue(undefined);
  const coordinator = new AgentNotificationCoordinator({
    publish,
    locale: () => 'en',
    runtime: () => runtime,
  });
  coordinator.observe({ kind: 'task', task: task('a', 'failed') });
  runtime = 'second';
  await vi.advanceTimersByTimeAsync(2000);
  expect(publish).not.toHaveBeenCalled();
});
