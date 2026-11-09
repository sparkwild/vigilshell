import { m } from './generated/messages.js';
import type { ResolvedLocale, ErrorCode } from '@contracts';
export type MessageKey = keyof typeof m;
export type MessageInputs = Record<string, string | number>;
export function translate(
  key: MessageKey,
  locale: ResolvedLocale,
  inputs: MessageInputs = {},
): string {
  const render = m[key] as (inputs: MessageInputs, options: { locale: ResolvedLocale }) => string;
  return render(inputs, { locale });
}
export const errorMessage: Record<ErrorCode, MessageKey> = {
  revision_limit: 'settings_revision_limit',
  runtime_unavailable: 'error_host_unavailable',
  result_unknown: 'settings_result_unknown',
  invalid_params: 'error_invalid_request',
  invalid_request: 'error_invalid_request',
  conflict: 'error_conflict',
  storage_unavailable: 'error_storage_unavailable',
  protocol_mismatch: 'error_protocol_mismatch',
  host_unavailable: 'error_host_unavailable',
  timeout: 'error_timeout',
  cancelled: 'error_cancelled',
  too_late: 'error_too_late',
  busy: 'error_busy',
  shutting_down: 'error_shutting_down',
};
