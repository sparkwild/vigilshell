import { expect, it, vi } from 'vitest';
import { NotificationFeedbackCoordinator, type FeedbackIntent } from './feedback';
function setup(outcome: 'submitted' | 'unknown' | 'failed' | 'suppressed' = 'submitted') {
  let settle!: () => void;
  const publish = vi.fn(async (intent) => ({
    outcome: intent.stillRelevant() ? outcome : ('suppressed' as const),
    status: { authorization: 'authorized' as const, version: 1 },
  }));
  const changed = vi.fn(),
    activate = vi.fn();
  const coordinator = new NotificationFeedbackCoordinator({
    publish,
    changed,
    activate,
    settle: () =>
      new Promise<void>((resolve) => {
        settle = resolve;
      }),
  });
  const intent: FeedbackIntent = {
    eventId: 'one',
    target: { kind: 'fileEditor', id: 'editor' },
    category: 'executionIssue',
    title: 'Sync interrupted',
    body: 'Local changes retained',
    runtimeId: 'runtime',
    connectionId: 'connection',
  };
  return { coordinator, publish, activate, intent, settle: () => settle() };
}
it('allows the visible surface to claim an event before native delivery and never replays it', async () => {
  const f = setup();
  const pending = f.coordinator.publish(f.intent);
  expect(f.coordinator.list()[0]?.route).toBe('pending');
  f.coordinator.present(['unknown', 'one']);
  f.settle();
  await pending;
  expect(f.coordinator.list()[0]?.route).toBe('presented');
  expect(f.publish.mock.calls[0]![0].stillRelevant()).toBe(false);
  await f.coordinator.publish(f.intent);
  expect(f.publish).toHaveBeenCalledOnce();
});
it.each([
  ['submitted', 'submitted'],
  ['unknown', 'unknown'],
  ['failed', 'local'],
  ['suppressed', 'local'],
] as const)('routes %s to %s without a second delivery', async (outcome, route) => {
  const f = setup(outcome);
  const pending = f.coordinator.publish(f.intent);
  f.settle();
  await pending;
  expect(f.coordinator.list()[0]?.route).toBe(route);
  f.coordinator.refresh();
  expect(f.publish).toHaveBeenCalledOnce();
});
it('keeps known click targets after their source expires, rejecting arbitrary IDs', async () => {
  const f = setup();
  const pending = f.coordinator.publish(f.intent);
  f.settle();
  await pending;
  f.coordinator.invalidate(() => true);
  f.coordinator.activate('foreign');
  expect(f.activate).not.toHaveBeenCalled();
  f.coordinator.activate('one');
  expect(f.activate).toHaveBeenCalledWith({ kind: 'fileEditor', id: 'editor' });
  expect(f.coordinator.list()[0]?.relevant).toBe(false);
});
it('suppresses connection-caused children but keeps independent failures, and clears on reconnect', async () => {
  const f = setup();
  f.coordinator.failConnection('runtime', 'connection');
  let pending = f.coordinator.publish({ ...f.intent, cause: 'connection' });
  f.settle();
  await pending;
  expect(f.coordinator.list()[0]?.relevant).toBe(false);
  pending = f.coordinator.publish({ ...f.intent, eventId: 'independent' });
  f.settle();
  await pending;
  expect(f.coordinator.list()[1]?.route).toBe('submitted');
  f.coordinator.connectionReady('runtime', 'connection');
  pending = f.coordinator.publish({ ...f.intent, eventId: 'new', cause: 'connection' });
  f.settle();
  await pending;
  expect(f.coordinator.list()[2]?.route).toBe('submitted');
});
it('invalidates all dependent events on runtime failure while preserving the root notification', async () => {
  const f = setup();
  f.coordinator.failRuntime('runtime');
  let pending = f.coordinator.publish(f.intent);
  f.settle();
  await pending;
  expect(f.coordinator.list()[0]?.relevant).toBe(false);
  pending = f.coordinator.publish({ ...f.intent, eventId: 'root', target: { kind: 'runtime' } });
  f.settle();
  await pending;
  expect(f.coordinator.list()[1]?.route).toBe('submitted');
});
it('retains a late presentation acknowledgement after local disposition settles', async () => {
  const f = setup('suppressed');
  const pending = f.coordinator.publish(f.intent);
  f.settle();
  await pending;
  expect(f.coordinator.list()[0]?.route).toBe('local');
  f.coordinator.present(['one']);
  f.coordinator.refresh();
  expect(f.coordinator.list()[0]?.route).toBe('presented');
  await f.coordinator.publish(f.intent);
  expect(f.publish).toHaveBeenCalledOnce();
});

it('does not consume a notification when requesting guarded navigation', async () => {
  const f = setup();
  const pending = f.coordinator.publish(f.intent);
  f.settle();
  await pending;
  f.coordinator.activate('one');
  expect(f.activate).toHaveBeenCalledWith(f.intent.target);
  expect(f.coordinator.list()[0]?.route).toBe('submitted');
  f.coordinator.present(['one']);
  expect(f.coordinator.list()[0]?.route).toBe('presented');
});
