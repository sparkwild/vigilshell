import { LockIcon } from 'lucide-react';
import { Badge } from '../../../components/ui/badge';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../../components/ui/tooltip';
import { PolicyLockContext } from './PolicyField';
import { useState, useEffect } from 'react';
import type {
  ConnectionPolicySettings,
  ConnectionPolicy,
  ConnectionSummary,
  PolicySettingsPatch,
} from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { Button } from '../../../components/ui/button';
import { Switch } from '../../../components/ui/switch';
import { Row } from '../../settings/components/SettingsControls';
import { type RegisterLeaveGuard } from '../SettingsParts';
import { McpFeedback } from '../McpFeedback';
import { type useMcp } from '../model';
import { PolicyCapabilities } from './PolicyCapabilities';
import { usePolicySettings } from './use-policy-settings';
import { validPolicy, emptyTargetInputs, applyPolicyPatch, policyPatchKey } from './policy-model';

export function PolicyEditor({
  connection,
  policy,
  model,
  register,
  unavailable = false,
}: {
  connection: ConnectionSummary;
  policy: ConnectionPolicySettings;
  model: ReturnType<typeof useMcp>;
  register: RegisterLeaveGuard;
  unavailable?: boolean;
}) {
  const { t } = useDesktop();
  const state = usePolicySettings(connection, policy, model, register, unavailable);
  const [patches, setPatches] = useState<Record<string, PolicySettingsPatch>>({});
  const [inputs, setInputs] = useState(emptyTargetInputs);
  useEffect(() => {
    setPatches({});
    setInputs(emptyTargetInputs);
  }, [state.reset]);
  const draft = Object.values(patches).reduce(applyPolicyPatch, policy as ConnectionPolicy);
  const busy = !state.editable;
  const edit = async (next: ConnectionPolicy): Promise<boolean> => {
    const field =
      draft.verifyConnect !== next.verifyConnect
        ? 'verifyConnect'
        : (Object.keys(next.execution) as (keyof typeof next.execution)[]).find(
            (k) => JSON.stringify(draft.execution[k]) !== JSON.stringify(next.execution[k]),
          );
    if (!field || field === 'version') {
      if (!state.failed) return true;
      const failedKey = policyPatchKey(state.failed);
      const ok = (await state.retry()) ?? false;
      if (ok)
        setPatches((old) => {
          const next = { ...old };
          delete next[failedKey];
          return next;
        });
      return ok;
    }
    const key =
      field === 'limits'
        ? (Object.keys(next.execution.limits) as (keyof typeof next.execution.limits)[]).find(
            (k) => draft.execution.limits[k] !== next.execution.limits[k],
          )
        : null;
    const patch = (
      field === 'uploadRoots' || field === 'downloadRoots'
        ? { field: 'localDirectory', value: next.execution.uploadRoots[0]?.path ?? null }
        : field === 'limits' && key
          ? { field: 'limit', value: { key, value: next.execution.limits[key] } }
          : { field, value: field === 'verifyConnect' ? next.verifyConnect : next.execution[field] }
    ) as PolicySettingsPatch;
    const patchKey = policyPatchKey(patch);
    const candidate = applyPolicyPatch(policy, patch);
    setPatches((p) => ({ ...p, [patchKey]: patch }));
    if (!validPolicy(candidate)) {
      state.setInvalid(true);
      return false;
    }
    return state.save(patch).then((ok) => {
      if (ok)
        setPatches((p) => {
          const remaining = { ...p };
          delete remaining[patchKey];
          return remaining;
        });
      return ok;
    });
  };
  const incomplete = Object.keys(patches).length > 0 || Object.values(inputs).some(Boolean);
  const { setInvalid } = state;
  useEffect(() => setInvalid(incomplete), [incomplete, setInvalid]);
  return (
    <>
      <div className="agent-policy-heading">
        <div className="settings-content">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h1 className="page-title">{t('agent_policy_title')}</h1>
              <p className="text-sm text-muted-foreground">
                {connection.name} · {connection.host}
              </p>
            </div>
            {policy.accessEnabled ? (
              <Tooltip>
                <TooltipTrigger render={<Badge variant="secondary" />} tabIndex={0}>
                  <LockIcon data-icon="inline-start" />
                  {t('policy_locked')}
                </TooltipTrigger>
                <TooltipContent>{t('policy_locked_hint')}</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
          <p role="status" className="text-sm text-muted-foreground empty:hidden">
            {state.busy ? t('agent_auth_wait') : ''}
          </p>
        </div>
      </div>
      <div className="agent-policy-editor-scroll">
        <div className="settings-content">
          <McpFeedback
            code={state.error}
            onRetry={() => {
              void model.refresh();
            }}
          />
          {state.failed ? (
            <Button
              variant="outline"
              disabled={state.busy}
              onClick={() => {
                const field = state.failed ? policyPatchKey(state.failed) : null;
                void state.retry()?.then((ok) => {
                  if (ok && field)
                    setPatches((p) => {
                      const next = { ...p };
                      delete next[field];
                      return next;
                    });
                });
              }}
            >
              {t('retry')}
            </Button>
          ) : null}
          <div key={state.reset}>
            <PolicyLockContext value={policy.accessEnabled ? t('policy_locked_hint') : undefined}>
              <PolicyCapabilities
                accessControl={
                  <Row
                    id="policy-access"
                    label={t('policy_access_switch')}
                    description={t(
                      policy.accessEnabled ? 'policy_close_to_edit' : 'policy_autosave_hint',
                    )}
                  >
                    <Switch
                      id="policy-access"
                      checked={policy.accessEnabled}
                      disabled={
                        state.busy || (!policy.accessEnabled && (state.invalid || !!state.failed))
                      }
                      onCheckedChange={(v) => void state.toggle(v)}
                    />
                  </Row>
                }
                draft={draft}
                protections={model.snapshot?.protections ?? []}
                connection={connection}
                busy={busy}
                edit={edit}
                inputs={inputs}
                changeInput={(key, value) => {
                  setInputs((v) => ({ ...v, [key]: value }));
                  state.setInvalid(!!value);
                }}
              />
            </PolicyLockContext>
          </div>
        </div>
      </div>
    </>
  );
}
