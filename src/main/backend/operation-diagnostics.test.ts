import { afterEach, expect, it, vi } from 'vitest';
import type { TerminalSessionView } from '@contracts';
import { agentDiagnostic, diagnosticFailure, workspaceDiagnostic } from './operation-diagnostics';
afterEach(() => vi.restoreAllMocks());
it('retains technical correlation fields without logging arbitrary remote content', () => {
  const log = vi.spyOn(console, 'warn').mockImplementation(() => {});
  workspaceDiagnostic({
    kind: 'session',
    session: {
      id: 'session-id',
      operationId: 'operation-id',
      error: 'disconnected',
      exitCode: 7,
      endReason: { kind: 'transportError', message: 'private remote payload' },
    } as TerminalSessionView,
  });
  expect(JSON.parse(log.mock.calls[0]![0])).toMatchObject({
    code: 'disconnected',
    sessionId: 'session-id',
    operationId: 'operation-id',
    reason: 'transportError',
    exitCode: 7,
  });
  expect(log.mock.calls[0]![0]).not.toContain('private remote payload');
});
it('does not treat an arbitrary exception message as a safe log code', () => {
  const log = vi.spyOn(console, 'warn').mockImplementation(() => {});
  diagnosticFailure('agent_ui', 'Error: token=private-value');
  expect(JSON.parse(log.mock.calls[0]![0]).code).toBe('unclassified_error');
  expect(log.mock.calls[0]![0]).not.toContain('private-value');
});
it('retains the partial marker and original stable code separately', () => {
  const log = vi.spyOn(console, 'warn').mockImplementation(() => {});
  diagnosticFailure('agent_task', 'partial:file_commit_unknown', { taskId: 'task' });
  expect(JSON.parse(log.mock.calls[0]![0])).toMatchObject({
    partial: true,
    code: 'file_commit_unknown',
    taskId: 'task',
  });
});

it('expected script-review attention is not logged as a failed operation', () => {
  const log = vi.spyOn(console, 'warn').mockImplementation(() => {});
  agentDiagnostic({
    kind: 'attention',
    id: 'script',
    connection_id: 'connection',
    code: 'script_review_required',
  });
  expect(log).not.toHaveBeenCalled();
});
