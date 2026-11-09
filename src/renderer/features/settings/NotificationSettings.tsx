import { useEffect, useId, useState } from 'react';
import { ChevronsRightIcon } from 'lucide-react';
import type { Preferences, PreferencesPatch } from '../../../contracts/generated/runtime';
import type {
  SystemNotificationsApi,
  SystemNotificationStatus,
} from '../../../contracts/system-notifications';
import { notificationText } from '../../../shared/i18n/notification-messages';
import { Card, CardContent } from '../../components/ui/card';
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldGroup,
} from '../../components/ui/field';
import { Button } from '../../components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '../../components/ui/alert';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';
import { Switch } from '../../components/ui/switch';
interface Props {
  api: SystemNotificationsApi;
  preferences: Preferences | null;
  disabled: boolean;
  locale: 'en' | 'zh-CN';
  onUpdate(patch: PreferencesPatch): Promise<unknown>;
}
export function NotificationSettings({ api, preferences, disabled, locale, onUpdate }: Props) {
  const id = useId();
  const [status, setStatus] = useState<SystemNotificationStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const t = (key: Parameters<typeof notificationText>[0]) => notificationText(key, locale);
  useEffect(() => {
    let alive = true;
    let request = 0;
    const accept = (next: SystemNotificationStatus) => {
      if (alive)
        setStatus((current) => (!current || next.version >= current.version ? next : current));
    };
    const unsubscribe = api.onStatusChanged((next) => {
      request += 1;
      accept(next);
    });
    const refresh = () => {
      const currentRequest = ++request;
      void api
        .getStatus()
        .then((next) => {
          if (currentRequest === request) accept(next);
        })
        .catch(() => {
          if (alive && currentRequest === request)
            setStatus((current) => (current ? { ...current, authorization: 'unknown' } : null));
        });
    };
    refresh();
    window.addEventListener('focus', refresh);
    return () => {
      alive = false;
      unsubscribe();
      window.removeEventListener('focus', refresh);
    };
  }, [api]);
  const authorized = status?.authorization === 'authorized';
  const mode = preferences?.systemNotificationMode ?? 'unfocused';
  const modes = (['off', 'unfocused', 'always'] as const).map((value) => ({
    value,
    label: t(value),
  }));
  const switches = [
    ['attentionRequiredNotificationEnabled', 'attention', 'attentionDescription'],
    ['executionIssueNotificationEnabled', 'issue', 'issueDescription'],
  ] as const;
  const settingsButton = (
    <Button
      variant="link"
      size="sm"
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        setFailed(false);
        try {
          const result = await api.openSystemSettings();
          setFailed(result.outcome !== 'opened');
        } catch {
          setFailed(true);
        } finally {
          setBusy(false);
        }
      }}
    >
      {t('systemSettings')}
      <ChevronsRightIcon data-icon="inline-end" aria-hidden="true" />
    </Button>
  );
  return (
    <div className="flex flex-col gap-4">
      {!authorized ? (
        <div className="flex flex-col gap-2">
          <Alert variant="warning">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <AlertTitle>{t('permissionRequired')}</AlertTitle>
                <AlertDescription>{t('permissionDescription')}</AlertDescription>
              </div>
              {settingsButton}
            </div>
          </Alert>
          {failed ? (
            <Alert variant="warning" role="status">
              <AlertDescription>{t('failed')}</AlertDescription>
            </Alert>
          ) : null}
        </div>
      ) : null}
      <Card appearance="settings">
        <CardContent className="p-0">
          <FieldGroup className="gap-0">
            <Field orientation="responsive" appearance="settings" data-disabled={disabled}>
              <FieldContent>
                <FieldLabel htmlFor={`${id}-mode`}>{t('mode')}</FieldLabel>
                <FieldDescription>{t('currentScope')}</FieldDescription>
              </FieldContent>
              <Select
                items={modes}
                value={mode}
                disabled={disabled}
                onValueChange={(value) => {
                  if (value === 'off' || value === 'unfocused' || value === 'always')
                    void onUpdate({ systemNotificationMode: value });
                }}
              >
                <SelectTrigger id={`${id}-mode`} size="sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {modes.map((item) => (
                      <SelectItem key={item.value} value={item.value}>
                        {item.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
            {switches.map(([field, label, description]) => (
              <Field
                key={field}
                orientation="responsive"
                appearance="settings"
                data-disabled={disabled || mode === 'off'}
              >
                <FieldContent>
                  <FieldLabel htmlFor={`${id}-${field}`}>{t(label)}</FieldLabel>
                  <FieldDescription id={`${id}-${field}-description`}>
                    {t(description)}
                  </FieldDescription>
                </FieldContent>
                <Switch
                  id={`${id}-${field}`}
                  aria-describedby={`${id}-${field}-description`}
                  checked={preferences?.[field] ?? true}
                  disabled={disabled || mode === 'off'}
                  onCheckedChange={(value) => void onUpdate({ [field]: value })}
                />
              </Field>
            ))}
          </FieldGroup>
        </CardContent>
      </Card>
    </div>
  );
}
