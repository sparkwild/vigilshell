import type { ReactNode } from 'react';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
} from '../../components/ui/context-menu';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace } from '../connections/workspace-context';
import { terminalTabId } from './terminal-tab-model';
export function AgentTabContextMenu({
  sessionId,
  active,
  overflowHidden = false,
  width,
  busy,
  close,
  children,
}: {
  sessionId: string;
  active: boolean;
  overflowHidden?: boolean;
  width?: number;
  busy: boolean;
  close(id: string): void;
  children: ReactNode;
}) {
  const { t } = useDesktop(),
    { store, state } = useWorkspace();
  const view = state.agentViews.find((view) => `agent:${view.context.id}` === sessionId);
  return (
    <ContextMenu>
      <ContextMenuTrigger
        className="workspace-tab group/session-tab"
        data-active={active || undefined}
        style={{ width }}
        data-overflow-hidden={overflowHidden || undefined}
        aria-hidden={overflowHidden || undefined}
        inert={overflowHidden}
      >
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent
        finalFocus={() => document.getElementById(terminalTabId(sessionId)) ?? false}
      >
        <ContextMenuGroup>
          <ContextMenuItem disabled={busy} onClick={() => close(sessionId)}>
            {t('agent_view_hide')}
          </ContextMenuItem>
          <ContextMenuItem
            variant="destructive"
            disabled={
              busy ||
              !view ||
              ['closing', 'closed', 'disconnected', 'failed'].includes(view.context.phase)
            }
            onClick={() => {
              if (!view) return;
              void window.vigilshell.mcp
                .request({ action: 'closeSession', sessionId: view.context.id })
                .then((result) => {
                  if (!result.ok) store.report(result.code);
                });
            }}
          >
            {t('agent_view_end')}
          </ContextMenuItem>
        </ContextMenuGroup>
      </ContextMenuContent>
    </ContextMenu>
  );
}
