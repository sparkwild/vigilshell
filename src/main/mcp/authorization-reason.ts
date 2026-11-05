import type { AgentMutation, AgentOperation, AgentReply, ResolvedLocale } from '@contracts';
import { translate } from '../../shared/i18n/messages';

function label(value: string) {
  return [
    ...value
      .replace(/[\p{Cc}\p{Cf}]/gu, ' ')
      .replace(/\s+/g, ' ')
      .trim(),
  ]
    .slice(0, 96)
    .join('');
}

export async function authorizationReason(
  change: AgentMutation,
  locale: ResolvedLocale,
  ports: {
    request(operation: AgentOperation): Promise<AgentReply>;
    connectionName(id: string): Promise<string>;
  },
) {
  switch (change.kind) {
    case 'approveConnection': {
      const reply = await ports.request({ operation: 'contexts' });
      if (reply.kind !== 'contexts') throw Error('invalid_response');
      const context = reply.contexts.find((value) => value.id === change.context_id);
      if (!context || context.phase !== 'waitingUser') throw Error('authorization_expired');
      return translate('mcp_session_auth_reason', locale, {
        agent: label(context.entryName),
        connection: label(context.connectionName),
      });
    }
    case 'setPolicyAccess':
      return translate('mcp_access_auth_reason', locale, {
        connection: label(await ports.connectionName(change.connection_id)),
      });
    case 'approveScript': {
      const reply = await ports.request({ operation: 'task', task_id: change.task_id });
      if (reply.kind !== 'task') throw Error('invalid_response');
      if (reply.task.id !== change.task_id || reply.task.phase !== 'waitingReview')
        throw Error('review_not_waiting');
      return translate('mcp_script_auth_reason', locale, {
        connection: label(reply.task.connectionName),
      });
    }
    default:
      return translate('mcp_auth_reason', locale);
  }
}
