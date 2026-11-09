import { expect, it, vi } from 'vitest';
import type { AgentTask } from '@contracts';
import { SystemNotificationService } from '../notifications/service';
import { agentNotificationIntent } from './notifications';

it('delivers script confirmation once, ignores success and preserves task navigation', async () => {
  const deliver = vi.fn().mockResolvedValue('submitted');
  const service = new SystemNotificationService({
    supported: () => true,
    authorization: async () => 'authorized',
    preferences: () => ({
      systemNotificationMode: 'always',
      attentionRequiredNotificationEnabled: true,
      executionIssueNotificationEnabled: true,
    }),
    focused: () => true,
    deliver,
    openSettings: async () => undefined,
    dispose: () => undefined,
  });
  const attention = agentNotificationIntent(
    {
      kind: 'attention',
      id: 'script',
      connection_id: 'connection',
      code: 'script_review_required',
    },
    'en',
  )!;
  const task = { id: 'script', phase: 'succeeded', contextId: 'session' } as AgentTask;
  expect(agentNotificationIntent({ kind: 'task', task }, 'en')).toBeNull();
  expect((await service.publish(attention)).outcome).toBe('submitted');
  expect((await service.publish(attention)).outcome).toBe('duplicate');
  expect(deliver).toHaveBeenCalledOnce();
  expect(attention.eventId.split(':').slice(0, 3)).toEqual(['agent', 'task', 'script']);
  expect(attention.target).toMatchObject({ kind: 'agentTask', taskId: 'script', review: true });
  expect(
    agentNotificationIntent({ kind: 'task', task: { ...task, phase: 'waitingReview' } }, 'en'),
  ).toBeNull();
  service.stop();
});
