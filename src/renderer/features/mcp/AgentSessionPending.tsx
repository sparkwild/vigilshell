import type { AgentSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Spinner } from '../../components/ui/spinner';
import { ControlledAgentLogo } from '../terminal/AgentControlNotice';
import { TerminalFloatingNotice, TerminalNoticeOverlay } from '../terminal/TerminalFloatingNotice';
import { terminalPanelId, terminalTabId } from '../terminal/terminal-tab-model';
import { tabLabel } from '../terminal/workbench-tab-model';
import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { McpFeedback } from './McpFeedback';
import { useMcp } from './model';

/** 等待授权或终端建立时使用同一浮窗；就绪后由普通终端接替，不创建旧工作台。 */
export function AgentSessionPending({
  view,
  visible,
}: {
  view: AgentSessionView;
  visible: boolean;
}) {
  const { t } = useDesktop();
  const model = useMcp();
  const id = `agent:${view.context.id}`;
  const pending = ['waitingUser', 'connecting', 'ready'].includes(view.context.phase);
  const notice = useNotificationSurface(visible ? `agentContext:${view.context.id}` : null);
  return (
    <section
      className="relative h-full"
      id={terminalPanelId(id)}
      aria-labelledby={terminalTabId(id)}
    >
      <TerminalNoticeOverlay hidden={!visible}>
        <TerminalFloatingNotice
          ref={notice}
          role="status"
          actions={
            pending ? (
              <>
                {view.context.phase === 'waitingUser' ? (
                  <Button
                    size="xs"
                    variant="outline"
                    disabled={model.busy}
                    onClick={() =>
                      void model.issue({ action: 'approveConnection', contextId: view.context.id })
                    }
                  >
                    {t('agent_verify_apply')}
                  </Button>
                ) : null}
                <Button
                  size="xs"
                  variant="outline"
                  disabled={model.busy}
                  onClick={() =>
                    void model.issue({ action: 'closeSession', sessionId: view.context.id })
                  }
                >
                  {t('agent_control_release')}
                </Button>
              </>
            ) : null
          }
          detail={<McpFeedback code={view.context.error ?? model.error} />}
        >
          <ControlledAgentLogo entryId={view.context.entryId} />
          <span>
            {view.context.entryName} · {t(tabLabel({ kind: 'agent', id, view }, [], 1).phase)}
          </span>
          {pending ? <Spinner /> : null}
        </TerminalFloatingNotice>
      </TerminalNoticeOverlay>
    </section>
  );
}
