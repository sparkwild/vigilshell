import type { NotificationFeedback, NotificationTarget } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Alert, AlertDescription, AlertTitle } from '../../components/ui/alert';
import { Button } from '../../components/ui/button';
import { useNotificationFeedback, useNotificationSurface } from './NotificationFeedbackProvider';
export function needsAttention(item: NotificationFeedback) {
  return (
    item.relevant &&
    (item.category === 'attentionRequired' ||
      item.route !== 'presented' ||
      /unknown/i.test(item.id))
  );
}
export function FeedbackItem({ item }: { item: NotificationFeedback }) {
  const { t } = useDesktop();
  const ref = useNotificationSurface(`event:${item.id}`);
  return (
    <Alert ref={ref}>
      <AlertTitle>{item.title}</AlertTitle>
      <AlertDescription>
        <div className="flex flex-col gap-2">
          <span>{item.body}</span>
          <Button
            size="xs"
            variant="link"
            onClick={() => void window.vigilshell.notifications.activate(item.id)}
          >
            {t('notification_view')}
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  );
}
export function FeatureFeedback({ kinds }: { kinds: NotificationTarget['kind'][] }) {
  const items = useNotificationFeedback().filter(
    (item) => item.relevant && kinds.includes(item.target.kind),
  );
  if (!items.length) return null;
  return (
    <div className="flex max-h-64 shrink-0 flex-col gap-2 overflow-y-auto">
      {items.map((item) => (
        <FeedbackItem key={item.id} item={item} />
      ))}
    </div>
  );
}
