import type { MessageKey } from '../../../shared/i18n/messages';
import { errorKey } from './workspace-context';

export function connectionAdvice(code: string): MessageKey {
  if (['resolve_failed'].includes(code)) return 'connection_help_dns';
  if (code === 'connection_refused') return 'connection_help_refused';
  if (['connection_timeout', 'timeout', 'network_unreachable', 'connection_reset'].includes(code))
    return 'connection_help_network';
  if (
    [
      'vault_key_missing',
      'vault_key_invalid',
      'credential_corrupt',
      'credential_format_unsupported',
    ].includes(code)
  )
    return 'connection_help_vault';
  if (code === 'credential_denied') return 'ws_keychain_help';
  if (code === 'authentication_failed') return 'connection_help_auth';
  if (
    [
      'credential_missing',
      'credential_denied',
      'credential_unavailable',
      'vault_key_missing',
      'vault_key_invalid',
      'credential_corrupt',
      'credential_format_unsupported',
      'invalid_key',
      'unsupported_key_format',
      'wrong_passphrase',
      'native_unavailable',
    ].includes(code)
  )
    return 'connection_help_credential';
  if (['trust_required', 'trust_changed', 'trust_expired'].includes(code))
    return 'connection_help_trust';
  if (code === 'algorithm_unsupported') return 'connection_help_algorithm';
  if (['channel_rejected', 'pty_rejected', 'shell_rejected'].includes(code))
    return 'connection_help_terminal';
  if (code.startsWith('proxy_')) return 'connection_help_proxy';
  if (['runtime_unavailable', 'protocol_mismatch', 'stream_unavailable'].includes(code))
    return 'connection_help_local';
  return 'connection_help_unknown';
}
export function connectionFailureTitle(code: string | null): MessageKey {
  return code === 'disconnected'
    ? 'connection_interrupted'
    : !code || code === 'unknown_error'
      ? 'connection_failed_unknown'
      : errorKey(code);
}
