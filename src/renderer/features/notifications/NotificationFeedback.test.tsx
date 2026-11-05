// @vitest-environment jsdom
import { act, cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { NotificationFeedback } from '@contracts';
import { notificationApiFixture } from '../../../../tests/fixtures/notifications';
import {
  NotificationFeedbackProvider,
  useNotificationSurface,
} from './NotificationFeedbackProvider';
import { NotificationFeedbackStore } from './feedback-store';
const stores: NotificationFeedbackStore[] = [];
afterEach(() => {
  cleanup();
  stores.splice(0).forEach((store) => store.dispose());
  vi.restoreAllMocks();
});
function Surface() {
  const ref = useNotificationSurface('fileEditor:editor');
  return <div ref={ref}>Editing problem</div>;
}
it('only acknowledges visible details while application subscriptions remain independent', async () => {
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
  const api = notificationApiFixture();
  const item: NotificationFeedback = {
    id: 'event',
    title: 'Failed',
    body: 'Retained',
    category: 'executionIssue',
    route: 'pending',
    relevant: true,
    target: { kind: 'fileEditor', id: 'editor' },
    relatedTargets: [{ kind: 'fileEditor', id: 'editor' }],
  };
  api.getFeedback = vi.fn(async () => [item]);
  api.reportPresented = vi.fn(async () => {});
  const store = new NotificationFeedbackStore(api);
  stores.push(store);
  store.start();
  const view = render(
    <NotificationFeedbackProvider store={store}>
      <span>Editing entry</span>
    </NotificationFeedbackProvider>,
  );
  await act(async () => {});
  expect(api.reportPresented).not.toHaveBeenCalled();
  view.rerender(
    <NotificationFeedbackProvider store={store}>
      <Surface />
    </NotificationFeedbackProvider>,
  );
  await waitFor(() => expect(api.reportPresented).toHaveBeenCalledWith(['event']));
  view.unmount();
  await store.refresh();
  expect(api.reportPresented).toHaveBeenCalledOnce();
});
