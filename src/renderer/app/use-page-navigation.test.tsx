// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { usePageNavigation } from './use-page-navigation';
it('guards all navigation and ignores overlapping navigation while a decision is pending', async () => {
  const { result } = renderHook(() => usePageNavigation());
  let finish!: (v: boolean) => void;
  const guard = vi.fn(
      () =>
        new Promise<boolean>((r) => {
          finish = r;
        }),
    ),
    first = vi.fn(),
    second = vi.fn();
  result.current.register(guard);
  const pending = result.current.navigate(first);
  expect(await result.current.navigate(second)).toBe(false);
  await act(async () => {
    finish(false);
    await pending;
  });
  expect(first).not.toHaveBeenCalled();
  expect(second).not.toHaveBeenCalled();
  result.current.register(async () => true);
  await act(async () => {
    await result.current.navigate(second);
  });
  expect(second).toHaveBeenCalledOnce();
});
it('does not run a pending navigation after its owner is unmounted', async () => {
  const { result, unmount } = renderHook(() => usePageNavigation());
  let finish!: (v: boolean) => void;
  result.current.register(
    () =>
      new Promise<boolean>((r) => {
        finish = r;
      }),
  );
  const action = vi.fn();
  const pending = result.current.navigate(action);
  unmount();
  finish(true);
  expect(await pending).toBe(false);
  expect(action).not.toHaveBeenCalled();
});
it('keeps both overlay drafts and settings drafts when notification navigation is declined', async () => {
  const { result } = renderHook(() => usePageNavigation());
  const action = vi.fn(),
    pageGuard = vi.fn(async () => true);
  result.current.register(pageGuard);
  result.current.registerOverlay(async () => false);
  expect(await result.current.navigate(action)).toBe(false);
  expect(action).not.toHaveBeenCalled();
  expect(pageGuard).not.toHaveBeenCalled();
  result.current.registerOverlay(async () => true);
  result.current.register(async () => false);
  expect(await result.current.navigate(action)).toBe(false);
  expect(action).not.toHaveBeenCalled();
  result.current.register(async () => true);
  expect(await result.current.navigate(action)).toBe(true);
  expect(action).toHaveBeenCalledOnce();
});
