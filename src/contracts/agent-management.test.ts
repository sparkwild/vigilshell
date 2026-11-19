import { expect, it } from 'vitest';
import { contractValidator } from './validation';
import { defaultPolicy } from '../renderer/features/mcp/model';

const validate = contractValidator('AgentUiRequest');
it.each([
  { action: 'manualConnection', connectionId: 'connection' },
  { action: 'createAccess', name: 'entry' },
  { action: 'associateAccess', id: 'entry', client: 'dsh' },
  { action: 'openSessionView', sessionId: 'session' },
  { action: 'terminalSnapshot', taskId: 'task' },
  { action: 'activity', sessionId: 'session', taskId: null, cursor: 0, includeOutput: true },
  { action: 'output', taskId: 'task', cursor: 0 },
  { action: 'cancel', taskId: 'task' },
  {
    action: 'observeFiles',
    sessionId: 'session',
    target: { scopeId: '__account__', path: '/' },
    after: null,
  },
])('rejects the retired desktop action $action at the IPC boundary', (request) => {
  expect(validate(request)).toBe(false);
});

it('keeps capability preview, history and the current access workflow in the desktop contract', () => {
  for (const request of [
    { action: 'previewPolicy', policy: defaultPolicy('connection') },
    { action: 'recordings', filter: { offset: 0 } },
    { action: 'activitySummaries', filter: {} },
    { action: 'task', taskId: 'task' },
    { action: 'tasks', sessionId: null, connectionId: null, entryId: null, phase: null, offset: 0 },
    {
      action: 'prepareAccess',
      client: 'dsh',
      name: 'DeepSeek Harness',
      id: null,
      delivery: 'prompt',
      requestKey: 'setup',
    },
  ])
    expect(validate(request), request.action).toBe(true);
});

it('keeps runtime view states without the retired history routing flag', () => {
  const view = {
    context: {
      id: 'session',
      entryId: 'entry',
      entryName: 'Cursor',
      connectionId: 'connection',
      connectionName: 'Server',
      connectionRevision: 1,
      phase: 'failed',
      cwd: '/',
      environment: {},
      revision: 1,
      createdAt: 1,
      error: 'authorization_expired',
    },
    runningTasks: 0,
  };
  const validateView = contractValidator('AgentSessionView');
  expect(validateView(view)).toBe(true);
  expect(validateView({ ...view, archived: true })).toBe(false);
});

it.each([null, 0, 1, 2_147_483_648, 0xffff_ffff, 0x1_0000_0000])(
  'applies seconds boundary %s to both the policy and the single-field IPC patch',
  (value) => {
    const accepted = value === null || (value > 0 && value <= 0xffff_ffff);
    for (const key of [
      'maxTaskSeconds',
      'terminalSeconds',
      'terminalIdleSeconds',
      'transferSeconds',
      'transferStallSeconds',
    ] as const) {
      const policy = defaultPolicy('connection');
      policy.execution.limits[key] = value;
      expect(validate({ action: 'previewPolicy', policy })).toBe(accepted);
      expect(
        validate({
          action: 'updatePolicySettings',
          connectionId: 'connection',
          connectionRevision: 0,
          expectedRevision: 0,
          patch: { field: 'limit', value: { key, value } },
        }),
      ).toBe(accepted);
    }
  },
);
it.each([null, 0, 1, 16, 17])('applies concurrency boundary %s at IPC', (value) => {
  expect(
    validate({
      action: 'updatePolicySettings',
      connectionId: 'connection',
      connectionRevision: 0,
      expectedRevision: 0,
      patch: { field: 'limit', value: { key: 'concurrentTasks', value } },
    }),
  ).toBe(value !== null && value >= 1 && value <= 16);
});
