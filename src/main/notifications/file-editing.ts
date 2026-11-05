import type { FileEditingService, FileEditingNotice } from '../file-editing/service';
import type { NotificationFeedbackCoordinator } from './feedback';
import { translate } from '../../shared/i18n/messages';
import { connectionFailureCodes } from './workspace';
export function observeFileEditingNotifications(
  service: FileEditingService,
  feedback: NotificationFeedbackCoordinator,
  locale: () => 'en' | 'zh-CN',
) {
  const states = new Map<string, FileEditingNotice>();
  const observe = (notice: FileEditingNotice) => {
    states.set(notice.id, notice);
    feedback.refresh();
    if (
      !['conflict', 'failed', 'unknown', 'disconnected'].includes(notice.phase) ||
      (notice.phase !== 'conflict' && !notice.unsynced)
    )
      return;
    const key =
      notice.phase === 'conflict'
        ? 'notification_editor_conflict'
        : notice.phase === 'unknown'
          ? 'notification_editor_unknown'
          : notice.phase === 'disconnected'
            ? 'notification_editor_disconnected'
            : 'notification_editor_failed';
    void feedback.publish({
      eventId: `editor:${service.owner}:${notice.id}:${notice.attempt}:${notice.phase}`,
      runtimeId: notice.runtimeId,
      connectionId: notice.connectionId,
      cause:
        notice.error === 'runtime_unavailable'
          ? 'runtime'
          : connectionFailureCodes.has(notice.error ?? '') || notice.phase === 'disconnected'
            ? 'connection'
            : undefined,
      category: notice.phase === 'conflict' ? 'attentionRequired' : 'executionIssue',
      title: translate(key, locale()),
      body: translate('notification_editor_body', locale()),
      target: { kind: 'fileEditor', id: notice.id },
      stillRelevant: () =>
        states.get(notice.id) === notice &&
        service.list().some((item) => item.id === notice.id && item.phase === notice.phase),
    });
  };
  const changed = () => {
    const live = new Set(service.list().map((item) => item.id));
    let removed = false;
    for (const id of states.keys())
      if (!live.has(id)) {
        states.delete(id);
        removed = true;
      }
    if (removed) feedback.refresh();
  };
  service.on('notification', observe);
  service.on('changed', changed);
  return () => {
    service.off('notification', observe);
    service.off('changed', changed);
    states.clear();
  };
}
