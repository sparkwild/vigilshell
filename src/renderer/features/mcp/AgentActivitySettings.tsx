import { useCallback, useEffect, useRef, useState } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { FieldDescription } from '../../components/ui/field';
import { NumberPreference, Row } from '../settings/components/SettingsControls';
import { SettingsFooterContent } from '../settings/components/SettingsFooter';
import { useMcp, navigateMcp } from './model';
import { McpFeedback } from './McpFeedback';
import { SettingsGroup, type RegisterLeaveGuard } from './SettingsParts';

export function AgentActivitySettings({
  footer,
  register,
}: {
  footer: HTMLElement | null;
  register: RegisterLeaveGuard;
}) {
  const { t } = useDesktop();
  const model = useMcp();
  const [reset, setReset] = useState(0);
  const [saved, setSaved] = useState(false);
  const pending = useRef<Promise<boolean> | null>(null);
  useEffect(() => {
    register(() => pending.current ?? Promise.resolve(true));
    return () => register(null);
  }, [register]);
  const settings = model.snapshot?.settings;
  const issue = model.issue;
  const save = useCallback(
    (field: 'retentionDays' | 'storageMib', value: number) => {
      if (!settings || settings[field] === value || pending.current) return;
      setSaved(false);
      const next = { ...settings, [field]: value };
      const operation = issue({
        action: 'updateSettings',
        patch: {
          group: 'activity',
          retentionDays: next.retentionDays,
          storageMib: next.storageMib,
          baseRetentionDays: settings.retentionDays,
          baseStorageMib: settings.storageMib,
        },
      })
        .then((ok) => {
          if (!ok) setReset((value) => value + 1);
          setSaved(ok);
          return ok;
        })
        .finally(() => {
          pending.current = null;
        });
      pending.current = operation;
    },
    [settings, issue],
  );
  if (!settings) return <McpFeedback code={model.error} />;
  return (
    <>
      <FieldDescription>{t('agent_records_description')}</FieldDescription>
      <section className="settings-section">
        <h2 className="section-title">{t('agent_records')}</h2>
        <SettingsGroup key={reset}>
          <NumberPreference
            id="agent-retention"
            label={t('agent_retention')}
            description={t('agent_retention_hint')}
            value={settings.retentionDays}
            min={1}
            max={365}
            disabled={model.busy}
            onCommit={(value) => save('retentionDays', value)}
          />
          <NumberPreference
            id="agent-capacity"
            label={t('agent_capacity')}
            description={t('agent_capacity_hint')}
            value={settings.storageMib}
            min={16}
            max={1024}
            disabled={model.busy}
            onCommit={(value) => save('storageMib', value)}
          />
        </SettingsGroup>
      </section>
      <SettingsGroup>
        <Row
          id="agent-history"
          label={t('agent_tasks_and_records')}
          description={t('agent_history_hint')}
        >
          <Button variant="outline" onClick={() => navigateMcp({ view: 'activity' })}>
            {t('agent_view_records')}
          </Button>
        </Row>
      </SettingsGroup>
      <McpFeedback code={model.error} onRetry={() => void model.refresh()} />
      <SettingsFooterContent target={footer}>
        <span className="settings-save-status" role="status" aria-live="polite">
          {model.busy ? t('saving') : saved ? t('agent_saved') : ''}
        </span>
      </SettingsFooterContent>
    </>
  );
}
