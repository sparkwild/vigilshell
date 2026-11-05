import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { useFeedbackAnchor } from '../connections/feedback/use-operation-anchor';
import { useState, useEffect, useRef } from 'react';
import { AlertCircleIcon, CopyIcon } from 'lucide-react';
import type { TerminalSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorCode, phaseKey } from '../connections/workspace-context';
import { Button } from '../../components/ui/button';
import { connectionAdvice } from '../connections/connection-diagnostics';
import { terminalEndTitle, unexpectedTerminalEnd } from '../connections/session-end';
export { connectionAdvice } from '../connections/connection-diagnostics';

export function ConnectionFailure({
  session,
  expanded = false,
  revealToken = 0,
}: {
  session: TerminalSessionView;
  expanded?: boolean;
  revealToken?: number;
}) {
  const { t } = useDesktop(),
    { store } = useWorkspace(),
    [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const feedback = useFeedbackAnchor<HTMLDivElement>(
    unexpectedTerminalEnd(session) ? 'session:' + session.id : null,
    store.feedback,
  );
  const notificationRef = useNotificationSurface(
    unexpectedTerminalEnd(session) ? `connection:${session.id}` : null,
  );
  const details = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    if (expanded && details.current) details.current.open = true;
  }, [expanded, revealToken, session.id]);
  if (!unexpectedTerminalEnd(session)) return null;
  const code =
    session.error && /^[a-z][a-z0-9_]{0,127}$/.test(session.error)
      ? session.error
      : 'unknown_error';
  const title = t(terminalEndTitle(session));
  const fields = [
    [t('connection_target'), `${session.username}@${session.host}:${session.port}`],
    [t('connection_state'), t(phaseKey(session.phase))],
  ];
  return (
    <div ref={feedback} className="connection-failure">
      <div ref={notificationRef} className="connection-failure-summary" role="alert">
        <AlertCircleIcon aria-hidden="true" />
        <p>{title}</p>
      </div>
      <details ref={details}>
        <summary>{t('connection_details')}</summary>
        <p>{t(connectionAdvice(code))}</p>
        <Button
          size="xs"
          variant="outline"
          onClick={() =>
            void store.editConnection(session.connectionId).catch((e) => store.report(errorCode(e)))
          }
        >
          {t('ws_edit_connection')}
        </Button>
        <dl>
          {fields.map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
        <Button
          size="xs"
          variant="outline"
          onClick={() =>
            void navigator.clipboard
              .writeText(
                [
                  title,
                  t(connectionAdvice(code)),
                  ...fields.map(([label, value]) => `${label}: ${value}`),
                ].join('\n'),
              )
              .then(
                () => setCopyState('copied'),
                () => setCopyState('failed'),
              )
          }
        >
          <CopyIcon />
          {t('connection_copy')}
        </Button>
        <span role="status">
          {copyState === 'copied'
            ? t('connection_copied')
            : copyState === 'failed'
              ? t('connection_copy_failed')
              : ''}
        </span>
      </details>
    </div>
  );
}
