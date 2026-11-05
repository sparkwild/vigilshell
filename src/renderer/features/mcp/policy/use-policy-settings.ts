import { verificationDeclined as isVerificationDeclined } from '../authorization-feedback';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ConnectionPolicySettings, ConnectionSummary, PolicySettingsPatch } from '@contracts';
import type { useMcp } from '../model';
import type { RegisterLeaveGuard } from '../SettingsParts';
import { useDesktop } from '../../../app/DesktopProvider';
import { useDecision } from '../../../shared/ConfirmationProvider';

export async function setPolicyAccess(
  model: ReturnType<typeof useMcp>,
  connection: ConnectionSummary,
  policy: ConnectionPolicySettings,
  enabled: boolean,
) {
  let verificationDeclined = false;
  const ok = await model.issue(
    {
      action: 'setPolicyAccess',
      connectionId: connection.id,
      connectionRevision: connection.revision,
      expectedRevision: policy.revision,
      enabled,
    },
    undefined,
    (code) => {
      verificationDeclined = enabled && isVerificationDeclined(code);
      return verificationDeclined;
    },
  );
  if (!ok && !verificationDeclined) await model.refresh();
  return verificationDeclined ? 'unchanged' : ok;
}

export function usePolicySettings(
  connection: ConnectionSummary,
  policy: ConnectionPolicySettings,
  model: ReturnType<typeof useMcp>,
  register: RegisterLeaveGuard,
  unavailable = false,
) {
  const { t } = useDesktop();
  const decide = useDecision();
  const [pending, setPending] = useState(false);
  const [invalid, setInvalid] = useState(false);
  const [failed, setFailed] = useState<PolicySettingsPatch | null>(null);
  const [reset, setReset] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const flight = useRef(false);
  const latest = useRef(policy);
  latest.current = policy;
  const busy = pending || model.busy || unavailable;
  const save = async (patch: PolicySettingsPatch) => {
    if (unavailable || connection.deleting || flight.current || latest.current.accessEnabled)
      return false;
    flight.current = true;
    setPending(true);
    setError(null);
    try {
      const ok = await model.issue(
        {
          action: 'updatePolicySettings',
          connectionId: connection.id,
          connectionRevision: connection.revision,
          expectedRevision: latest.current.revision,
          patch,
        },
        (s) => {
          const p = s.policies.find((p) => p.connectionId === connection.id);
          if (p) latest.current = p;
        },
      );
      setFailed(ok ? null : patch);
      if (!ok) await model.refresh();
      return ok;
    } finally {
      flight.current = false;
      setPending(false);
    }
  };
  const toggle = async (enabled: boolean) => {
    if (unavailable || connection.deleting || flight.current || (enabled && (invalid || failed)))
      return;
    flight.current = true;
    setPending(true);
    setError(null);
    try {
      const ok = await setPolicyAccess(model, connection, latest.current, enabled);
      if (ok === false) {
        setError('policy_access_check_failed');
      }
    } finally {
      flight.current = false;
      setPending(false);
    }
  };
  const leave = useCallback(async () => {
    if (flight.current) return false;
    if (!invalid && !failed) return true;
    const choice = await decide({
      title: t('policy_incomplete_title'),
      description: t('policy_incomplete_hint'),
      actions: [
        { id: 'stay', label: t('agent_continue'), role: 'cancel' },
        { id: 'discard', label: t('agent_discard_leave'), role: 'secondary' },
      ],
    });
    if (choice !== 'discard') return false;
    setInvalid(false);
    setFailed(null);
    setReset((v) => v + 1);
    return true;
  }, [invalid, failed, decide, t]);
  useEffect(() => {
    register(leave);
    return () => register(null);
  }, [register, leave]);
  return {
    save,
    toggle,
    busy,
    invalid,
    setInvalid,
    failed,
    reset,
    error: error ?? model.error,
    editable: !policy.accessEnabled && !busy && !connection.deleting,
    retry: () => failed && save(failed),
  };
}
