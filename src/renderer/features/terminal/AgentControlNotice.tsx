import { useState } from 'react';
import { useAgentWorkspaceReview } from '../mcp/AgentWorkspaceReview';
import type { WorkspaceAgentControl } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { TerminalFloatingNotice } from './TerminalFloatingNotice';
import { TruncatedText } from '../../components/ui/tooltip';
import { Button } from '../../components/ui/button';
import { Spinner } from '../../components/ui/spinner';
import { AgentLogo } from '../mcp/McpClientPicker';
import { useMcp } from '../mcp/model';
import { McpFeedback } from '../mcp/McpFeedback';

export function ControlledAgentLogo({ entryId }: { entryId: string }) {
  const { snapshot } = useMcp();
  return (
    <AgentLogo size="sm" client={snapshot?.entries.find((entry) => entry.id === entryId)?.client} />
  );
}

export function AgentControlNotice({ control }: { control: WorkspaceAgentControl }) {
  const { t } = useDesktop();
  const review = useAgentWorkspaceReview({ sessionId: control.contextId });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const release = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await window.vigilshell.mcp.request({
        action: 'closeSession',
        sessionId: control.contextId,
      });
      if (!result.ok) setError(result.code);
    } catch {
      setError('runtime_unavailable');
    } finally {
      setBusy(false);
    }
  };
  return (
    <TerminalFloatingNotice
      role="status"
      actions={
        <Button size="xs" variant="outline" disabled={busy} onClick={() => void release()}>
          {busy ? <Spinner data-icon="inline-start" /> : null}
          {t('agent_control_release')}
        </Button>
      }
      detail={
        error || review ? (
          <>
            <McpFeedback code={error} />
            {review}
          </>
        ) : null
      }
    >
      <ControlledAgentLogo entryId={control.entryId} />
      <TruncatedText className="terminal-notice-label truncate">
        {t(control.releasing ? 'agent_control_releasing' : 'agent_control_operating', {
          name: control.entryName,
        })}
      </TruncatedText>
    </TerminalFloatingNotice>
  );
}
