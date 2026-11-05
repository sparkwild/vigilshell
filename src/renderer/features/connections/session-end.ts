import type { TerminalSessionView } from '@contracts';
import type { MessageKey } from '../../../shared/i18n/messages';
import { connectionFailureTitle } from './connection-diagnostics';

export { terminalEnded, unexpectedTerminalEnd } from './session-state';

export function terminalEndTitle(session: TerminalSessionView): MessageKey {
  if (session.error && session.phase !== 'cancelled') return connectionFailureTitle(session.error);
  switch (session.endReason?.kind) {
    case 'userDisconnected':
      return 'connection_end_user';
    case 'cancelled':
      return 'connection_end_cancelled';
    case 'remoteExit':
      return 'connection_end_remote_exit';
    case 'remoteClosed':
      return session.endReason.message ? 'connection_end_remote_closed' : 'connection_end_unknown';
    case 'transportError':
      return 'connection_interrupted';
    default:
      return session.phase === 'failed'
        ? 'connection_failed_unknown'
        : session.phase === 'cancelled'
          ? 'connection_end_cancelled'
          : 'connection_end_pending';
  }
}
