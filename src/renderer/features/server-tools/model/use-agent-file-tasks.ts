import { useEffect, useState } from 'react';
import type { AgentTask } from '@contracts';
import { taskActive } from '../../mcp/session-model';
import { parentPath } from '../file-format';

export const isAgentFileTask = (task: AgentTask) =>
  /^(files_read\.|files_write\.|files_manage\.|files_transfer\.|files_remove$)/.test(task.tool);

export function agentFileLocation(task: AgentTask) {
  const target =
    [...task.targets].reverse().find((target) => target.effect === 'write') ?? task.targets[0];
  if (!target) return null;
  return {
    directory: task.tool === 'files_read.list' ? target.path : parentPath(target.path),
    paths: task.targets.map((target) => target.path),
  };
}

/** Subscribe before loading so an older snapshot cannot overwrite live task feedback. */
export function useAgentFileTasks(contextId: string | undefined, runtimeId: string | null) {
  const [state, setState] = useState<{
    contextId?: string;
    runtimeId: string | null;
    tasks: AgentTask[];
    error: string | null;
  }>({ runtimeId, tasks: [], error: null });
  useEffect(() => {
    if (!contextId) return;
    let alive = true;
    const tasks = new Map<string, AgentTask>();
    const publish = (error: string | null = null) => {
      if (!alive) return;
      const ordered = [...tasks.values()].sort(
        (a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id),
      );
      const recent = ordered.filter((task) => !taskActive(task)).slice(0, 5);
      for (const task of ordered.filter((task) => !taskActive(task)).slice(100))
        tasks.delete(task.id);
      setState({
        contextId,
        runtimeId,
        tasks: [...ordered.filter(taskActive), ...recent],
        error,
      });
    };
    const accept = (task: AgentTask) => {
      if (!alive || task.contextId !== contextId || !isAgentFileTask(task)) return;
      const old = tasks.get(task.id);
      if (old && (old.revision ?? 0) >= (task.revision ?? 0)) return;
      tasks.set(task.id, task);
    };
    publish();
    const unsubscribe = window.vigilshell.mcp.onChanged((event) => {
      if (
        event.kind !== 'task' ||
        event.task.contextId !== contextId ||
        !isAgentFileTask(event.task)
      )
        return;
      const old = tasks.get(event.task.id);
      accept(event.task);
      if (old !== tasks.get(event.task.id)) publish();
    });
    void window.vigilshell.mcp
      .request({
        action: 'tasks',
        sessionId: contextId,
        offset: 0,
        connectionId: null,
        entryId: null,
        phase: null,
      })
      .then((result) => {
        if (!alive) return;
        if (!result.ok) publish(result.code);
        else if (result.reply?.kind === 'tasks') {
          result.reply.tasks.forEach(accept);
          publish();
        }
      })
      .catch(() => publish('runtime_unavailable'));
    return () => {
      alive = false;
      unsubscribe();
    };
  }, [contextId, runtimeId]);
  return state.contextId === contextId && state.runtimeId === runtimeId && contextId
    ? state
    : { tasks: [], error: null };
}
