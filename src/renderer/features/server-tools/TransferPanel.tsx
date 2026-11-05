import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { useFeedbackAnchor } from '../connections/feedback/use-operation-anchor';
import { transferFeedbackKey } from './transfer-feedback';
import { Progress } from '../../components/ui/progress';
import { useAutoAnimate } from '@formkit/auto-animate/react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  CardDescription,
} from '../../components/ui/card';
import { HoverCard, HoverCardTrigger, HoverCardContent } from '../../components/ui/hover-card';
import { useEffect, useState } from 'react';
import type { TransferView } from '@contracts';
import { ArrowUpIcon, ArrowDownIcon, ArrowDownUpIcon, XIcon } from 'lucide-react';
import { useWorkspace, errorCode } from '../connections/workspace-context';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { useDesktop } from '../../app/DesktopProvider';
import { IconButton } from '../../components/ui/icon-button';
import { Button } from '../../components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../../components/ui/dialog';
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '../../components/ui/empty';
import { fileSize, transferActive } from './file-format';
export function TransferList({
  connectionId,
  groupId,
  target,
  previewIds,
}: {
  previewIds?: ReadonlySet<string>;
  connectionId?: string;
  groupId?: string;
  target?: import('@contracts').TransferNotificationTarget;
}) {
  const { store, state } = useWorkspace(),
    { t } = useDesktop();
  const [, tick] = useState(0);
  const [animationParent] = useAutoAnimate<HTMLDivElement>();
  const tasks = [...state.transfers.values()].filter(
    (task) =>
      (previewIds
        ? previewIds.has(task.id)
        : connectionId
          ? task.connectionId === connectionId
          : groupId
            ? task.groupId === groupId
            : false) &&
      (!target || (target.runtimeId === state.runtimeId && task.batchId === target.batchId)),
  );
  const focusId = target ? tasks.find((task) => task.phase === target.phase)?.id : undefined;
  useEffect(() => {
    if (focusId) document.getElementById(`transfer-task-${focusId}`)?.focus();
  }, [focusId]);
  const active = tasks.some((task) => transferActive(task.phase));
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, [active]);
  useEffect(() => {
    if (state.transportReady) void store.refreshTransfers().catch(() => {});
  }, [state.transportReady, store]);
  function speed(task: TransferView) {
    if (task.phase !== 'running') return null;
    const points =
      store.transferSamples
        .get(task.id)
        ?.points.filter((p) => performance.now() - p.time <= 5000) ?? [];
    if (points.length < 2) return null;
    const first = points[0]!,
      last = points.at(-1)!,
      seconds = (last.time - first.time) / 1000;
    return seconds > 0 ? (last.bytes - first.bytes) / seconds : null;
  }
  if (!tasks.length)
    return (
      <Empty>
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <ArrowDownUpIcon />
          </EmptyMedia>
          <EmptyTitle>
            {t(target ? 'transfer_notification_unavailable' : 'files_no_transfers')}
          </EmptyTitle>
        </EmptyHeader>
      </Empty>
    );
  const priority = (task: TransferView) => (transferActive(task.phase) ? 0 : 1);
  tasks.sort(
    (a, b) => priority(a) - priority(b) || b.createdAt - a.createdAt || a.id.localeCompare(b.id),
  );
  return (
    <section className="transfer-panel" aria-label={t('tools_transfers')}>
      <div ref={animationParent} className="flex min-w-0 flex-col gap-2">
        {tasks.map((task) => (
          <TransferRow
            key={task.id}
            task={task}
            speed={speed(task)}
            connectionLabel={previewIds ? store.connectionLabel(task.connectionId) : undefined}
          />
        ))}
      </div>
    </section>
  );
}

