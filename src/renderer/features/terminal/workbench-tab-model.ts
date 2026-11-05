import type { ConnectionSummary } from '@contracts';
import type { WorkbenchTab } from '../connections/workspace-store';
import type { MessageKey } from '../../../shared/i18n/messages';
import { phaseKey } from '../connections/workspace-context';
import { terminalLabel } from './terminal-tab-model';
export interface WorkbenchTabLabel {
  tab: WorkbenchTab;
  id: string;
  label: string;
  name: string;
  ordinal: number;
  description: string;
  phase: MessageKey;
  environment: ConnectionSummary['environment'];
  establishing: boolean;
  failed: boolean;
}
export function tabLabel(
  tab: WorkbenchTab,
  connections: ConnectionSummary[],
  ordinal: number,
): WorkbenchTabLabel {
  const connectionId =
    tab.kind === 'terminal' ? tab.session.connectionId : tab.view.context.connectionId;
  const connection = connections.find((connection) => connection.id === connectionId);
  if (tab.kind === 'terminal') {
    const session = tab.session;
    return {
      tab,
      id: tab.id,
      ordinal,
      label: terminalLabel(session, connection?.name, ordinal),
      name: terminalLabel(session, connection?.name, 1),
      description: `${session.username}@${session.host}:${session.port}`,
      phase: phaseKey(session.phase),
      environment: connection?.environment ?? 'none',
      establishing: ['connecting', 'trust', 'authenticating', 'opening'].includes(session.phase),
      failed: session.phase === 'failed',
    };
  }
  const context = tab.view.context;
  const name = context.connectionName || 'Agent';
  const phase: MessageKey = tab.view.runningTasks
    ? 'agent_session_running'
    : (
        {
          waitingUser: 'agent_session_waiting',
          connecting: 'agent_session_connecting',
          ready: 'agent_session_ready',
          closing: 'agent_session_closing',
          closed: 'agent_session_closed',
          disconnected: 'agent_session_unknown',
          failed: 'agent_session_failed',
        } as const
      )[context.phase];
  return {
    tab,
    id: tab.id,
    ordinal,
    name,
    label: ordinal > 1 ? `${name} · ${ordinal}` : name,
    description: context.entryName,
    phase,
    environment: connection?.environment ?? 'none',
    establishing:
      tab.view.runningTasks > 0 || context.phase === 'connecting' || context.phase === 'closing',
    failed: context.phase === 'failed' || context.phase === 'disconnected',
  };
}
