import { useEffect, useState } from 'react';
import type { AgentTask } from '@contracts';
import { ScriptReview } from './ScriptReview';
import { McpFeedback } from './McpFeedback';
import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';

/** 普通终端仍须保留脚本执行前的内容审阅入口。 */
export function useAgentWorkspaceReview({ sessionId }: { sessionId: string }) {
  const [tasks, setTasks] = useState<AgentTask[]>([]);
  const [error, setError] = useState<string | null>(null);
  const notice = useNotificationSurface(tasks[0] ? `agentTask:${tasks[0].id}` : null);
  useEffect(() => {
    let active = true;
    const revisions = new Map<string, AgentTask>();
    const accept = (task: AgentTask) => {
      if (
        task.contextId !== sessionId ||
        (revisions.get(task.id)?.revision ?? -1) > (task.revision ?? 0)
      )
        return;
      revisions.set(task.id, task);
      setTasks([...revisions.values()].filter((task) => task.phase === 'waitingReview'));
    };
    const unsubscribe = window.vigilshell.mcp.onChanged((event) => {
      if (event.kind === 'task') accept(event.task);
    });
    void window.vigilshell.mcp
      .request({
        action: 'tasks',
        sessionId,
        offset: 0,
        connectionId: null,
        entryId: null,
        phase: 'waitingReview',
      })
      .then((result) => {
        if (!active) return;
        if (!result.ok) setError(result.code);
        else if (result.reply?.kind === 'tasks') result.reply.tasks.forEach(accept);
      })
      .catch(() => {
        if (active) setError('runtime_unavailable');
      });
    return () => {
      active = false;
      unsubscribe();
    };
  }, [sessionId]);
  if (!tasks.length && !error) return null;
  return (
    <div ref={notice} className="max-w-xl">
      <McpFeedback code={error} />
      {tasks.map((task) => (
        <ScriptReview key={task.id} task={task} />
      ))}
    </div>
  );
}
