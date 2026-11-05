import type { AgentUiRequest } from '@contracts';

export function verificationDeclined(code: string) {
  return [
    'authorization_cancelled',
    'authorization_failed',
    'authorization_timed_out',
    'authorization_interrupted',
  ].includes(code);
}

// These native verification outcomes occur before commit; keep the existing state.
export function quietAuthorizationFailure(request: AgentUiRequest, code: string) {
  if (!verificationDeclined(code)) return false;
  switch (request.action) {
    case 'approveConnection':
    case 'approveScript':
      return true;
    case 'setPolicyAccess':
      return request.enabled;
    default:
      return false;
  }
}
