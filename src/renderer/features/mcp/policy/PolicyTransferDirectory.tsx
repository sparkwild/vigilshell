import type { ConnectionPolicy } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { Button } from '../../../components/ui/button';
import { FieldDescription, FieldLabel, FieldError } from '../../../components/ui/field';
import { PolicyField } from './PolicyField';
import { PolicyInput } from './PolicyInput';
import { PolicySwitch } from './PolicySwitch';
import { validTarget } from '../TargetList';
import type { TargetInputs } from './policy-model';

export function PolicyTransferDirectory({
  policy,
  busy,
  inputs,
  changeInput,
  edit,
  choose,
}: {
  policy: ConnectionPolicy;
  busy: boolean;
  inputs: TargetInputs;
  changeInput(key: keyof TargetInputs, value: TargetInputs[keyof TargetInputs]): void;
  edit(value: ConnectionPolicy): void;
  choose(): Promise<string | null>;
}) {
  const { t } = useDesktop();
  const p = policy.execution;
  const configured = p.uploadRoots.length > 0 || p.downloadRoots.length > 0;
  const enabled = configured || inputs.directoryRestrictionPending;
  const shared =
    p.uploadRoots.length === 1 &&
    p.downloadRoots.length === 1 &&
    p.uploadRoots[0]?.path === p.downloadRoots[0]?.path
      ? p.uploadRoots[0]!.path
      : '';
  const value = inputs.directoryRestrictionPending ? inputs.localDirectory : shared;
  const invalid = !!value && !validTarget(value, true);
  const save = (path: string | null) => {
    const normalized =
      path && path.length > 1 && path.endsWith('/') && !path.endsWith('//')
        ? path.slice(0, -1)
        : path;
    if (normalized !== null && !validTarget(normalized, true)) {
      changeInput('localDirectory', normalized);
      changeInput('directoryRestrictionPending', true);
      return;
    }
    const roots = normalized === null ? [] : [{ id: 'local', path: normalized }];
    edit({ ...policy, execution: { ...p, uploadRoots: roots, downloadRoots: roots } });
    changeInput('localDirectory', '');
    changeInput('directoryRestrictionPending', false);
  };
  return (
    <>
      <PolicySwitch
        id="policy-local-directory-limit"
        label={t('policy_local_directory_limit')}
        description={t('policy_local_directory_limit_hint')}
        checked={enabled}
        busy={busy}
        change={(on) => {
          if (on) changeInput('directoryRestrictionPending', true);
          else save(null);
        }}
      />
      {enabled ? (
        <PolicyField appearance="settings" data-invalid={invalid}>
          <FieldLabel htmlFor="policy-local-directory">{t('policy_local_directory')}</FieldLabel>
          <FieldDescription>{t('policy_local_directory_required')}</FieldDescription>
          <div className="flex w-full min-w-0 flex-wrap gap-2">
            <PolicyInput
              id="policy-local-directory"
              className="min-w-0 flex-1 basis-48"
              value={value}
              placeholder="/Users/me/transfers"
              disabled={busy}
              aria-invalid={invalid}
              aria-describedby={invalid ? 'policy-local-directory-error' : undefined}
              onChange={(event) => save(event.target.value)}
            />
            <Button
              variant="outline"
              disabled={busy}
              onClick={() =>
                void choose().then((path) => {
                  if (path !== null) save(path);
                })
              }
            >
              {t('policy_choose_directory')}
            </Button>
          </div>
          {invalid ? (
            <FieldError id="policy-local-directory-error">
              {t('policy_local_directory_invalid')}
            </FieldError>
          ) : null}
        </PolicyField>
      ) : null}
    </>
  );
}
