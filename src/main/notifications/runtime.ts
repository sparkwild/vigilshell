import type { BackendClient } from '../backend/client';
import type { NotificationFeedbackCoordinator } from './feedback';
import { translate } from '../../shared/i18n/messages';
export function observeRuntimeNotifications(
  host: BackendClient,
  feedback: NotificationFeedbackCoordinator,
  ports: { count(): number; planned(): boolean; locale(): 'en' | 'zh-CN'; available(): boolean },
) {
  const failed = new Set<string>();
  const unavailable = () => {
    const runtime = host.instanceId;
    if (!runtime || ports.planned() || failed.has(runtime)) return;
    failed.add(runtime);
    const count = ports.count();
    feedback.failRuntime(runtime);
    if (!count) return;
    void feedback.publish({
      eventId: `runtime:${runtime}`,
      runtimeId: runtime,
      category: 'executionIssue',
      target: { kind: 'runtime' },
      title: translate('notification_runtime_failed', ports.locale()),
      body: translate('notification_runtime_failed_body', ports.locale(), { count }),
      stillRelevant: () => !ports.available(),
    });
  };
  host.prependListener('unavailable', unavailable);
  return () => host.off('unavailable', unavailable);
}
