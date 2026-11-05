import type {
  ExecutionPolicy,
  ExecutionRequest,
  ExecutionState,
  ScriptReviewBinding,
  ConnectionConfig,
  ProxyConfig,
  TunnelRule,
  PreferencesPatch,
  UiStateData,
  UpdatePreferencesRequest,
  WriteOutcome,
  WorkspaceReply,
} from './index';
import {
  executionPolicyValid,
  executionRequestValid,
  executionStateValid,
  scriptReviewBindingValid,
} from './execution-validation';

const bytes = (value: string) => new TextEncoder().encode(value).length;
const control = /\p{Cc}/u;
const named = (value: string, limit: number) =>
  value.trim().length > 0 && bytes(value.trim()) <= limit && !control.test(value.trim());
const ipv4 = (value: string) =>
  /^(?:0|[1-9]\d{0,2})(?:\.(?:0|[1-9]\d{0,2})){3}$/.test(value) &&
  value.split('.').every((v) => Number(v) <= 255);
const ipv6 = (value: string) => {
  if (!value.includes(':') || /[%[\]\s]/.test(value)) return false;
  try {
    return new URL(`http://[${value}]/`).hostname.startsWith('[');
  } catch {
    return false;
  }
};
function host(value: string): boolean {
  let v = value.trim();
  if (v.startsWith('[') && v.endsWith(']')) {
    v = v.slice(1, -1);
    if (!ipv6(v)) return false;
  }
  return (
    v.length > 0 &&
    bytes(v) <= 255 &&
    !/[\s\p{Cc}/\\@?#[\]"']/u.test(v) &&
    (!v.includes(':') || ipv6(v))
  );
}
function tunnel(v: TunnelRule): boolean {
  return (
    named(v.name, 128) &&
    (ipv4(v.bindHost) || ipv6(v.bindHost)) &&
    (v.targetHost === null || (!/[[\]]/.test(v.targetHost) && host(v.targetHost))) &&
    (v.kind === 'socks5'
      ? v.targetHost === null && v.targetPort === null
      : v.targetHost !== null && v.targetPort !== null)
  );
}
function connection(v: ConnectionConfig): boolean {
  const tunnels = v.tunnels ?? [];
  return (
    named(v.name, 128) &&
    host(v.host) &&
    named(v.username, 128) &&
    bytes(v.notes ?? '') <= 4096 &&
    !(v.notes ?? '').includes('\0') &&
    new Set(tunnels.map((t) => t.id)).size === tunnels.length
  );
}
function proxy(v: ProxyConfig): boolean {
  return (
    named(v.name, 128) &&
    host(v.host) &&
    !/[[\]]/.test(v.host.trim()) &&
    bytes(v.username) <= 255 &&
    !control.test(v.username) &&
    (v.kind !== 'http' || !v.username.includes(':'))
  );
}
function preferences(v: PreferencesPatch): boolean {
  return (
    (!v.filesColumns ||
      (v.filesColumns[0] === 'name' && new Set(v.filesColumns).size === v.filesColumns.length)) &&
    (v.downloadDirectory === undefined || !v.downloadDirectory.includes('\0')) &&
    (v.monitorSampleSeconds === undefined ||
      v.monitorFullSeconds === undefined ||
      v.monitorFullSeconds === (v.monitorSampleSeconds === 15 ? 120 : 60))
  );
}
function uiState(v: UiStateData): boolean {
  const paths = v.kind === 'directory' ? [v.path, ...v.expanded] : [v.upload, v.download];
  return (
    paths.every((p) => !p.includes('\0')) &&
    bytes(JSON.stringify(v)) <= 32000 &&
    (v.kind !== 'directory' || v.expanded.reduce((n, p) => n + bytes(p), 0) <= 24000)
  );
}
export function validUiScope(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    (value === 'pickers' ||
      /^connection:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value))
  );
}
// Schema handles structure; these rules cover only byte budgets and relationships.
// Ajv may invoke a keyword before sibling type checks, so malformed data must
// return false instead of throwing out of the protocol boundary.
export function semanticRule(rule: string, value: unknown): boolean {
  try {
    switch (rule) {
      case 'executionPolicy':
        return executionPolicyValid(value as ExecutionPolicy);
      case 'executionRequest':
        return executionRequestValid(value as ExecutionRequest);
      case 'scriptReviewBinding':
        return scriptReviewBindingValid(value as ScriptReviewBinding);
      case 'executionState':
        return executionStateValid(value as ExecutionState);
      case 'connection':
        return connection(value as ConnectionConfig);
      case 'proxy':
        return proxy(value as ProxyConfig);
      case 'tunnel':
        return tunnel(value as TunnelRule);
      case 'preferences':
        return preferences(value as PreferencesPatch);
      case 'preferencesUpdate': {
        const v = value as UpdatePreferencesRequest;
        return (
          Object.keys(v.patch).length > 0 &&
          Object.keys(v.patch).sort().join(',') === Object.keys(v.base).sort().join(',')
        );
      }
      case 'writeOutcome': {
        const v = value as WriteOutcome;
        return v.status === 'rejected'
          ? Boolean(v.error)
          : Boolean(v.preferences || v.uiState) && !v.error;
      }
      case 'uiState':
        return uiState(value as UiStateData);
      case 'uiScope':
        return validUiScope((value as { scope: unknown }).scope);
      case 'workspaceReply': {
        const v = value as WorkspaceReply;
        return v.kind !== 'reconciled' || v.result === null || v.result.kind !== 'reconciled';
      }
      default:
        return false;
    }
  } catch {
    return false;
  }
}
