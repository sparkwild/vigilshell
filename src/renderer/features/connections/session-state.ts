import type { TerminalSessionView } from '@contracts';

export const terminalEnded = (session: TerminalSessionView) =>
  ['closed', 'failed', 'cancelled'].includes(session.phase);

export function unexpectedTerminalEnd(session: TerminalSessionView) {
  if (!terminalEnded(session)) return false;
  return (
    session.phase === 'failed' ||
    session.endReason?.kind === 'transportError' ||
    session.endReason?.kind === 'remoteClosed' ||
    (session.endReason?.kind === 'remoteExit' && session.exitCode !== 0)
  );
}
