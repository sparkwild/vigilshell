import { TerminalFloatingNotice } from './TerminalFloatingNotice';
import { unexpectedTerminalEnd } from '../connections/session-state';
import { CloseButton } from '../../components/ui/icon-button';
import { useEffect, useLayoutEffect, useState } from 'react';
import { CircleCheckIcon, LoaderCircleIcon } from 'lucide-react';
import type { TerminalSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { TruncatedText } from '../../components/ui/tooltip';
import { OperationStatus } from '../connections/OperationFeedback';
import { errorCode, phaseKey, useWorkspace } from '../connections/workspace-context';
import { ConnectionFailure } from './ConnectionFailure';

export function TerminalConnectionInfo({
  session,
  visible,
}: {
  session: TerminalSessionView;
  visible: boolean;
}) {
  const { store } = useWorkspace();
  const { t } = useDesktop();
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [pageVisible, setPageVisible] = useState(!document.hidden);
  useEffect(() => {
    const update = () => setPageVisible(!document.hidden);
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  }, []);
  useLayoutEffect(() => {
    if (!visible) {
      setHovered(false);
      setFocused(false);
    }
  }, [visible]);
  useLayoutEffect(() => {
    if (!visible || !pageVisible || hovered || focused || session.phase !== 'running') return;
    return store.startTerminalInfoCountdown(session.id);
  }, [store, session.id, session.phase, visible, pageVisible, hovered, focused]);
  const ended = ['closed', 'failed', 'cancelled'].includes(session.phase);
  const establishing = !ended && session.phase !== 'running';
  const host =
    session.host.includes(':') && !session.host.startsWith('[')
      ? `[${session.host}]`
      : session.host;
  const busy = store.feedback.isBusy('session:' + session.id);
  return (
    <TerminalFloatingNotice
      aria-label={t('ws_connection_info')}
      data-phase={session.phase}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      onFocusCapture={() => setFocused(true)}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      }}
      actions={
        ended ? (
          <>
            <Button
              size="xs"
              variant="outline"
              disabled={!store.canReconnectTerminal(session.id)}
              onClick={() =>
                void store
                  .reconnectTerminal(session.id)
                  .catch((error) => store.report(errorCode(error), 'session:' + session.id))
              }
            >
              {t('ws_reconnect')}
            </Button>
            <Button
              size="xs"
              variant="ghost"
              disabled={busy}
              onClick={() =>
                void store
                  .connect(session.connectionId, true)
                  .catch((error) => store.report(errorCode(error), 'session:' + session.id))
              }
            >
              {t('ws_connect_new')}
            </Button>
          </>
        ) : null
      }
      dismiss={
        <CloseButton
          onClick={() => {
            store.hideTerminalInfo(session.id);
            store.terminals.get(session.id)?.focus();
          }}
          label={t('ws_hide_connection_info')}
        />
      }
      detail={
        ended &&
        (unexpectedTerminalEnd(session) || session.exitCode !== null) && (
          <>
            <ConnectionFailure key={`${session.id}:${session.error}`} session={session} />
            {session.exitCode !== null && (
              <div className="terminal-connection-exit">
                {t(session.exitCode === 0 ? 'connection_exit_normal' : 'connection_exit_failed')}
              </div>
            )}
          </>
        )
      }
    >
      <OperationStatus owner={'session:' + session.id} />
      {establishing && (
        <LoaderCircleIcon
          aria-hidden="true"
          className="terminal-connection-progress animate-spin motion-reduce:animate-none"
        />
      )}
      <Badge
        variant={
          session.phase === 'failed'
            ? 'destructive'
            : session.phase === 'running'
              ? 'connected'
              : 'secondary'
        }
      >
        {session.phase === 'running' && <CircleCheckIcon data-icon="inline-start" />}
        {t(session.phase === 'running' ? 'ws_connection_success' : phaseKey(session.phase))}
      </Badge>
      <TruncatedText className="terminal-notice-label truncate">{`${session.username}@${host}:${session.port}`}</TruncatedText>
      {establishing && (
        <Button
          size="xs"
          variant="ghost"
          disabled={busy}
          onClick={() =>
            void store
              .disconnectTerminal(session.id)
              .catch((error) => store.report(errorCode(error), 'session:' + session.id))
          }
        >
          {t('ws_cancel')}
        </Button>
      )}
    </TerminalFloatingNotice>
  );
}
