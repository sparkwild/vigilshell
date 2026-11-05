import { useNotificationSurface } from '../features/notifications/NotificationFeedbackProvider';
import { useCallback, useRef, useState } from 'react';
import type { ErrorCode } from '@contracts';
import { useDesktop } from './DesktopProvider';
import { Alert, AlertDescription, AlertTitle } from '../components/ui/alert';
import { Button } from '../components/ui/button';
import { errorMessage } from '../../shared/i18n/messages';

function useRuntimeFeedback() {
  const { store, snapshot, t } = useDesktop();
  const [retrying, setRetrying] = useState(false);
  const inFlight = useRef(false);
  const retry = useCallback(async () => {
    if (inFlight.current || store.getSnapshot().runtime.status !== 'unavailable') return;
    inFlight.current = true;
    setRetrying(true);
    try {
      await store.retry();
    } catch {
      // Keep the existing failure visible when the desktop bridge cannot retry.
    } finally {
      inFlight.current = false;
      setRetrying(false);
    }
  }, [store]);
  const storageFailure = snapshot.error === 'storage_unavailable';
  return {
    t,
    status: snapshot.runtime?.status ?? 'starting',
    title: t(storageFailure ? 'settings_load_failed' : 'backend_unavailable'),
    description: storageFailure
      ? undefined
      : t(
          errorMessage[(snapshot.error ?? 'runtime_unavailable') as ErrorCode] ??
            'error_host_unavailable',
        ),
    retry,
    retrying,
  };
}

/** A fixed local status surface; startup remains silent. */
export function RuntimeFeedback({ settingsOpen }: { settingsOpen: boolean }) {
  if (settingsOpen) return null;
  return <SettingsRuntimeFeedback />;
}

export function SettingsRuntimeFeedback() {
  const { t, status, title, description, retry, retrying } = useRuntimeFeedback();
  const feedback = useNotificationSurface(status === 'unavailable' ? 'runtime:' : null);
  if (status !== 'unavailable') return null;
  return (
    <Alert ref={feedback} className="w-fit max-w-full">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div>
          <AlertTitle>{title}</AlertTitle>
          {description ? <AlertDescription>{description}</AlertDescription> : null}
        </div>
        <Button size="xs" variant="outline" disabled={retrying} onClick={() => void retry()}>
          {t('retry')}
        </Button>
      </div>
    </Alert>
  );
}
