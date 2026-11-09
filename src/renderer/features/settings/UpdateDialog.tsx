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

export function UpdateDialog({
  open,
  onOpenChange,
  state,
  info,
  pending,
  failure,
  onDownload,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  state: UpdateSnapshot;
  info?: ApplicationInfo;
  pending: boolean;
  failure: boolean;
  onDownload(): void;
}) {
  const { t } = useDesktop();
  const checking = pending || (!failure && state.phase === 'checking');
  const failed =
    !checking &&
    (failure || ['empty', 'networkError', 'rateLimited', 'invalidResponse'].includes(state.phase));
  const available = !checking && !failed && state.release;
  const Icon = failed
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
      : t(`updates_${state.phase}`);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent closeLabel={t('ws_close')} className="flex flex-col overflow-hidden">
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
              {checking ? <Spinner className="size-5" /> : <Icon className="size-5" />}
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
          {available ? (
            <>
              <section className="flex min-w-0 flex-col gap-2" aria-label={t('updates_notes')}>
                <h3 className="text-sm font-medium">{t('updates_notes')}</h3>
                <div className="whitespace-pre-wrap rounded-lg bg-muted p-3 text-sm text-muted-foreground [overflow-wrap:anywhere]">
                  {available.notes.trim() || t('updates_no_notes')}
                </div>
              </section>
              <p className="text-xs text-muted-foreground">{t('updates_manual')}</p>
            </>
          ) : null}
        </DialogBody>
        <div
          className={cn(
            'flex shrink-0 gap-2 pt-2',
            available ? 'flex-col-reverse sm:flex-row sm:justify-end' : 'justify-center',
          )}
        >
          {failed ? (
            <DialogClose render={<Button />}>{t('updates_dismiss')}</DialogClose>
          ) : (
            <>
              {available ? (
                <Button onClick={onDownload}>
                  <CircleArrowUpIcon data-icon="inline-start" />
                  {t('updates_download')}
                </Button>
              ) : null}
              <DialogClose render={<Button variant="outline" />}>{t('ws_close')}</DialogClose>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
