import { describe, expect, it, vi } from 'vitest';
import {
  SystemNotificationService,
  type NotificationPorts,
  type SystemNotificationIntent,
} from './service';

function setup() {
  const preferences = {
    systemNotificationMode: 'unfocused' as 'off' | 'unfocused' | 'always',
    attentionRequiredNotificationEnabled: true,
    executionIssueNotificationEnabled: true,
  };
  const ports: NotificationPorts = {
    supported: () => true,
    authorization: vi.fn<NotificationPorts['authorization']>().mockResolvedValue('authorized'),
    preferences: () => preferences,
    focused: vi.fn(() => true),
    deliver: vi.fn<NotificationPorts['deliver']>().mockResolvedValue('submitted'),
    openSettings: vi.fn<NotificationPorts['openSettings']>().mockResolvedValue(),
    dispose: vi.fn(),
  };
  const service = new SystemNotificationService(ports);
  const intent: SystemNotificationIntent = {
    eventId: 'test-event',
    category: 'executionIssue',
    title: 'Task failed',
    body: 'Open the app',
  };
  return { service, ports, preferences, intent };
}
describe('system notification policy', () => {
  it.each([
    ['off', true, 'suppressed'],
    ['off', false, 'suppressed'],
    ['unfocused', true, 'suppressed'],
    ['unfocused', false, 'submitted'],
    ['always', true, 'submitted'],
    ['always', false, 'submitted'],
  ] as const)('global mode %s with focus %s returns %s', async (mode, focus, expected) => {
    const { service, ports, preferences, intent } = setup();
    preferences.systemNotificationMode = mode;
    vi.mocked(ports.focused).mockReturnValue(focus);
    expect((await service.publish(intent)).outcome).toBe(expected);
    expect(ports.deliver).toHaveBeenCalledTimes(expected === 'submitted' ? 1 : 0);
  });
  it('attention obeys the global mode and its category switch', async () => {
    const { service, preferences, intent } = setup();
    preferences.systemNotificationMode = 'off';
    intent.category = 'attentionRequired';
    expect((await service.publish(intent)).outcome).toBe('suppressed');
    preferences.systemNotificationMode = 'always';
    preferences.attentionRequiredNotificationEnabled = false;
    expect((await service.publish({ ...intent, eventId: 'next' })).outcome).toBe('suppressed');
  });
  it.each(['denied', 'unknown'] as const)(
    'never prompts or sends automatically for %s permission',
    async (authorization) => {
      const { service, ports, preferences, intent } = setup();
      preferences.systemNotificationMode = 'always';
      vi.mocked(ports.authorization).mockResolvedValue(authorization);
      expect((await service.publish(intent)).outcome).toBe('suppressed');
      expect(ports.authorization).toHaveBeenCalledWith(false);
      expect(ports.authorization).not.toHaveBeenCalledWith(true);
      expect(ports.deliver).not.toHaveBeenCalled();
    },
  );
  it.each(['failed', 'unknown', 'submitted'] as const)(
    'does not replay an event after %s',
    async (outcome) => {
      const { service, ports, preferences, intent } = setup();
      preferences.systemNotificationMode = 'always';
      vi.mocked(ports.deliver).mockResolvedValue(outcome);
      expect((await service.publish(intent)).outcome).toBe(outcome);
      expect((await service.publish(intent)).outcome).toBe('duplicate');
      expect(ports.deliver).toHaveBeenCalledOnce();
    },
  );
  it('claims duplicate IDs synchronously while authorization is pending', async () => {
    const { service, ports, preferences, intent } = setup();
    preferences.systemNotificationMode = 'always';
    let resolve!: (value: 'authorized') => void;
    vi.mocked(ports.authorization).mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const first = service.publish(intent);
    expect((await service.publish(intent)).outcome).toBe('duplicate');
    resolve('authorized');
    expect((await first).outcome).toBe('submitted');
  });
  it('stops pending sends and ignores late permission results', async () => {
    const { service, ports, preferences, intent } = setup();
    preferences.systemNotificationMode = 'always';
    let resolve!: (value: 'authorized') => void;
    vi.mocked(ports.authorization).mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const changed = vi.fn();
    service.on('changed', changed);
    const first = service.publish(intent);
    service.stop();
    resolve('authorized');
    expect((await first).outcome).toBe('stopped');
    expect(changed).not.toHaveBeenCalled();
    expect(ports.deliver).not.toHaveBeenCalled();
    expect(ports.dispose).toHaveBeenCalledOnce();
  });
  it('coalesces explicit permission requests and re-reads real authorization', async () => {
    const { service, ports } = setup();
    vi.mocked(ports.authorization)
      .mockResolvedValueOnce('notDetermined')
      .mockResolvedValueOnce('denied');
    const a = service.requestPermission();
    const b = service.requestPermission();
    expect(a).toBe(b);
    expect((await a).authorization).toBe('denied');
    expect(ports.authorization).toHaveBeenNthCalledWith(2, true);
    await service.requestPermission();
    expect(ports.authorization).toHaveBeenCalledTimes(3);
    expect(ports.authorization).toHaveBeenLastCalledWith(false);
  });
  it('does not use native permission APIs on unsupported platforms', async () => {
    const { service, ports } = setup();
    ports.supported = () => false;
    expect((await service.getStatus()).authorization).toBe('unsupported');
    expect((await service.openSystemSettings()).outcome).toBe('suppressed');
    expect(ports.authorization).not.toHaveBeenCalled();
    expect(ports.openSettings).not.toHaveBeenCalled();
  });
});

