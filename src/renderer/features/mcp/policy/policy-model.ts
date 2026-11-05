import type { ConnectionPolicy, PolicySettingsPatch } from '@contracts';
import type { MessageKey } from '../../../../shared/i18n/messages';
import { validateConnectionPolicy } from '../../../../contracts/generated/renderer-validation';
import { validProtectedPath } from './protected-path';
import { validTarget } from '../TargetList';
export const emptyTargetInputs = { localDirectory: '', directoryRestrictionPending: false };
export type TargetInputs = typeof emptyTargetInputs;
export interface PolicySession {
  selected: string | null;
  initialized: boolean;
  query: string;
  scrollTop: number;
}
export const initialPolicySession: PolicySession = {
  selected: null,
  initialized: false,
  query: '',
  scrollTop: 0,
};
export function validPolicy(policy: ConnectionPolicy) {
  const { connectionId, connectionRevision, revision, verifyConnect, execution } = policy;
  if (
    !validateConnectionPolicy({
      connectionId,
      connectionRevision,
      revision,
      verifyConnect,
      execution,
    })
  )
    return false;
  const p = policy.execution;
  return (
    p.version === 2 &&
    p.protectedPaths.length <= 128 &&
    new Set(p.protectedPaths.map((s) => s.path)).size === p.protectedPaths.length &&
    p.protectedPaths.every(validProtectedPath) &&
    [p.uploadRoots, p.downloadRoots].every(
      (roots) =>
        roots.length <= 128 &&
        new Set(roots.map((r) => r.id)).size === roots.length &&
        new Set(roots.map((r) => r.path)).size === roots.length &&
        roots.every(
          (r) => /^[A-Za-z0-9_-]{1,128}$/.test(r.id) && r.path !== '/' && validTarget(r.path),
        ),
    ) &&
    Object.entries(p.limits).every(
      ([key, v]) =>
        v == null ||
        (Number.isInteger(v) && v > 0 && v <= (key.startsWith('concurrent') ? 16 : 2147483647)),
    )
  );
}
export function policySummary(policy: ConnectionPolicy): MessageKey {
  return policy.execution.level === 'disabled'
    ? 'policy_level_disabled'
    : policy.execution.level === 'readOnly'
      ? 'policy_level_readonly'
      : 'policy_level_operate';
}
export function applyPolicyPatch(
  policy: ConnectionPolicy,
  patch: PolicySettingsPatch,
): ConnectionPolicy {
  if (patch.field === 'localDirectory') {
    const roots = patch.value === null ? [] : [{ id: 'local', path: patch.value }];
    return {
      ...policy,
      execution: { ...policy.execution, uploadRoots: roots, downloadRoots: roots },
    };
  }
  if (patch.field === 'verifyConnect') return { ...policy, verifyConnect: patch.value };
  if (patch.field === 'limit')
    return {
      ...policy,
      execution: {
        ...policy.execution,
        limits: { ...policy.execution.limits, [patch.value.key]: patch.value.value },
      },
    };
  return { ...policy, execution: { ...policy.execution, [patch.field]: patch.value } };
}
export function policyPatchKey(patch: PolicySettingsPatch) {
  return patch.field === 'limit' ? `limit:${patch.value.key}` : patch.field;
}
