import type { MessageKey } from './messages';

const groups: [MessageKey, string[]][] = [
  ['agent_control_busy', ['terminal_control_busy']],
  ['agent_control_released', ['terminal_control_released', 'terminal_already_attached']],
  ['policy_close_to_edit', ['policy_access_enabled']],
  ['policy_access_closed', ['policy_access_closed']],
  ['policy_upgrade_required', ['policy_upgrade_required']],
  ['agent_recording_gap', ['recording_incomplete', 'activity_recording_unavailable']],
  ['agent_auth_cancelled', ['authorization_cancelled']],
  ['agent_auth_timeout', ['authorization_timed_out']],
  ['agent_auth_interrupted', ['authorization_interrupted']],
  ['agent_auth_failed', ['authorization_failed']],
  ['agent_auth_busy', ['authorization_busy']],
  ['agent_auth_window_hidden', ['authorization_window_hidden']],
  ['agent_auth_unavailable', ['system_authentication_unavailable']],
  ['agent_config_conflict', ['configuration_conflict']],
  ['agent_connection_changed', ['connection_changed']],
  ['agent_port_unavailable', ['listener_unavailable']],
  ['agent_access_setup_required', ['access_setup_required']],
  ['agent_access_revoked', ['access_revoked']],
  ['agent_access_not_revoked', ['access_not_revoked']],
  ['agent_access_busy', ['access_busy']],
  ['agent_service_stopped', ['service_stopped']],
  ['agent_install_unavailable', ['client_install_unavailable']],
  ['agent_invalid_settings', ['invalid_settings', 'invalid_policy', 'invalid_limits']],
  ['agent_already_prepared', ['access_already_prepared']],
  ['operation_account_denied', ['account_permission_denied']],
  [
    'operation_path_denied',
    ['file_access_denied', 'path_denied', 'local_transfer_denied', 'scope_creation_denied'],
  ],
  [
    'operation_core_protected',
    [
      'system_core_protected',
      'protected_system_resource',
      'protected_scope_root',
      'protected_descendant',
    ],
  ],
  ['operation_private_protected', ['protected_account_resource', 'protected_runtime_path']],
  [
    'operation_file_changed',
    ['file_commit_conflict', 'invalid_content_hash', 'script_source_changed'],
  ],
  ['operation_edit_ambiguous', ['edit_not_unique']],
  ['operation_text_encoding', ['file_not_utf8', 'script_not_utf8']],
  [
    'operation_file_missing',
    [
      'local_file_not_found',
      'scope_not_found',
      'path_unavailable',
      'scope_unavailable',
      'scope_parent_unavailable',
      'parent_unavailable',
    ],
  ],
  ['operation_transfer_integrity', ['transfer_hash_mismatch']],
  [
    'operation_partial_write',
    ['file_partial_write', 'package_commit_partial', 'transfer_partial_commit'],
  ],
  [
    'operation_result_unknown',
    [
      'file_commit_unknown',
      'local_commit_unknown',
      'remote_result_unknown',
      'service_request_outcome_unknown',
      'power_request_outcome_unknown',
      'terminal_input_unknown',
      'input_delivery_unknown',
      'transfer_input_unknown',
      'script_start_unknown',
    ],
  ],
  ['operation_expired_result', ['result_expired_no_replay']],
  [
    'operation_session_unavailable',
    ['session_not_ready', 'terminal_not_running', 'terminal_unavailable', 'connection_unavailable'],
  ],
  ['operation_scope_expired', ['resource_browser_expired', 'authorization_expired']],
  [
    // Retained only to render existing historical recording errors; no current sandbox path.
    'operation_isolation_unavailable',
    [
      'isolation_unavailable',
      'isolated_runtime_unavailable',
      'sandbox_unavailable',
      'sandbox_not_active',
      'sandbox_probe_failed',
      'sandbox_filter_failed',
      'scope_visibility_unavailable',
      'network_isolation_unavailable',
      'network_filter_unavailable',
      'network_namespace_unavailable',
    ],
  ],
  ['operation_package_unsupported', ['package_manager_unsupported', 'package_backend_unavailable']],
  [
    'operation_configuration_unsupported',
    ['operation_unsupported', 'atomic_cross_filesystem_unsupported'],
  ],
  ['operation_source_invalid', ['invalid_package_source']],
  [
    'operation_source_format',
    ['package_source_contract_unsupported', 'package_source_link_unsupported'],
  ],
  ['operation_source_exists', ['package_source_exists']],
  ['operation_source_key', ['package_key_invalid', 'package_private_key_forbidden']],
  ['operation_user_manager', ['user_service_manager_unavailable']],
  ['operation_user_service_directory', ['unsafe_service_directory']],
  [
    'operation_source_changed',
    [
      'package_source_changed',
      'package_source_unavailable',
      'package_source_key_unavailable',
      'package_source_key_changed',
    ],
  ],
  ['operation_resolver_managed', ['resolver_manager_requires_adapter']],
  ['operation_request_conflict', ['request_conflict']],
  [
    'operation_stopped',
    [
      'cancelled',
      'operation_cancelled',
      'cancelled_before_script',
      'cancelled_before_start',
      'cancelled_confirmed',
    ],
  ],
  ['operation_timeout', ['timeout_confirmed', 'resource_browser_timeout']],
  ['operation_native_permission', ['native_maintenance_denied']],
  [
    'operation_native_unavailable',
    ['native_maintenance_unavailable', 'native_program_unavailable'],
  ],
  [
    'operation_native_failed',
    [
      'native_program_start_failed',
      'native_program_failed',
      'native_package_operation_failed',
      'native_repository_refresh_failed',
      'native_service_failed',
      'native_service_query_failed',
    ],
  ],
  [
    'operation_result_unknown',
    ['native_outcome_unknown', 'native_package_result_unknown', 'native_service_result_unknown'],
  ],
  ['operation_file_changed', ['local_package_changed', 'service_definition_changed']],
  [
    'operation_native_input',
    [
      'invalid_local_package',
      'package_backend_mismatch',
      'invalid_service_definition',
      'invalid_service_drop_in',
      'invalid_service_revision',
    ],
  ],
];
const messages = new Map(
  groups.flatMap(([key, codes]) => codes.map((code) => [code, key] as const)),
);

