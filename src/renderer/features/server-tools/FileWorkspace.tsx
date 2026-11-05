import { TerminalNoticeOverlay } from '../terminal/TerminalFloatingNotice';
import { AgentControlNotice } from '../terminal/AgentControlNotice';
import { IconButton } from '../../components/ui/icon-button';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../components/ui/tooltip';
import type { PanelImperativeHandle } from 'react-resizable-panels';
import { useEffect, useId, useRef, useState } from 'react';
import {
  FolderOpenIcon,
  ArrowDownUpIcon,
  ChevronDownIcon,
  MaximizeIcon,
  MinimizeIcon,
} from 'lucide-react';
import { TerminalWorkspace } from '../terminal/terminal-workspace';
import { useWorkspace } from '../connections/workspace-context';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '../../components/ui/resizable';
import { directories } from './directory-service';
import { FileBrowser } from './file-browser';
import { transferActive } from './file-format';
import { useAgentFileTasks } from './model/use-agent-file-tasks';

function FileWorkspaceToggleIcon({ expanded }: { expanded: boolean }) {
  return (
    <ChevronDownIcon
      className="file-workspace-chevron"
      data-expanded={expanded}
      aria-hidden="true"
    />
  );
}

export function FileWorkspace({
  active,
  transfersOpen = false,
  onToggleTransfers,
}: {
  active: boolean;
  transfersOpen?: boolean;
  onToggleTransfers?: () => void;
}) {
  const { store, state } = useWorkspace(),
    { t, snapshot: desktopSnapshot } = useDesktop();
  useEffect(
    () =>
      directories(store).prune(
        new Set([
          ...state.sessions
            .filter((s) => s.phase === 'running' || s.phase === 'connecting')
            .map((s) => s.groupId),
          ...[...state.transfers.values()]
            .filter((t) => transferActive(t.phase))
            .map((t) => t.groupId),
        ]),
      ),
    [store, state.sessions, state.transfers],
  );
  const session = state.sessions.find((s) => s.id === state.activeTab);
  const agentFiles = useAgentFileTasks(session?.agentControl?.contextId, state.runtimeId);
  const latestFileTask =
    [...agentFiles.tasks].sort((a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id))[0]
      ?.id ?? agentFiles.error;
  const [opened, setOpened] = useState(false),
    [initialized, setInitialized] = useState(false),
    [maximized, setMaximized] = useState(false),
    [height, setHeight] = useState(700);
  const initialRatio = 35;
  const terminalPanel = useRef<PanelImperativeHandle | null>(null),
    filePanel = useRef<PanelImperativeHandle | null>(null);
  const root = useRef<HTMLDivElement>(null),
    ratio = useRef(initialRatio);
  const toggle = useRef<HTMLButtonElement | null>(null);
  const focusToggle = useRef(false);
  const fileAreaId = useId();
  useEffect(() => {
    if (!initialized && session?.phase === 'running') {
      setInitialized(true);
      setOpened(desktopSnapshot.preferences?.filesAutoOpen ?? true);
    }
  }, [initialized, session?.phase, desktopSnapshot.preferences?.filesAutoOpen]);
  useEffect(() => {
    if (!latestFileTask) return;
    setOpened(true);
    if (height < 372) setMaximized(true);
  }, [latestFileTask, height]);
  useEffect(() => {
    if (!root.current) return;
    const ro = new ResizeObserver(([e]) => setHeight(e!.contentRect.height));
    ro.observe(root.current);
    return () => ro.disconnect();
  }, []);
  const show = !!session && opened && (height >= 372 || maximized);
  useEffect(() => {
    if (focusToggle.current) {
      toggle.current?.focus({ preventScroll: true });
      focusToggle.current = false;
    }
  }, [show]);
  const tasks = [...state.transfers.values()].filter(
    (task) => task.connectionId === session?.connectionId && transferActive(task.phase),
  );
  useEffect(() => {
    if (!show) return;
    const frame = requestAnimationFrame(() => {
      if (maximized) terminalPanel.current?.collapse();
      else if (terminalPanel.current?.isCollapsed()) {
        terminalPanel.current.expand();
        filePanel.current?.resize(`${ratio.current}%`);
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [show, maximized]);
  useEffect(() => {
    for (const terminal of store.terminals.values()) terminal.fit();
  }, [show, maximized, store, session?.agentControl]);
  return (
    <div className="file-workspace" ref={root}>
      {session?.agentControl ? (
        <TerminalNoticeOverlay hidden={!active}>
          <AgentControlNotice key={session.agentControl.leaseId} control={session.agentControl} />
        </TerminalNoticeOverlay>
      ) : null}
      <ResizablePanelGroup orientation="vertical">
        <ResizablePanel
          id="terminal-area"
          panelRef={terminalPanel}
          collapsible
          collapsedSize="0px"
          defaultSize={`${100 - initialRatio}%`}
          minSize={show ? '180px' : '0px'}
          className={show && maximized ? 'file-terminal-hidden' : ''}
        >
          <TerminalWorkspace visible={active && !(show && maximized)} />
        </ResizablePanel>
        {show ? <ResizableHandle aria-label={t('files_resize')} /> : null}
        {show ? (
          <ResizablePanel
            id="file-area"
            panelRef={filePanel}
            defaultSize={`${initialRatio}%`}
            minSize="160px"
            onResize={(size) => {
              if (!maximized) ratio.current = size.asPercentage;
            }}
          >
            <div className="file-pane file-workspace-reveal" id={fileAreaId}>
              <FileBrowser
                key={`${state.runtimeId}:${session.groupId}`}
                session={session}
                online={state.transportReady && session.phase === 'running'}
                visible={active}
                agentTasks={agentFiles.tasks}
                agentError={agentFiles.error}
                panelActions={
                  <>
                    <IconButton
                      variant="ghost"
                      size="icon-xs"
                      onClick={() => setMaximized((v) => !v)}
                      label={t(maximized ? 'files_restore' : 'files_maximize')}
                      tooltip={<>{t(maximized ? 'files_restore' : 'files_maximize')}</>}
                    >
                      {maximized ? <MinimizeIcon /> : <MaximizeIcon />}
                    </IconButton>
                    <IconButton
                      ref={toggle}
                      variant="ghost"
                      size="icon-xs"
                      className="file-workspace-toggle"
                      aria-expanded={true}
                      aria-controls={fileAreaId}
                      onClick={() => {
                        focusToggle.current = true;
                        setOpened(false);
                        setMaximized(false);
                      }}
                      label={t('files_collapse')}
                      tooltip={<>{t('files_collapse')}</>}
                    >
                      <FileWorkspaceToggleIcon expanded />
                    </IconButton>
                  </>
                }
              />
              {tasks.length > 0 && (
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <Button
                        variant="panel-toggle"
                        size="strip"
                        className="shrink-0"
                        data-active={transfersOpen || undefined}
                        aria-expanded={transfersOpen}
                        aria-controls="workbench-tools"
                        onClick={() =>
                          onToggleTransfers
                            ? onToggleTransfers()
                            : store.selectToolsTab('transfers', true)
                        }
                      />
                    }
                  >
                    <ArrowDownUpIcon data-icon="inline-start" />
                    <span>
                      {t(
                        tasks.some((task) => task.phase === 'running')
                          ? 'tools_transfer_running'
                          : 'files_transfer_tasks',
                      )}{' '}
                      · {tasks.length}
                    </span>
                    {tasks.some((task) => task.phase === 'awaitingDecision') && (
                      <span>· {t('tools_transfer_awaitingDecision')}</span>
                    )}
                  </TooltipTrigger>
                  <TooltipContent>
                    {t(transfersOpen ? 'files_hide_transfers' : 'files_show_transfers')}
                  </TooltipContent>
                </Tooltip>
              )}
            </div>
          </ResizablePanel>
        ) : null}
      </ResizablePanelGroup>
      {!show && session ? (
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                ref={toggle}
                variant="ghost"
                size="strip"
                className="file-workspace-toggle file-workspace-reveal shrink-0"
                aria-label={t('files_expand')}
                aria-expanded={false}
                aria-controls={fileAreaId}
                onClick={() => {
                  focusToggle.current = true;
                  setOpened(true);
                  if (height < 372) setMaximized(true);
                }}
              />
            }
          >
            <FolderOpenIcon data-icon="inline-start" />
            <span className="min-w-0 flex-1 text-left">{t('tools_files')}</span>
            <FileWorkspaceToggleIcon expanded={false} />
          </TooltipTrigger>
          <TooltipContent>{t('files_expand')}</TooltipContent>
        </Tooltip>
      ) : null}
    </div>
  );
}
