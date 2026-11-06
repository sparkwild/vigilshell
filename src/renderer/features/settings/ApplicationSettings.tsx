import applicationIcon from '../../../../resources/icons/app-icon-source.png';
import { useCallback, useEffect, useState } from 'react';
import { RefreshCwIcon, CodeXmlIcon, ExternalLinkIcon, MessageSquareIcon } from 'lucide-react';
import type { ApplicationInfo } from '@contracts';
import { automaticUpdateChecksEnabled } from '../../../shared/application-features';
import { useApplicationState } from '../../app/ApplicationProvider';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Badge } from '../../components/ui/badge';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { Card, CardContent } from '../../components/ui/card';
import { FieldGroup } from '../../components/ui/field';
import { Separator } from '../../components/ui/separator';
import { Skeleton } from '../../components/ui/skeleton';
import { Spinner } from '../../components/ui/spinner';
import { Switch } from '../../components/ui/switch';
import { Row } from './components/SettingsControls';
import { usePreferences } from './use-preferences';
import { UpdateDialog } from './UpdateDialog';

export function UpdateBadge() {
  const state = useApplicationState();
  const { t } = useDesktop();
  return state.release ? <Badge variant="secondary">{t('updates_available')}</Badge> : null;
}

export function ApplicationSettings({
  checkRequest = 0,
  onCheckRequestHandled,
}: {
  checkRequest?: number;
  onCheckRequestHandled?(): void;
}) {
  const { t } = useDesktop();
  const state = useApplicationState();
  const preferences = usePreferences();
  const [info, setInfo] = useState<ApplicationInfo>();
  const [failure, setFailure] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [updateFailure, setUpdateFailure] = useState(false);
  useEffect(() => {
    let active = true;
    void window.vigilshell.application
      .info()
      .then((value) => {
        if (active) setInfo(value);
      })
      .catch(() => {
        if (active) setFailure(true);
      });
    return () => {
      active = false;
    };
  }, []);
  const check = useCallback(() => {
    setDialogOpen(true);
    setPending(true);
    setUpdateFailure(false);
    void window.vigilshell.application
      .check()
      .catch(() => setUpdateFailure(true))
      .finally(() => setPending(false));
  }, []);
  useEffect(() => {
    if (checkRequest) {
      onCheckRequestHandled?.();
      check();
    }
  }, [checkRequest, onCheckRequestHandled, check]);
  const run = (action: () => Promise<unknown>) => {
    setFailure(false);
    void action().catch(() => setFailure(true));
  };
  const checking = pending || state.phase === 'checking';
  return (
    <div className="flex flex-col gap-7">
      <section
        className="flex flex-col items-center gap-5 py-3 text-center"
        aria-label={t('settings_about')}
      >
        <img src={applicationIcon} alt="" className="size-24" />
        <div className="flex flex-col items-center gap-3">
          <h2 className="text-2xl font-semibold tracking-tight">{info?.name ?? 'VigilShell'}</h2>
          <div className="flex flex-wrap items-center justify-center gap-2">
            {info ? <Badge>v{info.version}</Badge> : <Skeleton className="h-5 w-28" />}
            <Badge variant="secondary">Apache-2.0</Badge>
          </div>
          <p className="max-w-md text-sm leading-relaxed text-muted-foreground">
            {t('about_description')}
          </p>
        </div>
        <Button
          size="lg"
          variant="secondary"
          onClick={() => {
            if ((state.phase === 'available' && state.release) || checking) {
              setUpdateFailure(false);
              setDialogOpen(true);
            } else check();
          }}
        >
          {checking ? (
            <Spinner data-icon="inline-start" aria-hidden="true" />
          ) : (
            <RefreshCwIcon data-icon="inline-start" />
          )}
          {state.phase === 'available' && state.release && !checking
            ? t('updates_view')
            : t('updates_check')}
        </Button>
      </section>
      {failure ? (
        <Alert>
          <AlertDescription>{t('application_failed')}</AlertDescription>
        </Alert>
      ) : null}
      {automaticUpdateChecksEnabled ? (
        <section className="flex flex-col gap-3" aria-label={t('updates_title')}>
          <Card appearance="settings">
            <CardContent className="p-0">
              <FieldGroup className="gap-0">
                <Row
                  id="auto-check-updates"
                  label={t('updates_auto')}
                  description={t('updates_auto_description')}
                  disabled={preferences.disabled}
                >
                  <Switch
                    id="auto-check-updates"
                    aria-describedby="auto-check-updates-description"
                    checked={preferences.preferences.autoCheckUpdates}
                    disabled={preferences.disabled}
                    onCheckedChange={(value) => void preferences.save({ autoCheckUpdates: value })}
                  />
                </Row>
              </FieldGroup>
            </CardContent>
          </Card>
          {preferences.error ? (
            <Alert>
              <AlertDescription>{t('application_failed')}</AlertDescription>
            </Alert>
          ) : null}
          {state.phase !== 'idle' ? (
            <p className="text-xs text-muted-foreground" role="status">
              {t(`updates_${state.phase}`)}
            </p>
          ) : null}
        </section>
      ) : null}
      <section className="flex flex-col gap-3" aria-labelledby="about-more">
        <h3 id="about-more" className="text-sm font-medium text-muted-foreground">
          {t('about_more')}
        </h3>
        <Card appearance="settings">
          <CardContent className="p-0">
            <Button
              variant="ghost"
              size="list"
              className="w-full gap-3 px-4 py-3"
              onClick={() => run(() => window.vigilshell.application.open('project'))}
            >
              <CodeXmlIcon data-icon="inline-start" />
              <span className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="text-sm font-medium">{t('about_project')}</span>
                <span className="break-all text-xs text-muted-foreground">
                  github.com/sparkwild/vigilshell
                </span>
              </span>
              <ExternalLinkIcon data-icon="inline-end" />
            </Button>
            <Separator />
            <Button
              variant="ghost"
              size="list"
              className="w-full gap-3 px-4 py-3"
              onClick={() => run(() => window.vigilshell.application.open('issues'))}
            >
              <MessageSquareIcon data-icon="inline-start" />
              <span className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="text-sm font-medium">{t('about_issues')}</span>
                <span className="break-all text-xs text-muted-foreground">GitHub Issues</span>
              </span>
              <ExternalLinkIcon data-icon="inline-end" />
            </Button>
          </CardContent>
        </Card>
      </section>
      <p className="pb-2 text-center text-xs text-muted-foreground">© 2026 sparkwild</p>
      <UpdateDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        state={state}
        info={info}
        pending={pending}
        failure={updateFailure}
        onDownload={() => {
          setUpdateFailure(false);
          void window.vigilshell.application
            .open('openRelease')
            .catch(() => setUpdateFailure(true));
        }}
      />
    </div>
  );
}
