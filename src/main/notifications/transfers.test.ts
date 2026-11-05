import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { TransferView, NotificationOutcome } from '../../contracts/generated/runtime';
import type { SystemNotificationService } from './service';
import { TransferNotificationCoordinator } from './transfers';
import { transferFeedbackKey } from '../../shared/transfer-feedback';
const task = (id: string, phase: TransferView['phase'], revision = 1, attempt = 0, batchSize = 2) =>
  ({
    id,
    phase,
    revision,
    attempt,
    batchSize,
    batchId: 'batch',
    connectionId: 'secret-host',
    direction: 'upload',
    conflictVersion: 0,
    name: 'secret-name',
    remotePath: '/secret',
  }) as TransferView;
function setup(outcome: NotificationOutcome = 'submitted') {
  const publish = vi.fn<SystemNotificationService['publish']>(async () => ({
    outcome,
    status: { authorization: 'authorized' as const, version: 1 },
  }));
  const c = new TransferNotificationCoordinator({
    publish,
    runtime: () => 'runtime',
    focused: () => false,
    locale: () => 'en',
  });
  return { c, publish };
}
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());
it('waits for all members and excludes stale events', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'complete'));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
  c.observe(task('b', 'failed', 2));
  c.observe(task('b', 'running', 1));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).toHaveBeenCalledTimes(1);
  expect(JSON.stringify(publish.mock.calls)).not.toMatch(/secret/);
});
it('keeps successful retries silent after a failed round', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'failed'));
  c.observe(task('b', 'failed'));
  await vi.advanceTimersByTimeAsync(1000);
  c.observe(task('a', 'running', 2, 1));
  c.observe(task('a', 'complete', 3, 1));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).toHaveBeenCalledOnce();
  expect(publish.mock.calls[0]?.[0]).toMatchObject({ category: 'executionIssue' });
});
it('cancels pending summaries on retry and runtime reset', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'complete'));
  c.observe(task('b', 'failed'));
  await vi.advanceTimersByTimeAsync(500);
  c.observe(task('b', 'running', 2, 1));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
  c.observe(task('b', 'complete', 3, 1));
  c.reset();
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
});
it.each(['complete', 'cancelled', 'skipped'] as const)('does not notify all %s', async (phase) => {
  const { c, publish } = setup();
  c.observe(task('a', phase));
  c.observe(task('b', phase));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
});
it('does not notify historical or subsequent live successes', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'complete'), true);
  c.observe(task('b', 'complete'), true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
  c.observe(task('b', 'running', 2, 1));
  c.observe(task('b', 'complete', 3, 1));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
});
it('coalesces conflicts and allows a new episode after resolution', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'awaitingDecision'));
  c.observe(task('b', 'awaitingDecision'));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).toHaveBeenCalledTimes(1);
  c.observe(task('a', 'running', 2));
  c.observe(task('b', 'running', 2));
  c.observe({ ...task('a', 'awaitingDecision', 3), conflictVersion: 1 });
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).toHaveBeenCalledTimes(2);
});
it('suppresses locally presented failures', async () => {
  const { c, publish } = setup();
  const a = task('a', 'failed');
  c.observe(a);
  c.observe(task('b', 'complete'));
  c.report([transferFeedbackKey(a)!]);
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
});
it.each(['failed', 'suppressed', 'unknown', 'submitted'] as const)(
  'does not repeat %s delivery',
  async (outcome) => {
    const x = setup(outcome);
    x.c.observe(task('a', 'failed'));
    x.c.observe(task('b', 'complete'));
    await vi.advanceTimersByTimeAsync(1000);
    expect(x.publish).toHaveBeenCalledOnce();
    x.c.observe(task('a', 'failed'));
    await vi.advanceTimersByTimeAsync(1000);
    expect(x.publish).toHaveBeenCalledOnce();
  },
);
it('keeps a successful 100-task batch silent', async () => {
  const { c, publish } = setup();
  for (let i = 0; i < 100; i++) c.observe(task(String(i), 'complete', 1, 0, 100));
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).not.toHaveBeenCalled();
});

it('includes skipped child counts without exposing child paths', async () => {
  const { c, publish } = setup();
  c.observe({
    ...task('a', 'failed', 1, 1, 1),
    details: {
      stage: 'finished',
      completed: 2,
      failed: 0,
      skipped: 3,
      pending: 0,
    },
  });
  await vi.advanceTimersByTimeAsync(1000);
  expect(publish).toHaveBeenCalledOnce();
  expect(publish.mock.calls[0]![0].body).toContain('3 child items skipped');
  expect(JSON.stringify(publish.mock.calls)).not.toContain('secret');
});
it('does not announce a user-cancelled mixed batch as successful completion', async () => {
  const { c, publish } = setup();
  c.observe(task('a', 'complete'));
  c.observe(task('b', 'cancelled'));
  await vi.advanceTimersByTimeAsync(1100);
  expect(publish).not.toHaveBeenCalled();
});
