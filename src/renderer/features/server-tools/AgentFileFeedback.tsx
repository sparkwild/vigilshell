import type { AgentTask } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Badge } from '../../components/ui/badge';
import { Button } from '../../components/ui/button';
import { Spinner } from '../../components/ui/spinner';
import { TruncatedText } from '../../components/ui/tooltip';
import { taskActive, taskPhaseKeys, toolKey } from '../mcp/session-model';
import { navigateMcp } from '../mcp/model';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { McpFeedback } from '../mcp/McpFeedback';

export function AgentFileFeedback({ tasks, error }: { tasks: AgentTask[]; error: string | null }) {
  const { t } = useDesktop();
  if (!tasks.length && !error) return null;
  return (
    <div
      className="max-h-32 shrink-0 overflow-auto border-b px-3 py-2"
      aria-label={t('files_operations')}
    >
      <McpFeedback code={error} />
      <div role="status" className="flex flex-col gap-1 text-xs">
        {tasks.map((task) => (
          <div key={task.id} className="flex items-center gap-2">
            <Badge
              variant={
                task.phase === 'succeeded' ? 'success' : taskActive(task) ? 'secondary' : 'warning'
              }
            >
              {taskActive(task) ? <Spinner data-icon="inline-start" /> : null}
              {t(taskPhaseKeys[task.phase])}
            </Badge>
            <span className="shrink-0">{t(toolKey(task.tool))}</span>
            <TruncatedText className="min-w-0 flex-1 truncate">
              {task.targets.map((target) => target.path).join(' → ') +
                (task.error ? ` · ${t(userErrorKey(task.error))}` : '')}
            </TruncatedText>
            <Button
              variant="ghost"
              size="xs"
              onClick={() =>
                navigateMcp({ view: 'activity', contextId: task.contextId, taskId: task.id })
              }
            >
              {t('mcp_activity')}
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}
