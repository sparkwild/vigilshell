// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useDelayedBusy } from './use-delayed-busy';
afterEach(() => vi.useRealTimers());
it('suppresses brief saves and resets the delay for the next operation', () => {
  vi.useFakeTimers();
  const { result, rerender } = renderHook(({ busy, id }) => useDelayedBusy(busy, id), {
    initialProps: { busy: true, id: 'a' },
  });
  act(() => vi.advanceTimersByTime(150));
  expect(result.current).toBe(false);
  rerender({ busy: false, id: 'a' });
  act(() => vi.advanceTimersByTime(500));
  expect(result.current).toBe(false);
  rerender({ busy: true, id: 'b' });
  act(() => vi.advanceTimersByTime(299));
  expect(result.current).toBe(false);
  act(() => vi.advanceTimersByTime(1));
  expect(result.current).toBe(true);
  rerender({ busy: false, id: 'b' });
  expect(result.current).toBe(false);
});
