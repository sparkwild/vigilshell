import { expect, it } from 'vitest';
import { userErrorKey } from './operation-errors';
import { translate } from './messages';

it.each([
  '__proto__',
  'constructor',
  'Error: failed at /private/token',
  '<script>secret</script>',
  'unknown_new_failure',
])('never renders unknown code or exception text: %s', (code) => {
  expect(userErrorKey(code)).toBe('ws_error_generic');
  for (const locale of ['en', 'zh-CN'] as const)
    expect(translate(userErrorKey(code), locale)).not.toContain(code);
});
it.each([
  ['native_maintenance_denied', 'operation_native_permission'],
  ['native_package_result_unknown', 'operation_result_unknown'],
  ['partial:native_service_result_unknown', 'operation_result_unknown'],
  ['native_repository_refresh_failed', 'operation_native_failed'],
  ['user_service_manager_unavailable', 'operation_user_manager'],
  ['package_source_changed', 'operation_source_changed'],
  ['package_source_key_changed', 'operation_source_changed'],
  ['package_source_contract_unsupported', 'operation_source_format'],
  ['authorization_timed_out', 'agent_auth_timeout'],
  ['authorization_interrupted', 'agent_auth_interrupted'],
  ['authorization_failed', 'agent_auth_failed'],
  ['authorization_busy', 'agent_auth_busy'],
  ['authorization_window_hidden', 'agent_auth_window_hidden'],
  ['access_setup_required', 'agent_access_setup_required'],
  ['partial:file_commit_unknown', 'operation_result_unknown'],
  ['partial:system_tool_failed', 'operation_partial_result'],
  ['partial:service_denied', 'operation_partial_result'],
  ['file_partial_write', 'operation_partial_write'],
  ['package_commit_partial', 'operation_partial_write'],
  ['terminal_input_unknown', 'operation_result_unknown'],
  ['file_commit_conflict', 'operation_file_changed'],
  ['account_permission_denied', 'operation_account_denied'],
  ['system_core_protected', 'operation_core_protected'],
  ['path_denied', 'operation_path_denied'],
  ['service_registration_denied', 'operation_policy_denied'],
  ['files_permission_denied', 'ws_error_files_permission_denied'],
] as const)('preserves the actionable meaning of %s', (code, key) => {
  expect(userErrorKey(code)).toBe(key);
  for (const locale of ['en', 'zh-CN'] as const) expect(translate(key, locale)).toBeTruthy();
});