it('rechecks focus after asynchronous permission lookup', async () => {
  const { service, ports, intent } = setup();
  vi.mocked(ports.focused).mockReturnValue(false);
  let resolve!: (value: 'authorized') => void;
  vi.mocked(ports.authorization).mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const pending = service.publish({
    ...intent,
    category: 'attentionRequired',
  });
  vi.mocked(ports.focused).mockReturnValue(true);
  resolve('authorized');
  expect((await pending).outcome).toBe('suppressed');
  expect(ports.deliver).not.toHaveBeenCalled();
});

it('rechecks transfer relevance after asynchronous permission queries', async () => {
  const { service, ports, preferences, intent } = setup();
  preferences.systemNotificationMode = 'always';
  let resolve!: (value: 'authorized') => void;
  vi.mocked(ports.authorization).mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  let current = true;
  const result = service.publish({
    ...intent,
    category: 'attentionRequired',
    stillRelevant: () => current,
  });
  current = false;
  resolve('authorized');
  expect((await result).outcome).toBe('suppressed');
  expect(ports.deliver).not.toHaveBeenCalled();
});

it('requests native authorization for the first eligible real event and shares concurrent requests', async () => {
  const { service, ports, preferences, intent } = setup();
  preferences.systemNotificationMode = 'always';
  let grant!: (value: 'authorized') => void;
  vi.mocked(ports.authorization).mockImplementation(async (request) =>
    request
      ? await new Promise<'authorized'>((resolve) => {
          grant = resolve;
        })
      : 'notDetermined',
  );
  const first = service.publish(intent);
  const second = service.publish({ ...intent, eventId: 'second' });
  await vi.waitFor(() => expect(grant).toBeTypeOf('function'));
  grant('authorized');
  expect((await first).outcome).toBe('submitted');
  expect((await second).outcome).toBe('submitted');
  expect(vi.mocked(ports.authorization).mock.calls.filter(([request]) => request)).toHaveLength(1);
});
it.each(['attentionRequired', 'executionIssue'] as const)(
  'does not prompt when %s is disabled',
  async (category) => {
    const { service, ports, preferences, intent } = setup();
    preferences.systemNotificationMode = 'always';
    preferences.attentionRequiredNotificationEnabled = false;
    preferences.executionIssueNotificationEnabled = false;
    expect((await service.publish({ ...intent, category })).outcome).toBe('suppressed');
    expect(ports.authorization).not.toHaveBeenCalled();
  },
);
it('rechecks preferences and relevance after native authorization', async () => {
  const { service, ports, preferences, intent } = setup();
  preferences.systemNotificationMode = 'always';
  let grant!: (value: 'authorized') => void;
  vi.mocked(ports.authorization).mockImplementation(async (request) =>
    request
      ? await new Promise<'authorized'>((resolve) => {
          grant = resolve;
        })
      : 'notDetermined',
  );
  const send = service.publish(intent);
  await vi.waitFor(() => expect(grant).toBeTypeOf('function'));
  preferences.systemNotificationMode = 'off';
  grant('authorized');
  expect((await send).outcome).toBe('suppressed');
  expect(ports.deliver).not.toHaveBeenCalled();
});
