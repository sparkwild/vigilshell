import { SettingsGroup } from '../SettingsParts';
import type { ExecutionLimits } from '@contracts';
import type { MessageKey } from '../../../../shared/i18n/messages';
import { useDesktop } from '../../../app/DesktopProvider';
import { FieldDescription, FieldGroup, FieldSet, FieldLegend } from '../../../components/ui/field';
import { PolicyInput as Input } from './PolicyInput';
import { ChoiceSelect } from '../../settings/components/SettingsControls';
import { PolicyRow as Row } from './PolicyField';

type LimitField = {
  key: keyof ExecutionLimits;
  label: MessageKey;
  hint?: MessageKey;
  optional?: 'unlimited' | 'off';
  unit?: number;
  fallback?: number;
};
const groups: { title: MessageKey; fields: LimitField[] }[] = [
  {
    title: 'policy_tasks_heading',
    fields: [
      {
        key: 'maxTaskSeconds',
        label: 'policy_max_task',
        hint: 'policy_task_limit_hint',
        optional: 'unlimited',
        unit: 60,
        fallback: 1800,
      },
      {
        key: 'concurrentTasks',
        label: 'policy_concurrent_tasks',
        hint: 'policy_concurrent_tasks_hint',
      },
    ],
  },
  {
    title: 'policy_terminals_heading',
    fields: [
      {
        key: 'terminalSeconds',
        label: 'policy_terminal_limit',
        hint: 'policy_terminal_limit_hint',
        optional: 'unlimited',
        unit: 60,
        fallback: 7200,
      },
      {
        key: 'terminalIdleSeconds',
        label: 'policy_idle',
        hint: 'policy_idle_hint',
        optional: 'off',
        unit: 60,
        fallback: 900,
      },
      {
        key: 'concurrentTerminals',
        label: 'policy_concurrent_terminals',
        hint: 'policy_concurrent_terminals_hint',
      },
    ],
  },
  {
    title: 'policy_transfers_heading',
    fields: [
      {
        key: 'transferSeconds',
        label: 'policy_transfer_limit',
        hint: 'policy_transfer_limit_hint',
        optional: 'unlimited',
        unit: 60,
        fallback: 1800,
      },
      {
        key: 'transferStallSeconds',
        label: 'policy_transfer_stall',
        hint: 'policy_transfer_stall_hint',
        optional: 'off',
        unit: 1,
        fallback: 300,
      },
      {
        key: 'concurrentTransfers',
        label: 'policy_concurrent_transfers',
        hint: 'policy_concurrent_transfers_hint',
      },
    ],
  },
];
export function PolicyLimits({
  limits,
  busy,
  change,
}: {
  limits: ExecutionLimits;
  busy: boolean;
  change(value: ExecutionLimits): void;
}) {
  const { t } = useDesktop();
  return (
    <FieldGroup className="gap-4">
      {[groups.slice(0, 2), groups.slice(2)].map((section, index) => (
        <section className="settings-section" key={index}>
          <h2 className="section-title">
            {t(index === 0 ? 'policy_task_terminal_section' : 'policy_transfer_limits_section')}
          </h2>
          {index === 0 ? <FieldDescription>{t('policy_limits_global')}</FieldDescription> : null}
          <SettingsGroup>
            {section.map((group) => (
              <FieldSet key={group.title} className="gap-0">
                <FieldLegend
                  className={group.title === 'policy_terminals_heading' ? 'px-4 pt-4' : 'sr-only'}
                >
                  {t(group.title)}
                </FieldLegend>
                {group.title === 'policy_terminals_heading' ? (
                  <FieldDescription className="px-4 pb-2">
                    {t('policy_limits_display')}
                  </FieldDescription>
                ) : null}
                {group.fields.map((field) => {
                  const value = limits[field.key],
                    id = `policy-limit-${field.key}`;
                  const invalid =
                    value != null &&
                    (!Number.isInteger(value) ||
                      value < 1 ||
                      value > (field.optional ? 2147483647 : 16));
                  return (
                    <Row
                      key={field.key}
                      id={id}
                      label={t(field.label)}
                      description={field.hint ? t(field.hint) : undefined}
                      invalid={invalid}
                      disabled={busy}
                    >
                      <div className="flex flex-wrap items-center gap-2">
                        {field.optional ? (
                          <ChoiceSelect
                            id={`${id}-mode`}
                            value={value == null ? 'none' : 'bounded'}
                            disabled={busy}
                            items={[
                              {
                                value: 'none',
                                label: t(
                                  field.optional === 'off' ? 'policy_off' : 'policy_unlimited',
                                ),
                              },
                              { value: 'bounded', label: t('policy_set_limit') },
                            ]}
                            onChange={(mode) =>
                              change({
                                ...limits,
                                [field.key]: mode === 'none' ? null : field.fallback,
                              })
                            }
                          />
                        ) : null}
                        {value != null ? (
                          <>
                            <Input
                              id={id}
                              aria-label={t(field.label)}
                              className="w-20"
                              type="number"
                              min={field.unit ? 1 / field.unit : 1}
                              max={field.optional ? 2147483647 / (field.unit ?? 1) : 16}
                              step={field.unit === 60 ? 'any' : 1}
                              disabled={busy}
                              value={value / (field.unit ?? 1) || ''}
                              aria-invalid={invalid}
                              onChange={(e) =>
                                change({
                                  ...limits,
                                  [field.key]: Math.round(
                                    Number(e.target.value) * (field.unit ?? 1),
                                  ),
                                })
                              }
                            />
                            {field.unit ? (
                              <FieldDescription>
                                {t(field.unit === 60 ? 'policy_minute' : 'policy_seconds')}
                              </FieldDescription>
                            ) : null}
                          </>
                        ) : null}
                      </div>
                    </Row>
                  );
                })}
              </FieldSet>
            ))}
          </SettingsGroup>
        </section>
      ))}
    </FieldGroup>
  );
}