/** 异常原文及未识别的代码不能成为翻译键或直接进入用户界面。 */
export function userErrorKey(
  code: string | null | undefined,
  fallback: MessageKey = 'ws_error_generic',
): MessageKey {
  if (!code) return fallback;
  if (code.startsWith('partial:'))
    return messages.get(code.slice(8)) === 'operation_result_unknown'
      ? 'operation_result_unknown'
      : 'operation_partial_result';
  const known = messages.get(code);
  if (known) return known;
  if (workspaceCodes.has(code)) return `ws_error_${code}` as MessageKey;
  if (!/^[a-z][a-z0-9_]{0,127}$/.test(code)) return fallback;
  if (code.endsWith('_denied')) return 'operation_policy_denied';
  if (code.endsWith('_unsupported') || code === 'unsupported_tool') return 'operation_unsupported';
  if (code.startsWith('invalid_')) return 'operation_invalid_request';
  if (code.startsWith('package_')) return 'operation_package_failed';
  if (code.startsWith('service_') || code === 'systemd_unavailable')
    return 'operation_service_failed';
  if (code.startsWith('network_') || code.startsWith('host_network_'))
    return 'operation_network_failed';
  if (code.startsWith('runtime_') || code.startsWith('sandbox_'))
    return 'operation_execution_unavailable';
  if (code.startsWith('file_') || code.startsWith('atomic_')) return 'operation_file_failed';
  if (code.startsWith('transfer_') || code.startsWith('local_transfer_'))
    return 'operation_transfer_failed';
  return fallback;
}

const workspaceCodes = new Set([
  'encoding_unsupported',
  'unmappable_input',
  'proxy_failed',
  'proxy_authentication_failed',
  'proxy_unavailable',
  'proxy_timeout',
  'proxy_in_use',
  'tunnel_failed',
  'tunnel_bind_failed',
  'tunnel_remote_refused',
  'tunnel_disconnected',
  'tunnel_target_unreachable',
  'tunnel_accept_failed',
  'tunnel_capacity',
  'tunnel_client_protocol',
  'tunnel_client_closed',
  'tunnel_client_timeout',

  'credential_cleanup_pending',
  'tools_unavailable',
  'files_permission_denied',
  'files_exists',
  'files_unsupported',
  'files_too_large',
  'files_not_empty',
  'invalid_input',
  'renderer_restarted',
  'not_found',
  'conflict',
  'busy',
  'connection_disabled',
  'duplicate_name',
  'storage_unavailable',
  'credential_missing',
  'credential_denied',
  'credential_unavailable',
  'configuration_locked',
  'configuration_corrupt',
  'configuration_format_unsupported',
  'vault_key_missing',
  'vault_key_invalid',
  'credential_corrupt',
  'credential_format_unsupported',
  'invalid_key',
  'unsupported_key_format',
  'wrong_passphrase',
  'native_unavailable',
  'cancelled',
  'too_late',
  'result_unknown',
  'trust_required',
  'trust_changed',
  'trust_expired',
  'resolve_failed',
  'connection_refused',
  'network_unreachable',
  'connection_reset',
  'connection_timeout',
  'algorithm_unsupported',
  'authentication_failed',
  'channel_rejected',
  'pty_rejected',
  'shell_rejected',
  'disconnected',
  'stream_unavailable',
  'runtime_unavailable',
  'protocol_mismatch',
  'timeout',
  'revision_limit',
  'input_backpressure',
  'clipboard_unavailable',
]);
