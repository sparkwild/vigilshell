import { expect, it } from 'vitest';
import type { ActivitySummary } from '@contracts';
import { activityDuration } from './activity-duration';
const row = {
  taskId: 'task',
  phase: 'running',
  startedAt: 1000,
  finishedAt: null,
  uncertain: false,
} as ActivitySummary;
it('measures observed execution without queue or review time', () => {
  expect(activityDuration(row, 5000)).toBe(4);
  expect(activityDuration({ ...row, phase: 'succeeded', finishedAt: 4000 }, 50000)).toBe(3);
  expect(activityDuration({ ...row, phase: 'queued' }, 5000)).toBeNull();
  expect(activityDuration({ ...row, phase: 'waitingReview' }, 5000)).toBeNull();
});
it('never invents totals for unknown or asynchronous outcomes', () => {
  expect(activityDuration({ ...row, uncertain: true }, 5000)).toBe('unknown');
  expect(activityDuration({ ...row, phase: 'interruptedUnknown', finishedAt: 4000 }, 5000)).toBe(
    'unknown',
  );
  expect(activityDuration({ ...row, phase: 'requestAccepted' }, 5000)).toBeNull();
  expect(activityDuration({ ...row, taskId: null }, 5000)).toBeNull();
  expect(activityDuration({ ...row, phase: 'succeeded', startedAt: null }, 5000)).toBe('unknown');
  expect(activityDuration(row, 0)).toBe('unknown');
});