function TransferRow({
  task,
  speed,
  connectionLabel,
}: {
  task: TransferView;
  speed: number | null;
  connectionLabel?: string;
}) {
  const notificationRef = useNotificationSurface(`transfer:${task.batchId}`);
  const { store } = useWorkspace(),
    { t } = useDesktop(),
    [busy, setBusy] = useState(false);
  const feedback = useFeedbackAnchor<HTMLDivElement>(transferFeedbackKey(task), store.feedback);
  async function act(action: () => Promise<unknown>) {
    setBusy(true);
    try {
      await action();
      await store.refreshTransfers();
    } catch (e) {
      store.report(errorCode(e));
    } finally {
      setBusy(false);
    }
  }
  const phase = t(`tools_transfer_${task.phase}` as Parameters<typeof t>[0]);
  const progress =
    task.total === null
      ? undefined
      : task.total === 0
        ? task.phase === 'complete'
          ? 100
          : 0
        : Math.min(100, (task.bytes / task.total) * 100);
  const stage = task.details?.stage;
  const transferring = task.phase === 'running' && (!stage || stage === 'transferring');
  const status =
    task.phase === 'running' && stage && stage !== 'transferring' && stage !== 'finished'
      ? t(`transfer_stage_${stage}`)
      : transferring && progress !== undefined && progress < 100
        ? `${Math.floor(progress)}%`
        : phase;
  const summary = task.details;
  const itemCount = summary
    ? summary.completed + summary.failed + summary.skipped + summary.pending
    : 0;
  const eta =
    transferring && task.total !== null && speed !== null && speed > 0
      ? Math.ceil(Math.max(0, task.total - task.bytes) / speed)
      : null;
  const remaining =
    eta === null || !Number.isFinite(eta)
      ? '—'
      : eta >= 3600
        ? t('transfer_remaining_hours', {
            hours: Math.floor(eta / 3600),
            minutes: Math.floor((eta % 3600) / 60),
          })
        : eta >= 60
          ? t('transfer_remaining_minutes', { minutes: Math.floor(eta / 60), seconds: eta % 60 })
          : t('transfer_remaining_seconds', { count: eta });
  return (
    <HoverCard>
      <HoverCardTrigger
        render={<div ref={feedback} className="min-w-0" />}
        id={`${connectionLabel ? 'transfer-preview' : 'transfer-task'}-${task.id}`}
        tabIndex={0}
        aria-label={`${task.name} · ${phase}`}
      >
        <Card ref={notificationRef} size="sm" className="min-w-0 gap-0.5 py-1.5">
          <CardHeader className="min-w-0 px-2">
            <CardTitle className="flex h-6 min-w-0 items-center gap-1.5">
              {task.direction === 'upload' ? (
                <ArrowUpIcon className="size-4 shrink-0" />
              ) : (
                <ArrowDownIcon className="size-4 shrink-0" />
              )}
              <span className="min-w-0 flex-1 truncate">{task.name}</span>
              {transferActive(task.phase) && task.phase !== 'awaitingDecision' ? (
                <IconButton
                  label={t('ws_cancel')}
                  size="icon-xs"
                  disabled={busy || task.phase === 'cancelling'}
                  onClick={() => void act(() => store.call('cancelTransfer', { id: task.id }))}
                >
                  <XIcon />
                </IconButton>
              ) : null}
            </CardTitle>
            {connectionLabel ? (
              <CardDescription className="truncate">{connectionLabel}</CardDescription>
            ) : null}
          </CardHeader>
          <CardContent className="@container flex min-w-0 flex-col gap-1 px-2">
            {transferActive(task.phase) ? (
              <Progress
                appearance="transfer"
                aria-label={task.name}
                max={100}
                value={stage === 'enumerating' ? null : (progress ?? null)}
              >
                {transferring && progress !== undefined ? (
                  <span className="hidden shrink-0 @min-[20rem]:inline">
                    {Math.floor(progress)}%
                  </span>
                ) : null}
                <span className="min-w-0 flex-1 truncate">
                  {transferring && progress !== undefined
                    ? speed !== null
                      ? `${fileSize(speed)}/s`
                      : '—'
                    : status}
                </span>
                <span className="min-w-0 flex-[2] truncate text-right">
                  {fileSize(task.bytes)} / {fileSize(task.total)}
                </span>
              </Progress>
            ) : (
              <div className="flex h-6 min-w-0 items-center justify-between gap-1.5 px-1.5 text-xs text-muted-foreground tabular-nums">
                <span className="min-w-0 flex-1 truncate">
                  {task.phase === 'complete'
                    ? fileSize(task.bytes)
                    : `${fileSize(task.bytes)} / ${fileSize(task.total)}`}
                </span>
                <span className="shrink-0">{phase}</span>
              </div>
            )}
            {summary && itemCount > 1 ? (
              <div className="transfer-metrics truncate">
                {t('transfer_items_completed', { completed: summary.completed, total: itemCount })}
                {summary.failed > 0
                  ? ` · ${t('transfer_items_failed', { count: summary.failed })}`
                  : ''}
                {summary.skipped > 0
                  ? ` · ${t('transfer_items_skipped', { count: summary.skipped })}`
                  : ''}
              </div>
            ) : summary && summary.skipped > 0 ? (
              <div className="transfer-metrics truncate">
                {t('transfer_items_skipped', { count: summary.skipped })}
              </div>
            ) : null}
            {task.error ? (
              <p className="text-sm text-destructive">{t(userErrorKey(task.error))}</p>
            ) : null}
            {['awaitingDecision', 'failed', 'cancelled'].includes(task.phase) ? (
              <div className="flex flex-wrap items-center justify-end gap-2">
                {task.phase === 'awaitingDecision' ? (
                  <Button
                    size="xs"
                    variant="outline"
                    onClick={() => store.showTransferConflicts(task.id)}
                  >
                    {t('transfer_conflict_handle')}
                  </Button>
                ) : ['failed', 'cancelled'].includes(task.phase) ? (
                  <>
                    <Button
                      size="xs"
                      variant="outline"
                      disabled={busy || !task.canRetry}
                      onClick={() => void act(() => store.call('retryTransfer', { id: task.id }))}
                    >
                      {t('ws_retry')}
                    </Button>
                    {!task.canRetry ? <span>{t('files_retry_reconnect')}</span> : null}
                  </>
                ) : null}
              </div>
            ) : null}
          </CardContent>
        </Card>
      </HoverCardTrigger>
      <HoverCardContent
        side="left"
        align="start"
        className="w-96 max-w-(--available-width) max-h-(--available-height) overflow-y-auto"
        role="region"
        aria-label={t('files_details')}
      >
        <TransferDetails task={task} remaining={remaining} />
      </HoverCardContent>
    </HoverCard>
  );
}
function TransferDetails({ task, remaining }: { task: TransferView; remaining: string }) {
  const { t } = useDesktop();
  const feedback = useFeedbackAnchor<HTMLDivElement>(
    transferFeedbackKey(task),
    useWorkspace().store.feedback,
  );
  return (
    <div ref={feedback} className="flex min-w-0 flex-col gap-4">
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
        {[
          ['transfer_item_name', task.name],
          ['connection_target', task.remoteIdentity],
          [
            task.direction === 'upload' ? 'transfer_remote_destination' : 'transfer_remote_source',
            task.remotePath,
          ],
          ['connection_state', t(`tools_transfer_${task.phase}`)],
          ...(transferActive(task.phase) ? [['transfer_remaining_time', remaining]] : []),
        ].map(([key, value]) => (
          <div className="contents" key={key}>
            <dt className="text-muted-foreground">{t(key as Parameters<typeof t>[0])}</dt>
            <dd className="min-w-0 select-text break-all">{value}</dd>
          </div>
        ))}
      </dl>
      {task.error ? (
        <p role="alert" className="text-destructive">
          {t(userErrorKey(task.error))}
        </p>
      ) : null}
    </div>
  );
}
export function TransferWindows() {
  const { store, state } = useWorkspace(),
    { t } = useDesktop();
  useEffect(() => {
    const id = state.transferWindows.at(-1);
    if (id !== undefined && !store.transferTargets?.has(id))
      document.querySelector<HTMLElement>(`[data-transfer-window="${id}"]`)?.focus();
  }, [state.transferWindows, store]);
  return (
    <>
      {state.transferWindows.map((id, index) => (
        <Dialog
          key={id}
          open
          mode="workspace"
          onOpenChange={(open) => {
            if (!open) store.closeTransfers(id);
          }}
        >
          <DialogContent
            {...(store.transferTargets?.has(id) ? { initialFocus: false } : {})}
            data-transfer-window={id}
            tabIndex={-1}
            closeLabel={t('ws_close')}
            size="transfer"
            style={{ transform: `translate(${index * 16}px, ${index * 16}px)` }}
          >
            <DialogHeader>
              <DialogTitle>
                {id ? `${state.connections.find((c) => c.id === id)?.name ?? id} · ` : ''}
                {t('tools_transfers')}
              </DialogTitle>
            </DialogHeader>
            {store.transferTargets?.has(id) ? (
              <Button size="xs" variant="outline" onClick={() => store.showTransfers(id)}>
                {t('transfer_notification_all')}
              </Button>
            ) : null}
            <TransferList connectionId={id} target={store.transferTargets?.get(id)} />
          </DialogContent>
        </Dialog>
      ))}
    </>
  );
}
