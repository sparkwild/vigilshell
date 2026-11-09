import type { TransferNotificationTarget, TransferView } from '@contracts';
import { transferActive } from './file-format';

export const transferBatchKey = (task: TransferView) => `${task.connectionId}:${task.batchId}`;
export function transferIsCovered(
  task: TransferView,
  runtimeId: string | null,
  sidebarConnection: string | null,
  windowConnection: string | undefined,
  windowTarget?: TransferNotificationTarget,
) {
  if (task.connectionId === sidebarConnection) return true;
  return (
    task.connectionId === windowConnection &&
    (!windowTarget ||
      (windowTarget.runtimeId === runtimeId && windowTarget.batchId === task.batchId))
  );
}
export interface FloatingBatch {
  active: boolean;
  eligible: boolean;
  dismissed: boolean;
  until: number | null;
}
export interface FloatingState {
  runtimeId: string | null;
  batches: ReadonlyMap<string, FloatingBatch>;
}
export function updateFloatingState(
  previous: FloatingState,
  runtimeId: string | null,
  tasks: readonly TransferView[],
  covered: ReadonlySet<string>,
  now: number,
): FloatingState {
  const old =
    previous.runtimeId === runtimeId ? previous.batches : new Map<string, FloatingBatch>();
  const grouped = new Map<string, TransferView[]>();
  for (const task of tasks) {
    const key = transferBatchKey(task);
    grouped.set(key, [...(grouped.get(key) ?? []), task]);
  }
  const batches = new Map<string, FloatingBatch>();
  let changed = previous.runtimeId !== runtimeId || old.size !== grouped.size;
  for (const [key, values] of grouped) {
    const before = old.get(key);
    const active = values.some((task) => transferActive(task.phase));
    const hidden = values.some((task) => !covered.has(task.id));
    const eligible = active
      ? (before?.eligible ?? false) || hidden
      : hidden && (before?.eligible ?? false);
    const until =
      !active && before?.active && eligible ? now + 4000 : active ? null : (before?.until ?? null);
    const next = { active, eligible, until, dismissed: before?.dismissed ?? false };
    batches.set(key, next);
    if (
      !before ||
      before.active !== active ||
      before.eligible !== eligible ||
      before.until !== until
    )
      changed = true;
  }
  return changed ? { runtimeId, batches } : previous;
}
export function floatingCandidates(
  tasks: readonly TransferView[],
  state: FloatingState,
  covered: ReadonlySet<string>,
  now: number,
) {
  return tasks.filter((task) => {
    const batch = state.batches.get(transferBatchKey(task));
    return (
      !covered.has(task.id) &&
      batch?.eligible &&
      !batch.dismissed &&
      ((transferActive(task.phase) && task.phase !== 'awaitingDecision') ||
        task.phase === 'failed' ||
        (!batch.active && task.phase === 'complete' && batch.until !== null && batch.until > now))
    );
  });
}
