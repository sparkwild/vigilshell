import { useEffect, useId, useState } from 'react';
import type { TransferView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorCode } from '../connections/workspace-context';
import type { TransferConflictTarget } from '../connections/workspace-store';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { useFeedbackAnchor } from '../connections/feedback/use-operation-anchor';
import { transferFeedbackKey } from './transfer-feedback';
import { transferActive } from './file-format';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../../components/ui/dialog';
import { Button } from '../../components/ui/button';
import { Spinner } from '../../components/ui/spinner';
import { Field, FieldLabel } from '../../components/ui/field';
import { Checkbox } from '../../components/ui/checkbox';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { TruncatedText } from '../../components/ui/tooltip';

export function TransferConflictDialog() {
  const { state, store } = useWorkspace();
  const { t } = useDesktop();
  const target = state.transferConflictTarget;
  if (!target) return null;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) store.closeTransferConflicts();
      }}
    >
      <DialogContent closeLabel={t('ws_close')} size="wide">
        <DialogHeader>
          <DialogTitle>{t('transfer_conflict_title')}</DialogTitle>
        </DialogHeader>
        <BatchConflicts
          key={`${target.runtimeId}:${target.connectionId}:${target.batchId}`}
          target={target}
        />
      </DialogContent>
    </Dialog>
  );
}

function BatchConflicts({ target }: { target: TransferConflictTarget }) {
  const { state, store } = useWorkspace();
  const { t } = useDesktop();
  const [selected, setSelected] = useState(target.taskId);
  const available = state.runtimeId === target.runtimeId && state.transportReady;
  const tasks = [...state.transfers.values()].filter(
    (task) => task.connectionId === target.connectionId && task.batchId === target.batchId,
  );
  const waiting = tasks
    .filter((task) => task.phase === 'awaitingDecision')
    .sort((a, b) => b.createdAt - a.createdAt || a.id.localeCompare(b.id));
  const current = waiting.find((task) => task.id === selected) ?? waiting[0];
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (available && tasks.length > 0 && waiting.length === 0 && !busy && !error)
      store.closeTransferConflicts();
  }, [available, tasks.length, waiting.length, busy, error, store]);
  return (
    <div className="flex min-h-0 flex-col gap-4 overflow-y-auto">
      {!available || !tasks.length ? (
        <p>{t('transfer_notification_unavailable')}</p>
      ) : (
        <>
          {waiting.length > 0 ? (
            <p>{t('transfer_conflict_count', { count: waiting.length })}</p>
          ) : null}
          {waiting.length > 1 ? (
            <div
              className="flex max-h-40 flex-col gap-1 overflow-y-auto"
              aria-label={t('transfer_conflict_tasks')}
            >
              {waiting.map((task) => (
                <Button
                  key={task.id}
                  variant={task.id === current?.id ? 'secondary' : 'ghost'}
                  disabled={busy}
                  aria-pressed={task.id === current?.id}
                  className="min-w-0 justify-start"
                  onClick={() => setSelected(task.id)}
                >
                  <TruncatedText className="min-w-0 truncate">{task.name}</TruncatedText>
                </Button>
              ))}
            </div>
          ) : null}
          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}
          {!current && busy ? <Spinner aria-label={t('ws_working')} /> : null}
          {current ? (
            <ConflictDecision
              key={`${current.id}:${current.conflictVersion}`}
              target={target}
              task={current}
              busy={busy}
              onBusy={setBusy}
              onError={setError}
            />
          ) : null}
        </>
      )}
    </div>
  );
}

function ConflictDecision({
  target,
  task,
  busy,
  onBusy: setBusy,
  onError: setError,
}: {
  target: TransferConflictTarget;
  task: TransferView;
  busy: boolean;
  onBusy(busy: boolean): void;
  onError(error: string | null): void;
}) {
  const { store } = useWorkspace();
  const { t } = useDesktop();
  const [batch, setBatch] = useState(false);
  const batchCheckboxId = useId();
  const summary = task.details;
  const canApplyBatch =
    task.batchSize > 1 ||
    Boolean(
      task.conflict?.sourcePath &&
      summary &&
      summary.completed + summary.failed + summary.skipped + summary.pending > 1,
    );
  const feedback = useFeedbackAnchor<HTMLDivElement>(transferFeedbackKey(task), store.feedback);
  function currentTask() {
    const state = store.getSnapshot();
    if (!state.transportReady || state.runtimeId !== target.runtimeId) return null;
    const current = state.transfers.get(task.id);
    return current?.connectionId === target.connectionId &&
      current.batchId === target.batchId &&
      current.phase === 'awaitingDecision' &&
      current.conflictVersion === task.conflictVersion
      ? current
      : null;
  }
  async function act(decision: 'skip' | 'overwrite' | 'cancel') {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      if (!currentTask()) {
        setError(t('transfer_conflict_stale'));
        return;
      }
      if (decision === 'cancel') {
        const ids = [...store.getSnapshot().transfers.values()]
          .filter(
            (value) =>
              value.batchId === target.batchId &&
              value.connectionId === target.connectionId &&
              transferActive(value.phase),
          )
          .map((value) => value.id);
        for (const id of ids) {
          const state = store.getSnapshot();
          if (!state.transportReady || state.runtimeId !== target.runtimeId) {
            setError(t('transfer_notification_unavailable'));
            return;
          }
          const value = state.transfers.get(id);
          if (
            value?.batchId === target.batchId &&
            value.connectionId === target.connectionId &&
            transferActive(value.phase)
          )
            await store.call('cancelTransfer', { id });
        }
      } else {
        await store.call('resolveTransferConflict', {
          id: task.id,
          version: task.conflictVersion,
          decision,
          batch: batch && canApplyBatch,
        });
      }
      await store.refreshTransfers();
    } catch (cause) {
      setError(t(userErrorKey(errorCode(cause))));
      void store.refreshTransfers().catch(() => {});
    } finally {
      setBusy(false);
    }
  }
  return (
    <div ref={feedback} className="flex min-w-0 flex-col gap-4">
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
        {[
          [t('connection_target'), task.remoteIdentity],
          [
            t('transfer_conflict_source'),
            task.conflict?.sourcePath ?? (task.conflict ? t('transfer_conflict_archive') : '—'),
          ],
          [t('transfer_conflict_target'), task.conflict?.targetPath ?? '—'],
        ].map(([label, value]) => (
          <div className="contents" key={label}>
            <dt className="text-muted-foreground">{label}</dt>
            <dd className="min-w-0 select-text break-all">{value}</dd>
          </div>
        ))}
      </dl>
      {!task.conflict ? <p>{t('transfer_conflict_missing')}</p> : null}
      <div className="flex flex-col gap-3">
        {canApplyBatch ? (
          <Field orientation="horizontal" data-disabled={busy}>
            <Checkbox
              id={batchCheckboxId}
              checked={batch}
              onCheckedChange={setBatch}
              disabled={busy}
            />
            <FieldLabel htmlFor={batchCheckboxId}>{t('transfer_conflict_apply_batch')}</FieldLabel>
          </Field>
        ) : null}
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={busy} onClick={() => void act('cancel')}>
            {t('ws_cancel')}
          </Button>
          <Button
            variant="outline"
            disabled={busy || !task.conflict}
            onClick={() => void act('skip')}
          >
            {t('files_skip')}
          </Button>
          <Button
            variant="destructive"
            disabled={busy || !task.conflict}
            onClick={() => void act('overwrite')}
          >
            {t('files_overwrite')}
          </Button>
        </div>
      </div>
    </div>
  );
}
