import type { RuntimeRequest, WorkspaceReply } from '@contracts';
import { contractValidator } from '../../src/contracts/validation';

const request = contractValidator<RuntimeRequest>('RuntimeRequest');
const reply = contractValidator<WorkspaceReply>('WorkspaceReply');

export function isRuntimeRequest(value: unknown): value is RuntimeRequest {
  return (
    request(value) &&
    value.jsonrpc === '2.0' &&
    value.id.length > 0 &&
    Buffer.byteLength(value.id) <= 128 &&
    Object.keys(value).every((k) => ['jsonrpc', 'id', 'method', 'params'].includes(k))
  );
}

export function isWorkspaceReply(value: unknown): value is WorkspaceReply {
  return reply(value);
}
