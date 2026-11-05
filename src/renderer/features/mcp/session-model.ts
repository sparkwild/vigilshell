import { capabilityLabels } from './policy/capability-labels';
import type { AgentTask, TaskPhase } from '@contracts';
import type { MessageKey } from '../../../shared/i18n/messages';
export const taskPhaseKeys: Record<TaskPhase, MessageKey> = {
  queued: 'mcp_queued',
  waitingReview: 'agent_script_waiting_review',
  running: 'mcp_task_running',
  stopping: 'agent_stopping',
  succeeded: 'mcp_succeeded',
  requestAccepted: 'agent_request_accepted',
  failed: 'mcp_failed',
  denied: 'mcp_denied',
  cancelled: 'mcp_cancelled',
  timedOut: 'mcp_timed_out',
  interruptedUnknown: 'mcp_interrupted_unknown',
};
export const taskActive = (task: Pick<AgentTask, 'phase'>) =>
  ['queued', 'waitingReview', 'running', 'stopping'].includes(task.phase);

export const toolKey = (tool: string): MessageKey => capabilityLabels[tool] ?? 'agent_operation';
