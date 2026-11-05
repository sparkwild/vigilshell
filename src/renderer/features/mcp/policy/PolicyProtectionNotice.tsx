import type { ProtectionCategory, SystemProtection } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { FieldDescription, FieldGroup } from '../../../components/ui/field';
import { ShieldCheckIcon } from 'lucide-react';
import { Button } from '../../../components/ui/button';
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../../../components/ui/dialog';
import type { MessageKey } from '../../../../shared/i18n/messages';
const categories: Record<ProtectionCategory, MessageKey> = {
  core: 'policy_protection_core',
  criticalConfiguration: 'policy_protection_critical',
  credential: 'policy_protection_credentials',
  control: 'policy_protection_control',
};
const maintenance: Record<string, MessageKey> = {
  preserve_container: 'policy_protection_container',
  packages: 'policy_protection_packages',
  accounts: 'policy_protection_accounts_route',
  services: 'policy_protection_services',
  scheduled_tasks: 'policy_protection_schedules',
  validated_configuration: 'policy_protection_validated',
  package_sources: 'policy_protection_sources',
  public_settings_only: 'policy_protection_public',
  application_control: 'policy_protection_application',
  system_settings: 'policy_system_settings',
  network_settings: 'policy_protection_network_route',
  login_policy: 'policy_protection_login_route',
};
export function PolicyProtectionNotice({ rules }: { rules: SystemProtection[] }) {
  const { t } = useDesktop();
  return (
    <div className="min-w-0 px-4 py-3">
      <Dialog>
        <DialogTrigger render={<Button variant="outline" />}>
          <ShieldCheckIcon data-icon="inline-start" />
          {t('policy_system_protection')}
        </DialogTrigger>
        <DialogContent
          size="wide"
          closeLabel={t('ws_close')}
          className="flex max-h-[min(40rem,calc(100dvh-4rem))] flex-col overflow-hidden"
        >
          <DialogHeader className="shrink-0 pr-6">
            <DialogTitle>{t('policy_system_protection')}</DialogTitle>
            <DialogDescription>{t('policy_protection_hint')}</DialogDescription>
          </DialogHeader>
          <FieldGroup className="min-h-0 min-w-0 gap-4 overflow-y-auto overscroll-contain">
            {rules.length ? (
              <dl className="flex flex-col gap-3">
                {(Object.entries(categories) as [ProtectionCategory, MessageKey][]).map(
                  ([category, label]) => (
                    <div key={category} className="min-w-0">
                      <dt className="mb-2 font-medium">{t(label)}</dt>
                      <dd className="flex flex-col gap-1 text-sm text-muted-foreground">
                        {rules
                          .filter((r) => r.category === category)
                          .map((rule) => (
                            <span
                              key={`${rule.path}-${rule.kind}`}
                              className="[overflow-wrap:anywhere]"
                            >
                              <code>{rule.path}</code> ·{' '}
                              {t(
                                rule.kind === 'file'
                                  ? 'policy_protection_object'
                                  : 'policy_protection_subtree',
                              )}
                              {' · '}
                              {t(maintenance[rule.maintenance] ?? 'policy_protection_validated')}
                            </span>
                          ))}
                      </dd>
                    </div>
                  ),
                )}
              </dl>
            ) : (
              <FieldDescription>{t('policy_protection_loading')}</FieldDescription>
            )}
            <FieldDescription>{t('policy_protection_accounts')}</FieldDescription>
          </FieldGroup>
        </DialogContent>
      </Dialog>
    </div>
  );
}
