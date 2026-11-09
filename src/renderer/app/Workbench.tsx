import { RuntimeFeedback } from './RuntimeFeedback';
import { AgentActivity } from '../features/mcp/AgentActivity';
import type { McpNavigation } from '../../contracts/mcp';
import { FloatingTransfers } from '../features/server-tools/FloatingTransfers';
import { AgentSessionPending } from '../features/mcp/AgentSessionPending';
import { IconButton } from '../components/ui/icon-button';
import { TerminalTabBar } from '../features/terminal/TerminalTabBar';
import { useNavigationInteractionHeld } from '../components/ui/navigation-interaction';
import { useWorkspace } from '../features/connections/workspace-context';
import { WorkspaceUi } from '../features/connections/workspace-ui';
import { FileWorkspace } from '../features/server-tools/FileWorkspace';
import { TransferWindows } from '../features/server-tools/TransferPanel';
import { ServerToolsPanel } from '../features/server-tools/ServerToolsPanel';
import { Popover, PopoverTrigger, PopoverContent } from '../components/ui/popover';
import {
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  PanelRightCloseIcon,
  PanelRightOpenIcon,
} from 'lucide-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import type { PanelImperativeHandle } from 'react-resizable-panels';
import { Button } from '../components/ui/button';

import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable';
import { SidebarProvider } from '../components/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip';
import { WorkbenchNavigation } from './WorkbenchNavigation';
import { useDesktop } from './DesktopProvider';

const previewAnchor = { getBoundingClientRect: () => new DOMRect(0, 0, 0, 0) };

export interface PanelLayout {
  collapsed: boolean;
  width: number;
}

