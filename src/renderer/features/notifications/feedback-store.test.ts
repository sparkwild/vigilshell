// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import type { NotificationFeedback } from '@contracts';
import { notificationApiFixture } from '../../../../tests/fixtures/notifications';
import { NotificationFeedbackStore } from './feedback-store';
const stores: NotificationFeedbackStore[] = [];
afterEach(() => {
  stores.splice(0).forEach((store) => store.dispose());
  vi.restoreAllMocks();
});
function fixture() {
  const api = notificationApiFixture();
  let changed = () => {};
  const unsubscribe = vi.fn();
  api.onFeedbackChanged = vi.fn((listener) => {
    changed = listener;
    return unsubscribe;
  });
  api.getFeedback = vi.fn(async () => []);
  const store = new NotificationFeedbackStore(api);
  stores.push(store);
  return { api, store, unsubscribe, changed: () => changed() };
}
const notice = (id: string): NotificationFeedback => ({
  id,
  title: 'Failure',
  body: 'Retained',
  category: 'executionIssue',
  route: 'local',
  relevant: true,
  target: { kind: 'runtime' },
  relatedTargets: [{ kind: 'runtime' }],
});
it('subscribes once and continues refreshing without any view subscribers', async () => {
  const f = fixture();
  f.store.start();
  f.store.start();
  expect(f.api.onFeedbackChanged).toHaveBeenCalledOnce();
  const stopView = f.store.subscribe(vi.fn());
  stopView();
  vi.mocked(f.api.getFeedback).mockResolvedValue([notice('headless')]);
  f.changed();
  await vi.waitFor(() => expect(f.store.getSnapshot()[0]?.id).toBe('headless'));
  expect(f.unsubscribe).not.toHaveBeenCalled();
  f.store.dispose();
  f.store.dispose();
  expect(f.unsubscribe).toHaveBeenCalledOnce();
});
it('rejects stale snapshots and preserves confirmed state when refresh fails', async () => {
  const f = fixture();
  let resolve!: (items: NotificationFeedback[]) => void;
  vi.mocked(f.api.getFeedback).mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  f.store.start();
  vi.mocked(f.api.getFeedback).mockResolvedValue([notice('new')]);
  await f.store.refresh();
  resolve([notice('old')]);
  await Promise.resolve();
  expect(f.store.getSnapshot()[0]?.id).toBe('new');
  vi.mocked(f.api.getFeedback).mockRejectedValue(Error('unavailable'));
  await f.store.refresh();
  expect(f.store.getSnapshot()[0]?.id).toBe('new');
});
it('ignores pending responses and detaches focus handlers after disposal', async () => {
  const f = fixture();
  let resolve!: (items: NotificationFeedback[]) => void;
  vi.mocked(f.api.getFeedback).mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  f.store.start();
  f.store.dispose();
  resolve([notice('late')]);
  await Promise.resolve();
  window.dispatchEvent(new Event('focus'));
  expect(f.store.getSnapshot()).toEqual([]);
  expect(f.api.getFeedback).toHaveBeenCalledOnce();
});
