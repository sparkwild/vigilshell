// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentEvent, AgentTask } from '@contracts';
import { agentFileLocation, useAgentFileTasks } from './use-agent-file-tasks';

afterEach(cleanup);
const fileTask = (id: string, changes: Partial<AgentTask> = {}) =>
  ({
    id,
    contextId: 'ctx',
    tool: 'files_write.write',
    phase: 'running',
    revision: 2,
    createdAt: 1,
    targets: [{ path: '/tmp/probe.txt', effect: 'write', recursive: false }],
    ...changes,
  }) as AgentTask;

it('keeps concurrent tasks, rejects stale snapshots and isolates releases and new runtimes', async () => {
  const listeners = new Set<(event: AgentEvent) => void>();
  const pending: ((result: unknown) => void)[] = [];
  const request = vi.fn(() => new Promise((done) => pending.push(done)));
  Object.assign(window, {
    vigilshell: {
      mcp: {
        request,
        onChanged: (listener: (event: AgentEvent) => void) => {
          listeners.add(listener);
          return () => listeners.delete(listener);
        },
      },
    },
  });
  const emit = (task: AgentTask) =>
    act(() => listeners.forEach((listener) => listener({ kind: 'task', task })));
  const view = renderHook(({ contextId, runtimeId }) => useAgentFileTasks(contextId, runtimeId), {
    initialProps: { contextId: 'ctx' as string | undefined, runtimeId: 'r1' },
  });
  emit(fileTask('write'));
  emit(fileTask('read', { tool: 'files_read.read', createdAt: 2 }));
  emit(fileTask('command', { tool: 'command_run.command' }));
  emit(fileTask('other', { contextId: 'other' }));
  expect(view.result.current.tasks.map((task) => task.id)).toEqual(['read', 'write']);
  emit(fileTask('write', { phase: 'succeeded', revision: 3 }));
  await act(async () =>
    pending[0]!({ ok: true, reply: { kind: 'tasks', tasks: [fileTask('write')] } }),
  );
  expect(view.result.current.tasks.find((task) => task.id === 'write')?.phase).toBe('succeeded');
  emit(fileTask('read', { phase: 'failed', error: 'permission_denied', revision: 3 }));
  expect(view.result.current.tasks.find((task) => task.id === 'read')?.error).toBe(
    'permission_denied',
  );
  view.rerender({ contextId: 'ctx', runtimeId: 'r2' });
  expect(view.result.current.tasks).toEqual([]);
  emit(fileTask('new'));
  view.rerender({ contextId: undefined, runtimeId: 'r2' });
  expect(view.result.current.tasks).toEqual([]);
  await act(async () =>
    pending[1]!({ ok: true, reply: { kind: 'tasks', tasks: [fileTask('late')] } }),
  );
  expect(view.result.current.tasks).toEqual([]);
  expect(listeners.size).toBe(0);
});

it('reports snapshot failures without discarding live file feedback', async () => {
  let receive!: (event: AgentEvent) => void;
  Object.assign(window, {
    vigilshell: {
      mcp: {
        request: vi.fn(async () => ({ ok: false, code: 'runtime_unavailable' })),
        onChanged: (listener: typeof receive) => {
          receive = listener;
          return () => {};
        },
      },
    },
  });
  const { result } = renderHook(() => useAgentFileTasks('ctx', 'r'));
  await waitFor(() => expect(result.current.error).toBe('runtime_unavailable'));
  act(() => receive({ kind: 'task', task: fileTask('file') }));
  expect(result.current.tasks).toHaveLength(1);
});

it('uses the listed directory itself and the destination parent for moves, without local paths', () => {
  expect(
    agentFileLocation(
      fileTask('list', {
        tool: 'files_read.list',
        targets: [{ path: '/var/log', effect: 'read', recursive: false }],
      }),
    )?.directory,
  ).toBe('/var/log');
  expect(
    agentFileLocation(
      fileTask('move', {
        tool: 'files_manage.move',
        targets: [
          { path: '/tmp/a', effect: 'remove', recursive: false },
          { path: '/var/tmp/b', effect: 'write', recursive: false },
        ],
      }),
    ),
  ).toEqual({ directory: '/var/tmp', paths: ['/tmp/a', '/var/tmp/b'] });
});
