import { useState } from 'react';
import { Button } from '../../components/ui/button';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { Alert, AlertDescription, AlertTitle } from '../../components/ui/alert';
import { useDesktop } from '../../app/DesktopProvider';
export function McpFeedback({ code, onRetry }: { code: string | null; onRetry?: () => void }) {
  const { t } = useDesktop();
  const [retrying, setRetrying] = useState(false);
  if (!code) return null;
  return (
    <Alert>
      <AlertTitle>{t('mcp_error')}</AlertTitle>
      <AlertDescription>
        <span>{t(userErrorKey(code, 'agent_operation_failed'))}</span>
        {onRetry && ['configuration_conflict', 'connection_changed'].includes(code) ? (
          <Button size="sm" variant="outline" onClick={onRetry}>
            {t('agent_reload')}
          </Button>
        ) : null}
        {code === 'activity_recording_unavailable' ? (
          <Button
            size="sm"
            variant="outline"
            disabled={retrying}
            onClick={() => {
              setRetrying(true);
              void window.vigilshell.mcp
                .request({ action: 'retryRecording' })
                .finally(() => setRetrying(false));
            }}
          >
            {t('tools_refresh')}
          </Button>
        ) : null}
      </AlertDescription>
    </Alert>
  );
}
