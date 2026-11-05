import { type ReactNode, useState } from 'react';
import type {
  ConnectionPolicy,
  ConnectionSummary,
  ExecutionPolicy,
  SystemProtection,
} from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { Button } from '../../../components/ui/button';
import { FieldGroup } from '../../../components/ui/field';
import { ChoiceSelect } from '../../settings/components/SettingsControls';
import { PolicyRow as Row } from './PolicyField';
import { SettingsGroup } from '../SettingsParts';
import { PolicyTransferDirectory } from './PolicyTransferDirectory';
import { PolicyLimits } from './PolicyLimits';
import { PolicySwitch } from './PolicySwitch';
import { PolicyResourcePicker } from './PolicyResourcePicker';
import { PolicyProtectionNotice } from './PolicyProtectionNotice';
import { PolicyProtectedPaths } from './PolicyProtectedPaths';
import type { TargetInputs } from './policy-model';

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h2 className="section-title">{title}</h2>
      <SettingsGroup>{children}</SettingsGroup>
    </section>
  );
}
export function PolicyCapabilities({
  draft,
  protections,
  connection,
  busy,
  edit,
  inputs,
  changeInput,
  accessControl,
}: {
  accessControl?: ReactNode;
  draft: ConnectionPolicy;
  protections: SystemProtection[];
  connection: ConnectionSummary;
  busy: boolean;
  edit(value: ConnectionPolicy): Promise<boolean>;
  inputs: TargetInputs;
  changeInput(key: keyof TargetInputs, value: TargetInputs[keyof TargetInputs]): void;
}) {
  const { t } = useDesktop();
  const [picker, setPicker] = useState(false);
  const p = draft.execution;
  const patch = (value: Partial<ExecutionPolicy>) =>
    edit({ ...draft, execution: { ...p, ...value } });
  const chooseLocal = async () => {
    const r = await window.vigilshell.mcp.request({ action: 'selectLocalDirectory' });
    return r.ok ? (r.directory ?? null) : null;
  };
  return (
    <FieldGroup>
      <Section title={t('policy_access_section')}>
        {accessControl}
        <PolicySwitch
          id="policy-verify"
          label={t('policy_verify_session')}
          checked={draft.verifyConnect}
          busy={busy}
          description={t('policy_verify_hint')}
          change={(verifyConnect) => edit({ ...draft, verifyConnect })}
        />
        <Row
          id="policy-level"
          label={t('policy_operation_level')}
          description={t('policy_native_boundary')}
        >
          <ChoiceSelect
            id="policy-level"
            disabled={busy}
            value={p.level}
            onChange={(level) => patch({ level })}
            items={[
              { value: 'readOnly', label: t('policy_level_readonly') },
              { value: 'operate', label: t('policy_level_operate') },
            ]}
          />
        </Row>
      </Section>
      <Section title={t('policy_protection_section')}>
        <PolicySwitch
          id="policy-protection"
          label={t('policy_system_protection')}
          checked={p.protectSystem}
          busy={busy}
          change={(protectSystem) => patch({ protectSystem })}
          description={t('policy_simple_protection_hint')}
        />
        <PolicySwitch
          id="policy-script-review"
          label={t('policy_script_review')}
          checked={p.scriptReview ?? false}
          busy={busy || p.level !== 'operate'}
          reason={p.level !== 'operate' ? t('policy_requires_operate') : undefined}
          change={(scriptReview) => patch({ scriptReview })}
          description={t('policy_script_review_hint')}
        />
        <PolicyProtectionNotice rules={protections} />
      </Section>
      <Section title={t('policy_paths_section')}>
        <Row id="policy-protected-paths" label={t('policy_protected_paths')}>
          <Button variant="outline" disabled={busy} onClick={() => setPicker(true)}>
            {t('policy_choose_server_paths')}
          </Button>
        </Row>
        <PolicyProtectedPaths
          paths={p.protectedPaths}
          busy={busy}
          change={(protectedPaths) => patch({ protectedPaths })}
        />
        {picker ? (
          <PolicyResourcePicker
            connection={connection}
            selected={p.protectedPaths}
            close={() => setPicker(false)}
            apply={(protectedPaths) => patch({ protectedPaths })}
          />
        ) : null}
      </Section>
      <Section title={t('policy_transfer_section')}>
        <PolicySwitch
          id="policy-transfer"
          label={t('policy_file_transfer')}
          checked={p.fileTransfer}
          busy={busy}
          change={(fileTransfer) => patch({ fileTransfer })}
          description={t('policy_simple_transfer_hint')}
        />
        <PolicyTransferDirectory
          policy={draft}
          busy={busy}
          inputs={inputs}
          changeInput={changeInput}
          edit={edit}
          choose={chooseLocal}
        />
      </Section>

      <PolicyLimits limits={p.limits} busy={busy} change={(limits) => patch({ limits })} />
    </FieldGroup>
  );
}
