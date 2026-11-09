import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { useEffect, useState } from 'react';
import type { WorkspaceReply } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorCode, errorKey } from '../connections/workspace-context';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../components/ui/dialog';
import { Button } from '../../components/ui/button';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { Empty, EmptyHeader, EmptyTitle } from '../../components/ui/empty';
import { Progress } from '../../components/ui/progress';
import { Spinner } from '../../components/ui/spinner';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectGroup,
  SelectItem,
} from '../../components/ui/select';
import { TruncatedText } from '../../components/ui/tooltip';

type Page = Extract<WorkspaceReply, { kind: 'fileOperation' }>['page'];
export function FileOperationsDialog({
  sessionId,
  initialId,
  close,
  onUnavailable,
}: {
  sessionId: string;
  initialId: string | null;
  close(): void;
  onUnavailable?(): void;
}) {
  const { store, state } = useWorkspace(),
    { t } = useDesktop();
  const operations = [...state.operations.values()].filter(
    (op) => op.kind === 'files' && op.sessionId === sessionId,
  );
  const [selected, setSelected] = useState(initialId),
    [offsets, setOffsets] = useState([0]),
    [refresh, setRefresh] = useState(0);
  const id = selected ?? operations.at(-1)?.id;
  const [page, setPage] = useState<Page | null>(null),
    [error, setError] = useState<string | null>(null),
    [busy, setBusy] = useState(false);
  const offset = offsets.at(-1)!;
  const runtime = state.runtimeId;
  const notificationRef = useNotificationSurface(id && page ? `fileOperation:${id}` : null);
  useEffect(() => {
    if (!id) return;
    let alive = true,
      timer: ReturnType<typeof setTimeout> | undefined;
    setPage(null);
    setError(null);
    const read = async () => {
      try {
        const reply = await store.call('getFileOperation', { id, offset });
        if (!alive || store.getSnapshot().runtimeId !== runtime) return;
        if (reply.kind !== 'fileOperation') throw new Error('invalid_response');
        setPage(reply.page);
        setError(null);
        if (reply.page.phase === 'working') timer = setTimeout(() => void read(), 500);
      } catch (cause) {
        if (alive) {
          if (onUnavailable && ['not_found', 'runtime_unavailable'].includes(errorCode(cause)))
            onUnavailable();
          else setError(errorCode(cause));
        }
      }
    };
    void read();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [store, id, offset, runtime, refresh, onUnavailable]);
  async function act(retry: boolean) {
    if (!id || busy || store.getSnapshot().runtimeId !== runtime) return;
    setBusy(true);
    setError(null);
    try {
      if (retry) await store.mutation('retryFileOperation', { id });
      else await store.call('cancelOperation', { id });
      setRefresh((n) => n + 1);
    } catch (cause) {
      setError(errorCode(cause));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent
        size="wide"
        closeLabel={t('ws_close')}
        className="flex max-h-[calc(100dvh-4rem)] flex-col overflow-hidden"
      >
        <DialogHeader>
          <DialogTitle>{t('files_operations')}</DialogTitle>
        </DialogHeader>
        {operations.length > 1 ? (
          <Select
            value={id}
            onValueChange={(value) => {
              if (value) {
                setSelected(value);
                setOffsets([0]);
              }
            }}
          >
            <SelectTrigger aria-label={t('files_operations')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {operations.map((op, index) => (
                  <SelectItem key={op.id} value={op.id}>
                    {t('files_operations')} {index + 1} · {op.id.slice(0, 8)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        ) : null}
        {error ? (
          <Alert variant="destructive">
            <AlertDescription>
              {t(errorKey(error))}
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => setRefresh((n) => n + 1)}
              >
                {t('retry')}
              </Button>
            </AlertDescription>
          </Alert>
        ) : null}
        {!id ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t('files_no_operations')}</EmptyTitle>
            </EmptyHeader>
          </Empty>
        ) : !page && !error ? (
          <Spinner />
        ) : null}
        {page ? (
          <>
            <div ref={notificationRef} role="status" className="flex flex-col gap-2">
              <span>
                {t(
                  page.stage === 'scanning'
                    ? 'files_operation_scanning'
                    : page.stage === 'applying'
                      ? 'files_operation_applying'
                      : 'files_operation_finished',
                )}
              </span>
              <span>
                {t('files_operation_items', {
                  completed: page.completed,
                  failed: page.failed,
                  skipped: page.skipped,
                  pending: page.pending,
                })}
              </span>
              <Progress
                value={
                  page.stage === 'scanning'
                    ? null
                    : ((page.completed + page.failed + page.skipped) /
                        Math.max(1, page.completed + page.failed + page.skipped + page.pending)) *
                      100
                }
              />
            </div>
            {page.error ? (
              <Alert variant="destructive">
                <AlertDescription>{t(errorKey(page.error))}</AlertDescription>
              </Alert>
            ) : null}
            <div className="flex min-h-0 flex-col gap-2 overflow-auto overscroll-contain">
              {page.items.map((item) => (
                <div key={item.entry.path} className="flex flex-wrap items-center gap-2">
                  <TruncatedText className="min-w-0 flex-1">{item.entry.path}</TruncatedText>
                  <span>{t(`files_operation_status_${item.status}`)}</span>
                  {item.error ? <span>{t(errorKey(item.error))}</span> : null}
                </div>
              ))}
            </div>
            <div className="flex gap-2">
              {offsets.length > 1 ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setOffsets((values) => values.slice(0, -1))}
                >
                  {t('back')}
                </Button>
              ) : null}
              {page.next != null ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setOffsets((values) => [...values, page.next!])}
                >
                  {t('files_operation_more')}
                </Button>
              ) : null}
            </div>
          </>
        ) : null}
        <DialogFooter>
          {page?.phase === 'working' ? (
            <Button variant="outline" disabled={busy} onClick={() => void act(false)}>
              {t('ws_cancel')}
            </Button>
          ) : page && (page.failed > 0 || page.pending > 0) ? (
            <Button variant="outline" disabled={busy} onClick={() => void act(true)}>
              {t('files_operation_retry')}
            </Button>
          ) : null}
          <Button onClick={close}>{t('ws_close')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
