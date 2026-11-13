import type { ApplicationInfo, UpdateSnapshot } from '@contracts';
import { cn } from 'cn';
import {
  CheckCircle2Icon,
  CircleArrowUpIcon,
  CircleHelpIcon,
  TriangleAlertIcon,
} from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogClose,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '../../components/ui/dialog';
import { Spinner } from '../../components/ui/spinner';
import { Progress } from '../../components/ui/progress';

export function UpdateDialog({
  open,
  onOpenChange,
  state,
  info,
  pending,
  failure,
  onDownload,
  onCancel,
  onInstall,
  onRelease,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  state: UpdateSnapshot;
  info?: ApplicationInfo;
  pending: boolean;
  failure: boolean;
  onDownload(): void;
  onCancel(): void;
  onInstall(): void;
  onRelease(): void;
}) {
  const { t } = useDesktop();
  const checking = pending || (!failure && state.phase === 'checking');
  const downloading = state.phase === 'downloading';
  const installing = state.phase === 'preparing' || state.phase === 'installing';
  const ready = state.phase === 'ready' || state.phase === 'installError';
  const failed =
    !checking &&
    (failure || ['empty', 'networkError', 'rateLimited', 'invalidResponse'].includes(state.phase));
  const available = !checking && !failed && state.release;
  const Icon =
    failed || state.phase === 'downloadError' || state.phase === 'installError'
      ? TriangleAlertIcon
      : available
        ? CircleArrowUpIcon
        : state.phase === 'current'
          ? CheckCircle2Icon
          : CircleHelpIcon;
  const status = checking
    ? t('updates_checking')
    : failed
      ? t('updates_failed')
      : ['installError', 'downloadError'].includes(state.phase) && state.issue
        ? t(`updates_${state.issue}`)
        : t(`updates_${state.phase}`);
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!installing) onOpenChange(value);
      }}
    >
      <DialogContent
        closeLabel={t('ws_close')}
        showCloseButton={!installing}
        className="flex flex-col overflow-hidden"
      >
        <DialogHeader className="min-w-0 shrink-0 pr-8">
          <DialogTitle>{t('updates_check')}</DialogTitle>
          <DialogDescription className="[overflow-wrap:anywhere]">
            {t('updates_installed')} {info ? `v${info.version}` : '—'}
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-4" tabIndex={0}>
          <div className="flex items-center gap-3 py-3" role="status" aria-live="polite">
            <span
              className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-muted text-muted-foreground"
              aria-hidden="true"
            >
              {checking || downloading || installing ? (
                <Spinner className="size-5" />
              ) : (
                <Icon className="size-5" />
              )}
            </span>
            <div className="flex min-w-0 flex-col gap-1">
              <p className="font-medium">{status}</p>
              {available ? (
                <p className="text-sm text-muted-foreground [overflow-wrap:anywhere]">
                  v{available.version}
                </p>
              ) : null}
            </div>
          </div>
          {downloading ? (
            <div className="flex flex-col gap-2">
              <Progress
                value={state.progress?.percent ?? null}
                aria-label={t('updates_downloading')}
              />
              <p className="text-xs text-muted-foreground">
                {state.progress ? `${Math.floor(state.progress.percent)}% · ` : ''}
                {t('updates_background')}
              </p>
            </div>
          ) : null}
          {available ? (
            <>
              <section className="flex min-w-0 flex-col gap-2" aria-label={t('updates_notes')}>
                <h3 className="text-sm font-medium">{t('updates_notes')}</h3>
                <div className="whitespace-pre-wrap rounded-lg bg-muted p-3 text-sm text-muted-foreground [overflow-wrap:anywhere]">
                  {available.notes.trim() || t('updates_no_notes')}
                </div>
              </section>
              {!state.canDownload ? (
                <p className="text-xs text-muted-foreground">{t('updates_manual')}</p>
              ) : null}
              {!installing ? (
                <Button variant="link" className="self-start px-0" onClick={onRelease}>
                  {t('updates_release_page')}
                </Button>
              ) : null}
            </>
          ) : null}
        </DialogBody>
        <div
          className={cn(
            'flex shrink-0 gap-2 pt-2',
            available ? 'flex-col-reverse sm:flex-row sm:justify-end' : 'justify-center',
          )}
        >
          {installing ? null : failed ? (
            <DialogClose render={<Button />}>{t('updates_dismiss')}</DialogClose>
          ) : (
            <>
              {available && !downloading && !ready ? (
                <Button onClick={onDownload}>
                  <CircleArrowUpIcon data-icon="inline-start" />
                  {t(
                    state.canDownload
                      ? state.phase === 'downloadError'
                        ? 'updates_retry_download'
                        : 'updates_download_now'
                      : 'updates_download',
                  )}
                </Button>
              ) : null}
              {downloading ? (
                <Button onClick={onCancel} disabled={state.progress?.percent === 100}>
                  {t('ws_cancel')}
                </Button>
              ) : null}
              {ready ? <Button onClick={onInstall}>{t('updates_install')}</Button> : null}
              <DialogClose render={<Button variant="outline" />}>
                {t(ready || downloading ? 'updates_later' : 'ws_close')}
              </DialogClose>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
