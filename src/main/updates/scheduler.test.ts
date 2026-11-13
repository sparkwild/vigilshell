import { afterEach, expect, it, vi } from 'vitest';
import { initialSchedule, rateLimitUntil, UpdateScheduler, updateDay } from './scheduler';
afterEach(() => vi.useRealTimers());
it('waits for readiness, coalesces wakeups and persists the daily deadline', async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000000);
  const check = vi.fn(async () => scheduler.finish(true));
  const scheduler = new UpdateScheduler(check);
  scheduler.configure(true);
  await vi.advanceTimersByTimeAsync(29999);
  expect(check).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(check).toHaveBeenCalledOnce();
  const record = { ...scheduler.record };
  scheduler.stop();
  const restored = new UpdateScheduler(check);
  restored.restore(record);
  restored.configure(true);
  await vi.advanceTimersByTimeAsync(updateDay - 1);
  expect(check).toHaveBeenCalledOnce();
  restored.resume();
  restored.resume();
  await vi.advanceTimersByTimeAsync(1);
  expect(check).toHaveBeenCalledTimes(2);
  restored.stop();
});
it('backs off failures, honors cooldown and suspends pending installation checks', async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000000);
  const check = vi.fn(async () => scheduler.finish(false));
  const scheduler = new UpdateScheduler(check);
  scheduler.configure(true);
  for (const delay of [30000, 900000, 3600000, 14400000, 14400000]) {
    const calls = check.mock.calls.length;
    await vi.advanceTimersByTimeAsync(delay - 1);
    expect(check).toHaveBeenCalledTimes(calls);
    await vi.advanceTimersByTimeAsync(1);
    expect(check).toHaveBeenCalledTimes(calls + 1);
  }
  scheduler.pause(true);
  await vi.advanceTimersByTimeAsync(updateDay);
  expect(check).toHaveBeenCalledTimes(5);
  scheduler.resume();
  await vi.advanceTimersByTimeAsync(1);
  expect(check).toHaveBeenCalledTimes(5);
  scheduler.pause(false);
  await vi.advanceTimersByTimeAsync(1);
  expect(check).toHaveBeenCalledTimes(6);
  scheduler.configure(false);
  await vi.advanceTimersByTimeAsync(updateDay);
  expect(check).toHaveBeenCalledTimes(6);
  scheduler.stop();
});
it('uses retry headers and prevents a manual cooldown being bypassed on restart', () => {
  const now = 1700000000000;
  expect(rateLimitUntil(new Response('', { headers: { 'retry-after': '120' } }), now)).toBe(
    now + 120000,
  );
  expect(
    rateLimitUntil(
      new Response('', { headers: { 'retry-after': new Date(now + 120000).toUTCString() } }),
      now,
    ),
  ).toBe(now + 120000);
  const scheduler = new UpdateScheduler(async () => {});
  scheduler.restore({ ...initialSchedule(), notBefore: now + 120000 });
  expect(scheduler.record.notBefore).toBe(now + 120000);
  scheduler.stop();
});
