import type { TransferView } from '@contracts';

/** 同一次失败或冲突在局部展示后不重播；重试和新的冲突仍是独立提醒。 */
export function transferFeedbackKey(
  task: Pick<TransferView, 'id' | 'attempt' | 'phase' | 'conflictVersion'>,
) {
  return task.phase === 'failed' || task.phase === 'awaitingDecision'
    ? `transfer:${task.id}:${task.attempt}:${task.phase}:${task.conflictVersion}`
    : null;
}
