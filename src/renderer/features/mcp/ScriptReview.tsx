import { quietAuthorizationFailure } from './authorization-feedback';
import { useState } from 'react';
import type { AgentTask } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { FieldDescription, FieldGroup } from '../../components/ui/field';
import { McpFeedback } from './McpFeedback';
export function ScriptReview({ task }: { task: AgentTask }) {
  const { t } = useDesktop();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (task.phase !== 'waitingReview' || !task.scriptSnapshot) return null;
  const decide = async (action: 'approveScript' | 'rejectScript') => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await window.vigilshell.mcp.request({ action, taskId: task.id });
      if (!result.ok && !quietAuthorizationFailure({ action, taskId: task.id }, result.code))
        setError(result.code);
    } catch {
      setError('runtime_unavailable');
    } finally {
      setBusy(false);
    }
  };
  return (
    <FieldGroup aria-label={t('agent_script_review_title')}>
      <h3 className="section-title">{t('agent_script_review_title')}</h3>
      <FieldDescription>{t('agent_script_review_hint')}</FieldDescription>
      <FieldDescription>
        {task.scriptSnapshot.path} · {task.scriptSnapshot.directory}
      </FieldDescription>
      {task.scriptSnapshot.arguments.length > 0 ? (
        <pre className="whitespace-pre-wrap break-all">
          {JSON.stringify(task.scriptSnapshot.arguments)}
        </pre>
      ) : null}
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all">
        {task.scriptSnapshot.text}
      </pre>
      <McpFeedback code={error} />
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={() => void decide('approveScript')}>
          {busy ? t('agent_auth_wait') : t('agent_script_allow')}
        </Button>
        <Button disabled={busy} variant="outline" onClick={() => void decide('rejectScript')}>
          {t('agent_script_reject')}
        </Button>
      </div>
    </FieldGroup>
  );
}
