import type { AgentEvent, WorkspaceEvent, RpcCommand, WorkspaceCommand } from '@contracts';

interface DiagnosticContext {
  requestId?: string;
  method?: RpcCommand['method'] | WorkspaceCommand['method'];
  operationId?: string;
  sessionId?: string;
  taskId?: string;
  reason?: string;
  exitCode?: number | null;
}

/** 只记录稳定代码与关联信息；请求正文、凭据和远端返回的任意文本不进入诊断。 */
export function diagnosticFailure(stage: string, code: string, context: DiagnosticContext = {}) {
  const partial = code.startsWith('partial:');
  if (partial) code = code.slice(8);
  console.warn(
    JSON.stringify({
      application: 'vigilshell',
      stage,
      outcome: 'failed',
      ...(partial ? { partial: true } : {}),
      code: /^[a-z][a-z0-9_]{0,127}$/.test(code) ? code : 'unclassified_error',
      ...context,
    }),
  );
}

export function workspaceDiagnostic(event: WorkspaceEvent) {
  if (event.kind === 'session') {
    const s = event.session;
    if (
      s.error ||
      s.endReason?.kind === 'transportError' ||
      s.endReason?.kind === 'remoteClosed' ||
      (s.exitCode !== null && s.exitCode !== 0)
    )
      diagnosticFailure('terminal_session', s.error ?? 'remote_session_ended', {
        sessionId: s.id,
        operationId: s.operationId,
        reason: s.endReason?.kind,
        exitCode: s.exitCode,
      });
  } else if (event.kind === 'transfer' && event.task.error) {
    diagnosticFailure('file_transfer', event.task.error, { taskId: event.task.id });
  } else if (event.kind === 'operation' && event.operation.error) {
    diagnosticFailure('workspace_operation', event.operation.error, {
      operationId: event.operation.id,
    });
  } else if (event.kind === 'inputRejected') {
    diagnosticFailure('terminal_input', event.code, { sessionId: event.sessionId });
  }
}

export function agentDiagnostic(event: AgentEvent) {
  if (event.kind === 'task' && event.task.error)
    diagnosticFailure('agent_task', event.task.error, {
      taskId: event.task.id,
      sessionId: event.task.contextId,
      exitCode: event.task.exitCode,
    });
  else if (event.kind === 'context' && event.context.error)
    diagnosticFailure('agent_session', event.context.error, { sessionId: event.context.id });
  else if (
    event.kind === 'attention' &&
    !['connection_approval_required', 'script_review_required'].includes(event.code)
  )
    diagnosticFailure('agent_attention', event.code);
}
