import { expect, it, vi } from 'vitest';
import type { AgentReply, AgentMutation } from '@contracts';
import { authorizationReason } from './authorization-reason';
const context = {
  id: 'session',
  entryName: 'DeepSeek Harness',
  connectionName: 'Test server',
  phase: 'waitingUser',
};
function fixture() {
  return {
    request: vi.fn(async (op): Promise<AgentReply> =>
      op.operation === 'contexts'
        ? ({ kind: 'contexts', contexts: [context] } as AgentReply)
        : ({
            kind: 'task',
            task: {
              id: 'task',
              phase: 'waitingReview',
              connectionName: 'Test server',
              command: 'SECRET COMMAND',
            },
          } as AgentReply),
    ),
    connectionName: vi.fn(async () => 'Test server'),
  };
}
const cases: [AgentMutation, string, string][] = [
  [
    { kind: 'approveConnection', context_id: 'session' },
    '验证身份，允许「DeepSeek Harness」连接「Test server」',
    'Verify your identity to allow “DeepSeek Harness” to connect to “Test server”',
  ],
  [
    {
      kind: 'setPolicyAccess',
      connection_id: 'connection',
      connection_revision: 1,
      expected_revision: 2,
      enabled: true,
    },
    '验证身份，开启「Test server」的智能体访问',
    'Verify your identity to enable agent access to “Test server”',
  ],
  [
    { kind: 'approveScript', task_id: 'task' },
    '验证身份，执行「Test server」上的已审阅脚本',
    'Verify your identity to execute the reviewed script on “Test server”',
  ],
];
it.each(cases)(
  'describes the actual target in both languages: %j',
  async (change, chinese, english) => {
    const ports = fixture();
    expect(await authorizationReason(change, 'zh-CN', ports)).toBe(chinese);
    expect(await authorizationReason(change, 'en', ports)).toBe(english);
    if (change.kind === 'setPolicyAccess')
      expect(ports.connectionName).toHaveBeenCalledWith('connection');
    if (change.kind === 'approveScript')
      expect(ports.request).toHaveBeenCalledWith({ operation: 'task', task_id: 'task' });
  },
);
it('refuses missing or no-longer-waiting targets instead of displaying an unrelated generic prompt', async () => {
  const ports = fixture();
  ports.request.mockResolvedValue({ kind: 'contexts', contexts: [] });
  await expect(authorizationReason(cases[0]![0], 'en', ports)).rejects.toThrow(
    'authorization_expired',
  );
  ports.request.mockResolvedValue({
    kind: 'task',
    task: { id: 'task', phase: 'succeeded' },
  } as AgentReply);
  await expect(authorizationReason(cases[2]![0], 'en', ports)).rejects.toThrow(
    'review_not_waiting',
  );
  ports.connectionName.mockRejectedValue(Error('connection_changed'));
  await expect(authorizationReason(cases[1]![0], 'en', ports)).rejects.toThrow(
    'connection_changed',
  );
});
it('keeps dynamic labels single-line and bounded without embedding script bodies', async () => {
  const ports = fixture();
  ports.connectionName.mockResolvedValue('\u202e\n' + '界'.repeat(200));
  const reason = await authorizationReason(cases[1]![0], 'en', ports);
  expect(reason).not.toMatch(/[\p{Cc}\p{Cf}]/u);
  expect(reason).toContain('界'.repeat(96));
  expect(reason).not.toContain('界'.repeat(97));
  expect(await authorizationReason(cases[2]![0], 'en', ports)).not.toContain('SECRET COMMAND');
});
