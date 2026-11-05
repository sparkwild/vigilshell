import type { ActivitySummary } from '@contracts';
export function activityDuration(item: ActivitySummary, now: number): number | 'unknown' | null {
  if (!item.taskId || item.phase === 'requestAccepted') return null;
  if (item.uncertain || item.phase === 'interruptedUnknown') return 'unknown';
  if (['queued', 'waitingReview'].includes(item.phase ?? '')) return null;
  if (item.startedAt == null) {
    return ['queued', 'waitingReview', 'denied', 'cancelled'].includes(item.phase ?? '')
      ? null
      : 'unknown';
  }
  const active = ['running', 'stopping'].includes(item.phase ?? '');
  const end = active ? now : item.finishedAt;
  if (end == null || end < item.startedAt) return 'unknown';
  return Math.floor((end - item.startedAt) / 1000);
}
export function compactDuration(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}
