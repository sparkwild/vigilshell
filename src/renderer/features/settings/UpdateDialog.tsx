import type { ApplicationInfo, UpdateSnapshot } from '@contracts';
import { cn } from 'cn';
import {
  CheckCircle2Icon,
  CircleArrowUpIcon,
  CircleHelpIcon,
  ExternalLinkIcon,
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
import { releaseNoteSections, selectReleaseNotes } from '../../../shared/release-notes';

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
  returnFocus,
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
  returnFocus?(): HTMLElement | false;
}) {
  const { t, snapshot } = useDesktop();
  const notes = state.release
    ? selectReleaseNotes(state.release, snapshot.appearance.locale)
    : null;
  const sectionLabels = {
    features: 'updates_features',
    fixes: 'updates_fixes',
    upgradeNotes: 'updates_upgrade_notes',
  } as const;
  const checking = pending || (!failure && state.phase === 'checking');
  const downloading = state.phase === 'downloading';
  const installing = state.phase === 'preparing' || state.phase === 'installing';
  const ready = state.phase === 'ready' || state.phase === 'installError';
  const failed =
    !checking &&
    (failure || ['empty', 'networkError', 'rateLimited', 'invalidResponse'].includes(state.phase));
  const available = !checking && !failed && state.release;
  const discovered = !!available && state.phase === 'available';
  const Icon =
    failed || state.phase === 'downloadError' || state.phase === 'installError'
      ? TriangleAlertIcon
      : available
        ? CircleArrowUpIcon
        : state.phase === 'current'
          ? CheckCircle2Icon
          : CircleHelpIcon;
  const title = checking
    ? t('updates_checking')
    : failed
      ? t('updates_failed')
      : state.phase === 'available'
        ? t('updates_discovered')
        : state.phase === 'installError'
          ? t('updates_install_failed_title')
          : state.phase === 'idle'
            ? t('updates_check')
            : t(`updates_${state.phase}`);
  const issue =
    !checking && !failed && ['installError', 'downloadError'].includes(state.phase) && state.issue
      ? t(`updates_${state.issue}`)
      : !checking && !failed && state.phase === 'installError'
        ? t('updates_installError')
        : null;
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!installing) onOpenChange(value);
      }}
    >
      <DialogContent
        finalFocus={returnFocus}
        closeLabel={t('ws_close')}
        showCloseButton={!installing}
        className="flex flex-col gap-3 overflow-hidden"
      >
        <DialogHeader className="min-w-0 shrink-0 pr-8">
          <DialogTitle className="flex min-w-0 items-center gap-2">
            {checking || downloading || installing ? (
              <Spinner className="size-4 shrink-0" aria-hidden="true" />
            ) : (
              <Icon
                className={cn('size-4 shrink-0', discovered && 'text-update-accent')}
                aria-hidden="true"
              />
            )}
            <span
              className={cn('min-w-0 [overflow-wrap:anywhere]', discovered && 'text-update-accent')}
              role="status"
              aria-live="polite"
            >
              {title}
            </span>
          </DialogTitle>
          <DialogDescription className="[overflow-wrap:anywhere]">
            {available ? (
              t('updates_versions', {
                current: info ? `v${info.version}` : '—',
                next: `v${available.version}`,
              })
            ) : (
              <>
                {t('updates_installed')} {info ? `v${info.version}` : '—'}
              </>
            )}
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="flex flex-col gap-3 overflow-hidden">
          {issue ? <p className="shrink-0 text-sm text-muted-foreground">{issue}</p> : null}
          {state.phase === 'idle' && !checking && !failed ? (
            <p className="text-sm text-muted-foreground">{t('updates_idle')}</p>
          ) : null}
          {downloading ? (
            <div className="flex shrink-0 flex-col gap-2">
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
              <section className="flex min-h-0 min-w-0 flex-col gap-2">
                <h3 className="shrink-0 text-sm font-medium">{t('updates_notes')}</h3>
                {notes && releaseNoteSections.some((key) => notes.sections[key]?.length) ? (
                  <div
                    className="flex min-h-0 max-h-80 flex-col gap-3 overflow-y-auto overscroll-contain rounded-lg bg-muted p-3 text-sm text-muted-foreground [overflow-wrap:anywhere]"
                    role="region"
                    aria-label={t('updates_notes')}
                    tabIndex={0}
                    data-testid="update-release-notes"
                  >
                    {releaseNoteSections.map((key) =>
                      notes.sections[key]?.length ? (
                        <section
                          key={key}
                          className="flex shrink-0 flex-col gap-2"
                          aria-label={t(sectionLabels[key])}
                        >
                          <h4 className="font-medium text-foreground">{t(sectionLabels[key])}</h4>
                          <ul className="flex list-disc flex-col gap-2 pl-4">
                            {notes.sections[key]!.map((text, index) => (
                              <li key={index} className="whitespace-pre-wrap">
                                {text}
                              </li>
                            ))}
                          </ul>
                        </section>
                      ) : null,
                    )}
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">{t('updates_no_notes')}</p>
                )}
              </section>
              {!state.canDownload ? (
                <p className="shrink-0 text-xs text-muted-foreground">{t('updates_manual')}</p>
              ) : null}
            </>
          ) : null}
        </DialogBody>
        {!installing ? (
          <div
            data-testid="update-actions"
            className={cn(
              'flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2',
              available ? 'justify-between' : 'justify-center',
            )}
          >
            {available ? (
              <Button variant="link" className="px-0" onClick={onRelease}>
                {t('updates_release_page')}
                <ExternalLinkIcon data-icon="inline-end" aria-hidden="true" />
              </Button>
            ) : null}
            {failed ? (
              <DialogClose render={<Button />}>{t('updates_dismiss')}</DialogClose>
            ) : (
              <div className={cn('flex flex-wrap justify-end gap-2', available && 'ml-auto')}>
                <DialogClose render={<Button variant="outline" />}>
                  {t(ready || downloading ? 'updates_later' : 'ws_close')}
                </DialogClose>
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
              </div>
            )}
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
