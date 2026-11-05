import { useEffect } from 'react';
import { hasNotificationTarget } from './target-availability';
import type { NotificationTarget } from '@contracts';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../../components/ui/dialog';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorKey } from '../connections/workspace-context';
import { FileOperationsDialog } from '../server-tools/FileOperationsDialog';
import { useNotificationSurface } from './NotificationFeedbackProvider';
export function NotificationDestination({
  target,
  close,
  onUnavailable,
}: {
  target: NotificationTarget;
  close(): void;
  onUnavailable(): void;
}) {
  const { state } = useWorkspace();
  const { t } = useDesktop();
  const tunnel =
    target.kind === 'connection' && target.runtimeId === state.runtimeId
      ? state.tunnelStates.get(target.connectionId)?.find((item) => item.ruleId === target.tunnelId)
      : undefined;
  const anchor = useNotificationSurface(
    tunnel && target.kind === 'connection' ? `tunnel:${target.tunnelId}` : null,
  );
  const available =
    hasNotificationTarget(target, state) && (target.kind === 'fileOperation' || !!tunnel);
  useEffect(() => {
    if (!available) onUnavailable();
  }, [available, onUnavailable]);
  if (!available) return null;
  if (target.kind === 'fileOperation')
    return (
      <FileOperationsDialog
        sessionId={target.sessionId}
        initialId={target.operationId}
        close={close}
        onUnavailable={onUnavailable}
      />
    );
  if (!tunnel) return null;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t('ws_tunnels')}</DialogTitle>
        </DialogHeader>
        <div ref={anchor}>
          <Alert>
            <AlertDescription>
              {t(
                tunnel.phase === 'running'
                  ? 'ws_tunnel_running'
                  : errorKey(tunnel.error ?? 'tools_unavailable'),
              )}
            </AlertDescription>
          </Alert>
        </div>
      </DialogContent>
    </Dialog>
  );
}