export function Workbench({
  onSettings,
  active,
  activityTarget,
  policyContent,
  beforeConnectionAction,
  onReturnToTerminal,
}: {
  onSettings(): void;
  active: boolean;
  policyContent?: ReactNode;
  beforeConnectionAction?: () => Promise<boolean>;
  activityTarget?: Extract<McpNavigation, { view: 'activity' }>;
  onReturnToTerminal?(): void;
}) {
  const [lastActivity, setLastActivity] = useState(activityTarget);
  useEffect(() => {
    if (activityTarget) setLastActivity(activityTarget);
  }, [activityTarget]);
  const auxiliaryVisible = !!activityTarget || !!policyContent;
  const navigationHeld = useNavigationInteractionHeld();
  const { t, snapshot: desktopSnapshot } = useDesktop();
  const hoverPreviewEnabled = desktopSnapshot.preferences?.sidebarHoverPreview ?? true;
  const { state: workspaceState, store: workspace } = useWorkspace();
  const [layout, onLayoutChange] = useState<PanelLayout>({
    collapsed: false,
    width: 240,
  });
  const [toolsLayout, onToolsLayoutChange] = useState<PanelLayout>({
    collapsed: true,
    width: 320,
  });
  const lastToolsReveal = useRef(workspaceState.toolsReveal);
  useEffect(() => {
    if (lastToolsReveal.current === workspaceState.toolsReveal) return;
    lastToolsReveal.current = workspaceState.toolsReveal;
    toolsRef.current?.expand();
    onToolsLayoutChange((old) => ({ ...old, collapsed: false }));
  }, [workspaceState.toolsReveal]);
  const panelRef = useRef<PanelImperativeHandle | null>(null);
  const toolsRef = useRef<PanelImperativeHandle | null>(null);
  const activityVisible = useRef(auxiliaryVisible);
  useEffect(() => {
    activityVisible.current = auxiliaryVisible;
    if (auxiliaryVisible) toolsRef.current?.collapse();
    else if (!toolsLayout.collapsed) toolsRef.current?.resize(`${toolsLayout.width}px`);
  }, [auxiliaryVisible, toolsLayout.collapsed, toolsLayout.width]);
  const motionPanel = useRef<'sidebar' | 'tools' | null>(null);
  const [toolsContentWidth, setToolsContentWidth] = useState(toolsLayout.width);
  const panelsAnimating = useRef(false);
  const suppressPreview = useRef(false);
  const previewTrigger = useRef<HTMLButtonElement | null>(null);
  const [transferAnchor, setTransferAnchor] = useState<HTMLDivElement | null>(null);
  const previewPopup = useRef<HTMLDivElement | null>(null);
  const previousNavigationHeld = useRef(false);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewContainer, setPreviewContainer] = useState<HTMLDivElement | null>(null);
  const [panelMotion, setPanelMotion] = useState(false);
  useEffect(() => {
    const released = previousNavigationHeld.current && !navigationHeld;
    previousNavigationHeld.current = navigationHeld;
    // 被交互占用取消的 hover-close 不会补发；释放后检查指针位置。
    if (
      released &&
      previewOpen &&
      layout.collapsed &&
      !previewTrigger.current?.matches(':hover') &&
      !previewPopup.current?.matches(':hover')
    ) {
      setPreviewOpen(false);
    }
  }, [navigationHeld, previewOpen, layout.collapsed]);
  // defaultSize 变化会让面板库重新注册；拖拽期间必须保留本次挂载初值。
  const [initialSidebarSize] = useState(() => (layout.collapsed ? '0px' : `${layout.width}px`));
  const [initialToolsSize] = useState(() =>
    toolsLayout.collapsed ? '0px' : `${toolsLayout.width}px`,
  );
  useEffect(() => {
    if (!active || !hoverPreviewEnabled) setPreviewOpen(false);
  }, [active, hoverPreviewEnabled]);
  const toolsToggleLabel = toolsLayout.collapsed ? t('expand_inspector') : t('collapse_inspector');
  const toggleLabel = layout.collapsed ? t('expand_resources') : t('collapse_resources');

  function setSidebarOpen(open: boolean) {
    if (!panelRef.current || open === !layout.collapsed) return;
    const animate =
      !(open && previewOpen) && !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (motionPanel.current === 'tools') finishPanelMotion();
    motionPanel.current = animate ? 'sidebar' : null;
    suppressPreview.current = previewTrigger.current?.matches(':hover') ?? false;
    panelsAnimating.current = animate;
    // 先启用过渡，再调用面板库；动画中间帧不能覆盖用户保存的展开宽度。
    flushSync(() => {
      setPreviewOpen(false);
      setPanelMotion(animate);
      onLayoutChange({ ...layout, collapsed: !open });
    });
    if (open) panelRef.current?.resize(`${layout.width}px`);
    else panelRef.current?.collapse();
  }

  function finishPanelMotion() {
    if (!panelsAnimating.current) return;
    panelsAnimating.current = false;
    motionPanel.current = null;
    flushSync(() => setPanelMotion(false));
    requestAnimationFrame(() => window.dispatchEvent(new Event('vigilshell:fit-terminals')));
    const panel = panelRef.current;
    if (panel) {
      const collapsed = panel.isCollapsed();
      onLayoutChange({
        collapsed,
        width: collapsed ? layout.width : panel.getSize().inPixels,
      });
    }
    const tools = toolsRef.current;
    if (tools) {
      const collapsed = tools.isCollapsed();
      const width = collapsed ? toolsLayout.width : tools.getSize().inPixels;
      if (!collapsed) setToolsContentWidth(width);
      onToolsLayoutChange({ collapsed, width });
    }
  }

  function setToolsOpen(open: boolean) {
    if (!toolsRef.current || open === !toolsLayout.collapsed) return;
    if (motionPanel.current === 'sidebar') finishPanelMotion();
    const animate = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const width = Math.min(
      toolsLayout.width,
      window.innerWidth - (layout.collapsed ? 0 : 240) - 360 - 2,
    );
    panelsAnimating.current = animate;
    motionPanel.current = animate ? 'tools' : null;
    flushSync(() => {
      setPreviewOpen(false);
      setPanelMotion(animate);
      if (open) setToolsContentWidth(width);
      onToolsLayoutChange({ ...toolsLayout, collapsed: !open });
    });
    if (open) toolsRef.current.resize(`${width}px`);
    else toolsRef.current.collapse();
  }

  return (
    <WorkspaceUi
      active={active}
      beforeConnectionAction={beforeConnectionAction}
      commandsActive={active && !auxiliaryVisible}
      navigationWidth={layout.collapsed ? 0 : layout.width}
    >
      <SidebarProvider
        className="workbench-shell"
        data-testid="workbench"
        inert={!active}
        style={!active ? { visibility: 'hidden', position: 'absolute', inset: 0 } : undefined}
        open={!layout.collapsed}
        onOpenChange={setSidebarOpen}
      >
        <Popover
          open={active && hoverPreviewEnabled && previewOpen && layout.collapsed && !panelMotion}
          modal={false}
          onOpenChange={(open, details) => {
            if (
              details.reason === 'trigger-press' ||
              (!hoverPreviewEnabled && details.reason === 'trigger-hover') ||
              (!open && details.reason === 'trigger-hover' && navigationHeld) ||
              (open && (!layout.collapsed || panelsAnimating.current || suppressPreview.current))
            ) {
              details.cancel();
              return;
            }
            if (details.reason === 'escape-key' || details.reason === 'outside-press') {
              suppressPreview.current = previewTrigger.current?.matches(':hover') ?? false;
            }
            setPreviewOpen(open);
          }}
        >
          <ResizablePanelGroup
            orientation="horizontal"
            className="shell-panels"
            data-panel-motion={panelMotion || undefined}
          >
            <ResizablePanel
              id="sidebar"
              panelRef={panelRef}
              defaultSize={initialSidebarSize}
              minSize="240px"
              maxSize="420px"
              groupResizeBehavior="preserve-pixel-size"
              collapsible
              collapsedSize="0px"
              style={{ overflow: 'hidden' }}
              onTransitionEnd={(event) => {
                if (
                  motionPanel.current === 'sidebar' &&
                  event.target === event.currentTarget &&
                  event.propertyName === 'flex-grow'
                ) {
                  finishPanelMotion();
                }
              }}
              onResize={(size) => {
                if (panelsAnimating.current) return;
                onLayoutChange({
                  collapsed: size.inPixels === 0,
                  width: size.inPixels > 0 ? size.inPixels : layout.width,
                });
              }}
            >
              <aside
                id="workbench-navigation"
                className="shell-sidebar"
                style={!active ? { display: 'none' } : undefined}
                aria-label={t('navigation')}
                inert={layout.collapsed}
              >
                <div className="native-titlebar" aria-hidden="true" />
                <WorkbenchNavigation
                  onSettings={onSettings}
                  onConnected={() => {
                    setPreviewOpen(false);
                    onReturnToTerminal?.();
                  }}
                  activityActive={!!activityTarget}
                  policyActive={!!policyContent}
                />
              </aside>
            </ResizablePanel>
            <ResizableHandle
              key={layout.collapsed ? 'sidebar-hidden' : 'sidebar-visible'}
              className="shell-resize-handle"
              appearance="sidebar"
              aria-label={t('resize_resources')}
              disabled={layout.collapsed}
              onPointerDownCapture={finishPanelMotion}
              onKeyDownCapture={finishPanelMotion}
            />
            <ResizablePanel id="workspace" minSize="360px">
              <main className="workbench-main">
                <header
                  className="workspace-titlebar"
                  data-sidebar-collapsed={layout.collapsed}
                  data-tools-collapsed={toolsLayout.collapsed}
                >
                  {!auxiliaryVisible ? <TerminalTabBar /> : null}
                  <div className="terminal-titlebar-drag" aria-hidden="true" />
                </header>
                <div className="workbench-body">
                  <RuntimeFeedback settingsOpen={!active} />
                  <div className="min-h-0 flex-1">
                    {policyContent}
                    {lastActivity ? (
                      <div className="h-full" hidden={!activityTarget}>
                        <AgentActivity target={lastActivity} onBack={onReturnToTerminal} />
                      </div>
                    ) : null}
                    <div
                      className="h-full"
                      hidden={auxiliaryVisible || workspaceState.activeTab.startsWith('agent:')}
                    >
                      <FileWorkspace
                        active={
                          active &&
                          !auxiliaryVisible &&
                          !workspaceState.activeTab.startsWith('agent:')
                        }
                        transfersOpen={
                          !toolsLayout.collapsed && workspaceState.toolsTab === 'transfers'
                        }
                        onToggleTransfers={() => {
                          if (!toolsLayout.collapsed && workspaceState.toolsTab === 'transfers')
                            setToolsOpen(false);
                          else workspace.selectToolsTab('transfers', true);
                        }}
                      />
                    </div>
                    {workspaceState.agentViews
                      .filter((view) => !workspace.agentTerminal(view.context.id))
                      .map((view) => (
                        <div
                          key={view.context.id}
                          className="h-full"
                          hidden={
                            auxiliaryVisible ||
                            workspaceState.activeTab !== `agent:${view.context.id}`
                          }
                        >
                          <AgentSessionPending
                            view={view}
                            visible={
                              active &&
                              !auxiliaryVisible &&
                              workspaceState.activeTab === `agent:${view.context.id}`
                            }
                          />
                        </div>
                      ))}
                  </div>
                </div>
              </main>
            </ResizablePanel>
            {/* 启用分隔线时重新注册，刷新面板库缓存的相邻面板映射。 */}
            <ResizableHandle
              key={toolsLayout.collapsed ? 'tools-hidden' : 'tools-visible'}
              className="tools-resize-handle"
              aria-label={t('resize_inspector')}
              disabled={toolsLayout.collapsed}
              onPointerDownCapture={finishPanelMotion}
              onKeyDownCapture={finishPanelMotion}
            />
            <ResizablePanel
              id="tools"
              panelRef={toolsRef}
              defaultSize={initialToolsSize}
              minSize="320px"
              maxSize="480px"
              groupResizeBehavior="preserve-pixel-size"
              collapsible
              collapsedSize="0px"
              style={{ overflow: 'hidden' }}
              onTransitionEnd={(event) => {
                if (
                  motionPanel.current === 'tools' &&
                  event.target === event.currentTarget &&
                  event.propertyName === 'flex-grow'
                ) {
                  finishPanelMotion();
                }
              }}
              onResize={(size) => {
                if (panelsAnimating.current || activityVisible.current) return;
                if (size.inPixels > 0) setToolsContentWidth(size.inPixels);
                onToolsLayoutChange({
                  collapsed: size.inPixels === 0,
                  width: size.inPixels > 0 ? size.inPixels : toolsLayout.width,
                });
              }}
            >
              <aside
                id="workbench-tools"
                className="tools-panel"
                style={{ width: toolsContentWidth }}
                aria-label={t('inspector')}
                inert={toolsLayout.collapsed}
                data-collapsed={toolsLayout.collapsed}
              >
                <div className="native-titlebar" aria-hidden="true" />
                <ServerToolsPanel visible={active && !auxiliaryVisible && !toolsLayout.collapsed} />
              </aside>
            </ResizablePanel>
          </ResizablePanelGroup>
          <TransferWindows />
          <div ref={setPreviewContainer} />
          <PopoverContent
            appearance="navigation"
            portalContainer={previewContainer}
            positionerProps={{
              anchor: previewAnchor,
              positionMethod: 'fixed',
              side: 'bottom',
              align: 'start',
              sideOffset: 0,
              collisionAvoidance: { side: 'none', align: 'none' },
            }}
            ref={previewPopup}
            id="workbench-navigation-preview"
            style={{ width: layout.width, display: active ? undefined : 'none' }}
            data-pinned={!layout.collapsed || undefined}
            aria-label={t('navigation')}
            initialFocus={false}
            finalFocus={false}
          >
            <div className="native-titlebar" aria-hidden="true" />
            <WorkbenchNavigation
              onSettings={onSettings}
              onConnected={() => {
                setPreviewOpen(false);
                onReturnToTerminal?.();
              }}
              activityActive={!!activityTarget}
              policyActive={!!policyContent}
            />
          </PopoverContent>
          {/* 放在拖动区域之后，让 Electron 最后应用按钮的 no-drag 排除区域。 */}
          <div
            className="window-toolbar window-toolbar-left"
            style={!active ? { display: 'none' } : undefined}
          >
            <Tooltip>
              <PopoverTrigger
                ref={previewTrigger}
                openOnHover={layout.collapsed && hoverPreviewEnabled}
                delay={100}
                closeDelay={150}
                onPointerLeave={() => {
                  suppressPreview.current = false;
                }}
                onClick={() => setSidebarOpen(layout.collapsed)}
                aria-expanded={!layout.collapsed || previewOpen}
                aria-controls={
                  previewOpen ? 'workbench-navigation-preview' : 'workbench-navigation'
                }
                render={
                  <TooltipTrigger
                    render={
                      <Button
                        variant="titlebar"
                        size="icon-sm"
                        aria-label={toggleLabel}
                        data-testid="toggle-left"
                      />
                    }
                  />
                }
              >
                {layout.collapsed ? <PanelLeftOpenIcon /> : <PanelLeftCloseIcon />}
              </PopoverTrigger>
              <TooltipContent>{toggleLabel}</TooltipContent>
            </Tooltip>
          </div>
          <div
            ref={setTransferAnchor}
            className="window-toolbar window-toolbar-right"
            style={!active ? { display: 'none' } : undefined}
          >
            <FloatingTransfers
              active={active && !auxiliaryVisible}
              sidebarVisible={!toolsLayout.collapsed}
              anchor={transferAnchor}
            />
            {!auxiliaryVisible ? (
              <IconButton
                variant="titlebar"
                size="icon-sm"
                data-testid="toggle-right"
                aria-expanded={!toolsLayout.collapsed}
                aria-controls="workbench-tools"
                onClick={() => setToolsOpen(toolsLayout.collapsed)}
                label={toolsToggleLabel}
                tooltip={<>{toolsToggleLabel}</>}
              >
                {toolsLayout.collapsed ? <PanelRightOpenIcon /> : <PanelRightCloseIcon />}
              </IconButton>
            ) : null}
          </div>
        </Popover>
      </SidebarProvider>
    </WorkspaceUi>
  );
}
