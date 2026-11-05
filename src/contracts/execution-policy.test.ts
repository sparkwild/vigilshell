import { expect, it } from 'vitest';
import { contractValidator } from './validation';
import { defaultPolicy } from '../renderer/features/mcp/model';
it('accepts only the single current policy and rejects old fields', () => {
  const validate = contractValidator('ConnectionPolicy'),
    policy = defaultPolicy('connection');
  expect(validate(policy)).toBe(true);
  expect(validate({ ...policy, enabled: true })).toBe(false);
  expect(validate({ ...policy, execution: { ...policy.execution, version: 1 } })).toBe(false);
  expect(
    validate({ ...policy, execution: { ...policy.execution, services: { manage: true } } }),
  ).toBe(false);
});
it('closes every action branch and rejects removed tool names', () => {
  const validate = contractValidator('AgentCall');
  const request = {
    tool: 'session_manage',
    action: 'open',
    connection_id: 'connection',
    request_key: 'key',
  };
  expect(validate(request)).toBe(true);
  expect(validate({ ...request, uid: 0 })).toBe(false);
  expect(validate({ ...request, action: 'close' })).toBe(false);
  expect(validate({ tool: 'session_open', connection_id: 'connection', request_key: 'key' })).toBe(
    false,
  );
  const command = {
    tool: 'command_run',
    action: 'command',
    session_id: 's',
    request_key: 'k',
    command: 'pwd',
    directory: '/srv',
    timeout_seconds: null,
  };
  expect(validate(command)).toBe(true);
  expect(validate({ ...command, source: { kind: 'inline', text: 'pwd' } })).toBe(false);
  expect(validate({ ...command, protect_system: false })).toBe(false);
});

it('separates live access from historical policy and rejects gate changes in field patches', () => {
  const validate = contractValidator('AgentUiRequest');
  const update = {
    action: 'updatePolicySettings',
    connectionId: 'connection',
    connectionRevision: 1,
    expectedRevision: 0,
    patch: { field: 'verifyConnect', value: false },
  };
  expect(validate(update)).toBe(true);
  expect(validate({ ...update, accessEnabled: true })).toBe(false);
  expect(validate({ ...update, patch: { field: 'accessEnabled', value: true } })).toBe(false);
  expect(validate({ action: 'savePolicy', policy: defaultPolicy('connection') })).toBe(false);
  const live = contractValidator('ConnectionPolicySettings');
  expect(live(defaultPolicy('connection'))).toBe(false);
  expect(live({ ...defaultPolicy('connection'), accessEnabled: false })).toBe(true);
});
