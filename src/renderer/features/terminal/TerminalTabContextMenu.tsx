import { useState, type ReactNode } from 'react';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
} from '../../components/ui/context-menu';
import { useDesktop } from '../../app/DesktopProvider';
import { errorCode, useWorkspace } from '../connections/workspace-context';
import { terminalTabId } from './terminal-tab-model';

/** 菜单目标由标签 ID 固定，活动终端变化不改变操作对象。 */
export function TerminalTabContextMenu({
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
  const { store, state } = useWorkspace();
  const { t } = useDesktop();
  const [open, setOpen] = useState(false);
  const session = state.sessions.find((item) => item.id === sessionId);
  const locked = busy || !session || store.feedback.isBusy('session:' + sessionId);
  const current = () => store.getSnapshot().sessions.find((item) => item.id === sessionId);
  const copy = async () => {
    const target = current();
    if (!target) return;
    const host =
      target.host.includes(':') && !target.host.startsWith('[') ? `[${target.host}]` : target.host;
    try {
      await window.vigilshell.copyText(`${target.username}@${host}:${target.port}`);
    } catch (error) {
      store.report(errorCode(error), 'session:' + sessionId);
    }
  };
  const disconnect = async () => {
    if (busy || current()?.phase !== 'running' || store.feedback.isBusy('session:' + sessionId))
      return;
    try {
      await store.disconnectTerminal(sessionId);
    } catch (error) {
      store.report(errorCode(error), 'session:' + sessionId);
    }
  };
  return (
    <ContextMenu onOpenChange={setOpen}>
      <ContextMenuTrigger
        className="workspace-tab group/session-tab"
        data-active={active || undefined}
        style={{ width }}
        data-overflow-hidden={overflowHidden || undefined}
        aria-hidden={overflowHidden || undefined}
        inert={overflowHidden}
        data-context={open || undefined}
      >
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent
        finalFocus={() =>
          document.getElementById(terminalTabId(sessionId)) ??
          document.getElementById(terminalTabId(store.getSnapshot().activeTab)) ??
          false
        }
      >
        <ContextMenuGroup>
          <ContextMenuItem
            disabled={locked || !store.canReconnectTerminal(sessionId)}
            onClick={() => {
              if (!busy)
                void store
                  .reconnectTerminal(sessionId)
                  .catch((error) => store.report(errorCode(error), 'session:' + sessionId));
            }}
          >
            {t('ws_reconnect')}
          </ContextMenuItem>
          <ContextMenuItem disabled={!session} onClick={() => void copy()}>
            {t('ws_copy_connection_address')}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={locked || session?.phase !== 'running'}
            onClick={() => void disconnect()}
          >
            {t('ws_disconnect')}
          </ContextMenuItem>
        </ContextMenuGroup>
        <ContextMenuSeparator />
        <ContextMenuGroup>
          <ContextMenuItem
            variant="destructive"
            disabled={locked}
            onClick={() => {
              if (current() && !busy && !store.feedback.isBusy('session:' + sessionId))
                close(sessionId);
            }}
          >
            {t('ws_close_terminal')}
          </ContextMenuItem>
        </ContextMenuGroup>
      </ContextMenuContent>
    </ContextMenu>
  );
}
