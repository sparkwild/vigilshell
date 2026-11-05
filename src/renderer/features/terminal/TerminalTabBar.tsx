import { ControlledAgentLogo } from './AgentControlNotice';
import { useTerminalTabLayout } from './use-terminal-tab-layout';
import { useRef, useState } from 'react';
import { ListIcon, LoaderCircleIcon, TerminalIcon, TriangleAlertIcon, BotIcon } from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '../../components/ui/tabs';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../components/ui/tooltip';
import { Popover, PopoverTrigger } from '../../components/ui/popover';
import { AgentTabContextMenu } from './AgentTabContextMenu';
import { tabLabel } from './workbench-tab-model';
import { TerminalTabContextMenu } from './TerminalTabContextMenu';
import { useConfirmation } from '../../shared/ConfirmationProvider';
import { TerminalCloseButton } from './TerminalCloseButton';
import { TerminalSessionsPopover } from './TerminalSessionsPopover';
import { EnvironmentBadge, environmentKey } from '../connections/EnvironmentBadge';
import { OperationButtonIcon } from '../connections/OperationFeedback';
import { errorCode, useWorkspace } from '../connections/workspace-context';
import { terminalFocusTarget, terminalPanelId, terminalTabId } from './terminal-tab-model';

export function TerminalTabBar() {
  const { store, state } = useWorkspace();
  const { t } = useDesktop();
  const confirm = useConfirmation();
  const [closingAll, setClosingAll] = useState(false);
  const closeAllPending = useRef(false);
  const [closeErrors, setCloseErrors] = useState<Record<string, string>>({});
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const pendingFocus = useRef<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const labels = store
    .tabs()
    .map((tab) => tabLabel(tab, state.connections, store.terminalOrdinal(tab.id)));
  const geometry = JSON.stringify(
    labels.map(({ id, label, environment }) => [
      id,
      label,
      environment === 'none' ? '' : t(environmentKey(environment)),
    ]),
  );
  const { viewport, list, widths, natural, hiddenCount } = useTerminalTabLayout(
    labels.map(({ id }) => id),
    state.activeTab,
    geometry,
  );
  const activate = (id: string) => {
    if (!store.tabs().some((tab) => tab.id === id)) return;
    store.tab(id);
    if (!menuOpen) store.terminals.get(id)?.focus();
  };
  const closeOne = async (id: string) => {
    setCloseErrors((previous) => {
      const next = { ...previous };
      delete next[id];
      return next;
    });
    try {
      await store.closeView(id);
    } catch (error) {
      const code = errorCode(error);
      setCloseErrors((previous) => ({ ...previous, [id]: code }));
      store.report(code, 'session:' + id);
    }
  };
  const closeAll = async () => {
    if (closeAllPending.current) return;
    const tabs = store.tabs();
    const ids = tabs.filter((tab) => tab.kind === 'terminal').map((tab) => tab.id);
    const agents = tabs.filter((tab) => tab.kind === 'agent');
    if (!tabs.length) return;
    closeAllPending.current = true;
    setMenuOpen(false);
    setClosingAll(true);
    try {
      const accepted =
        !ids.length ||
        (await confirm({
          title: t('ws_close_all_terminals_title'),
          description: agents.length
            ? t('agent_close_all_hint', { human: ids.length, agent: agents.length })
            : t('ws_close_all_terminals_description', { count: ids.length }),
          confirmLabel: t('ws_close_all_terminals'),
          cancelLabel: t('ws_cancel'),
          tone: 'danger',
        }));
      if (!accepted) return;
      const result = ids.length
        ? await store.closeTerminals(ids)
        : { failed: [], cancelled: false };
      if (!result.cancelled) for (const agent of agents) await store.closeView(agent.id);
      setCloseErrors(Object.fromEntries(result.failed.map(({ id, code }) => [id, code])));
      if (result.failed.length || result.cancelled) setMenuOpen(true);
    } finally {
      closeAllPending.current = false;
      setClosingAll(false);
    }
  };
  if (!labels.length) return null;
  const controlsVisible = hiddenCount > 0;
  return (
    <div className="terminal-tab-navigation">
      <div
        className="terminal-tab-viewport"
        ref={viewport}
        style={{
          width:
            [...widths.values()].reduce((total, width) => total + width, 0) +
            Math.max(0, widths.size - 1) * 4,
        }}
      >
        <Tabs
          appearance="workspace"
          value={state.activeTab}
          onValueChange={(value) => activate(String(value))}
        >
          <TabsList
            ref={list}
            variant="session"
            activateOnFocus={false}
            aria-label={t('ws_terminal_tabs')}
            onKeyDownCapture={(event) => {
              if (
                event.nativeEvent.isComposing ||
                event.altKey ||
                event.ctrlKey ||
                event.metaKey ||
                !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)
              )
                return;
              const target = (event.target as Element).closest<HTMLElement>('[role="tab"]');
              if (!target) return;
              // Base UI 允许聚焦 disabled 标签；收起项必须从方向键导航中排除。
              const tabs = Array.from(
                event.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]'),
              ).filter((tab) => widths.has(tab.dataset.sessionId!));
              const index = tabs.indexOf(target);
              if (index < 0) return;
              event.preventDefault();
              event.stopPropagation();
              const next =
                event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? tabs.length - 1
                    : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
              tabs[next]?.focus();
            }}
          >
            {labels.map(
              ({
                tab,
                id,
                label,
                name,
                ordinal,
                environment,
                phase,
                description,
                establishing,
                failed,
              }) => {
                const overflowHidden = !widths.has(id);
                const Icon = establishing
                  ? LoaderCircleIcon
                  : failed
                    ? TriangleAlertIcon
                    : tab.kind === 'agent'
                      ? BotIcon
                      : TerminalIcon;
                const Menu = tab.kind === 'agent' ? AgentTabContextMenu : TerminalTabContextMenu;
                return (
                  <Menu
                    key={id}
                    sessionId={id}
                    active={state.activeTab === id}
                    overflowHidden={overflowHidden}
                    width={widths.get(id) ?? Math.min(220, natural.get(id) ?? 160)}
                    busy={closingAll}
                    close={(id) => void closeOne(id)}
                  >
                    <Tooltip>
                      <TabsTrigger
                        id={terminalTabId(id)}
                        value={id}
                        data-session-id={id}
                        disabled={overflowHidden}
                        aria-controls={terminalPanelId(id)}
                        aria-label={`${label} — ${t(phase)}`}
                        render={<TooltipTrigger />}
                        onClick={() => {
                          if (state.activeTab === id) activate(id);
                        }}
                      >
                        <OperationButtonIcon owner={'session:' + id} action="ws_close_terminal">
                          {tab.kind === 'agent' ? (
                            <ControlledAgentLogo entryId={tab.view.context.entryId} />
                          ) : tab.session.agentControl ? (
                            <ControlledAgentLogo entryId={tab.session.agentControl.entryId} />
                          ) : (
                            <Icon
                              data-icon="inline-start"
                              className={
                                establishing ? 'animate-spin motion-reduce:animate-none' : undefined
                              }
                              data-session-phase={tab.session.phase}
                            />
                          )}
                        </OperationButtonIcon>
                        <span className="terminal-tab-caption">
                          <span className="terminal-tab-label truncate">{name}</span>
                          {ordinal > 1 && (
                            <span className="terminal-tab-ordinal">{` · ${ordinal}`}</span>
                          )}
                        </span>
                        <EnvironmentBadge environment={environment} compact />
                        <span className="terminal-tab-label-measure" aria-hidden="true">
                          {label}
                        </span>
                      </TabsTrigger>
                      <TooltipContent side="bottom">
                        <span>
                          {label} · {t('ws_terminal_number', { number: ordinal })}
                          <br />
                          {description}
                          <br />
                          {t(phase)}
                        </span>
                      </TooltipContent>
                    </Tooltip>
                    <TerminalCloseButton
                      className="terminal-tab-close"
                      label={label}
                      actionLabel={tab.kind === 'agent' ? t('agent_view_hide') : undefined}
                      disabled={closingAll || store.feedback.isBusy('session:' + id)}
                      onClick={() => void closeOne(id)}
                    />
                  </Menu>
                );
              },
            )}
          </TabsList>
        </Tabs>
      </div>
      <div
        className="terminal-overflow-controls"
        aria-hidden={!controlsVisible}
        inert={!controlsVisible}
        data-visible={controlsVisible}
      >
        <Popover
          open={menuOpen}
          onOpenChange={(open) => {
            if (open) pendingFocus.current = null;
            setMenuOpen(open);
          }}
        >
          <Tooltip>
            <PopoverTrigger
              render={
                <TooltipTrigger
                  render={
                    <Button
                      ref={menuTrigger}
                      variant="titlebar"
                      size="sm"
                      className="terminal-overflow-button"
                      aria-label={t('ws_all_terminals')}
                      aria-description={t('ws_hidden_terminals', {
                        count: hiddenCount,
                      })}
                    />
                  }
                />
              }
            >
              <ListIcon />
              <span className="terminal-overflow-count" aria-hidden="true">
                {hiddenCount}
              </span>
            </PopoverTrigger>
            <TooltipContent side="bottom">
              {t('ws_all_terminals')} · {t('ws_hidden_terminals', { count: hiddenCount })}
            </TooltipContent>
          </Tooltip>
          <TerminalSessionsPopover
            labels={labels}
            activeId={state.activeTab}
            busy={closingAll}
            errors={closeErrors}
            isClosing={(id) => store.feedback.isBusy('session:' + id)}
            select={(id) => {
              pendingFocus.current = id;
              activate(id);
              setMenuOpen(false);
            }}
            close={(id) => void closeOne(id)}
            closeAll={() => void closeAll()}
            finalFocus={() => {
              if (closeAllPending.current) return false;
              const id = pendingFocus.current;
              pendingFocus.current = null;
              return id
                ? (terminalFocusTarget(id) ??
                    (controlsVisible
                      ? menuTrigger.current
                      : document.getElementById(terminalTabId(id))) ??
                    false)
                : controlsVisible
                  ? menuTrigger.current
                  : document.getElementById(terminalTabId(store.getSnapshot().activeTab));
            }}
          />
        </Popover>
      </div>
    </div>
  );
}
