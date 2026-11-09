import { TerminalNoticeOverlay } from './TerminalFloatingNotice';
import { terminalPanelId, terminalTabId } from './terminal-tab-model';
import { useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from 'react';
import { ArrowDownIcon, ArrowUpIcon, SearchIcon } from 'lucide-react';
import type { TerminalSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { CloseButton } from '../../components/ui/icon-button';
import { Input } from '../../components/ui/input';
import { Toggle } from '../../components/ui/toggle';
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
} from '../../components/ui/context-menu';
import { TerminalConnectionInfo } from './TerminalConnectionInfo';
import { useWorkspace, errorCode } from '../connections/workspace-context';
import { useWorkspaceUi } from '../connections/ui-context';
import { ConnectionOverview } from '../connections/catalogue-view';
import { TerminalController } from './terminal-controller';
export function TerminalWorkspace({ visible }: { visible: boolean }) {
  const { store, state } = useWorkspace();
  useEffect(() => {
    const receive = (event: MessageEvent<unknown>) => {
      if (
        event.source !== window ||
        event.ports.length !== 1 ||
        !event.data ||
        typeof event.data !== 'object'
      )
        return;
      const value = event.data as {
        type?: string;
        sessionId?: string;
        runtimeId?: string;
      };
      if (
        value.type !== 'vigilshell:terminal-port' ||
        value.runtimeId !== store.getSnapshot().runtimeId ||
        !value.sessionId
      )
        return;
      const controller = store.terminals.get(value.sessionId);
      if (controller) controller.bind(event.ports[0]!);
      else event.ports[0]?.close();
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  }, [store]);
  useEffect(() => {
    for (const [id, controller] of store.terminals)
      if (!state.sessions.some((s) => s.id === id)) {
        controller.dispose();
        store.terminals.delete(id);
      }
  }, [state.sessions, store]);
  return (
    <div className="terminal-workspace" data-empty={!state.sessions.length || undefined}>
      <div className="workspace-panels">
        <div className="workspace-panel" hidden={state.sessions.length > 0}>
          <ConnectionOverview />
        </div>
        {state.sessions.map((session) => (
          <TerminalSurface
            key={session.id}
            session={session}
            active={state.activeTab === session.id}
            visible={visible}
          />
        ))}
      </div>
    </div>
  );
}
function TerminalSurface({
  session,
  active,
  visible,
}: {
  session: TerminalSessionView;
  active: boolean;
  visible: boolean;
}) {
  const { store, state } = useWorkspace(),
    ui = useWorkspaceUi(),
    { t, snapshot } = useDesktop();
  const element = useRef<HTMLDivElement | null>(null),
    input = useRef<HTMLInputElement | null>(null);
  const [controller, setController] = useState<TerminalController | null>(null),
    [search, setSearch] = useState(false),
    [query, setQuery] = useState(''),
    [caseSensitive, setCaseSensitive] = useState(false),
    [regex, setRegex] = useState(false);
  const pasteLatest = useEffectEvent((text: string) => ui.paste(text));
  const applySession = useEffectEvent((value: TerminalController) => value.state(session));
  useLayoutEffect(() => {
    let controller = store.terminals.get(session.id);
    if (!controller) {
      controller = new TerminalController(
        session.id,
        store,
        (text) => pasteLatest(text),
        (code) => store.report(code),
      );
      store.terminals.set(session.id, controller);
      window.vigilshell.attachTerminalPort(session.id);
    }
    if (element.current) controller.attach(element.current);
    applySession(controller);
    setController(controller);
    return () => controller.detach();
  }, [session.id, store]);
  useEffect(() => controller?.state(session), [controller, session]);
  useLayoutEffect(() => {
    if (snapshot.preferences) controller?.preferences(snapshot.preferences);
  }, [controller, snapshot.preferences]);
  useEffect(
    () =>
      controller?.language(
        t('ws_terminal_input'),
        t('ws_terminal_output_limit'),
        t('connection_readonly'),
        t(
          session.phase === 'failed'
            ? 'ws_failed'
            : session.phase === 'cancelled'
              ? 'ws_cancelled'
              : 'connection_disconnected',
        ),
      ),
    [controller, t, session.phase],
  );
  useEffect(() => controller?.appearance(), [controller, snapshot.appearance.theme]);
  useLayoutEffect(() => {
    if (active && visible) {
      controller?.fit();
      controller?.focus();
    }
  }, [active, visible, controller]);
  useEffect(() => {
    const fit = () => {
      if (active && visible) controller?.fit();
    };
    window.addEventListener('vigilshell:fit-terminals', fit);
    return () => window.removeEventListener('vigilshell:fit-terminals', fit);
  }, [active, visible, controller]);
  useEffect(() => {
    const find = (event: Event) => {
      if ((event as CustomEvent).detail === session.id) {
        setSearch(true);
        requestAnimationFrame(() => input.current?.focus());
      }
    };
    window.addEventListener('vigilshell:find', find);
    return () => window.removeEventListener('vigilshell:find', find);
  }, [session.id]);
  function find(next = true, value = query) {
    try {
      if (value) {
        if (next) controller?.search.findNext(value, { caseSensitive, regex });
        else controller?.search.findPrevious(value, { caseSensitive, regex });
      }
    } catch {
      store.report('invalid_input');
    }
  }
  return (
    <section
      className="workspace-panel terminal-panel"
      hidden={!active}
      id={terminalPanelId(session.id)}
      role="tabpanel"
      aria-labelledby={terminalTabId(session.id)}
      data-terminal-visible={active && visible}
    >
      {search ? (
        <div className="terminal-search">
          <SearchIcon />
          <Input
            ref={input}
            aria-label={t('ws_find')}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              find(true, e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === 'Enter') {
                e.preventDefault();
                find(!e.shiftKey);
              }
              if (e.key === 'Escape') {
                e.preventDefault();
                setSearch(false);
                controller?.focus();
              }
            }}
          />
          <Toggle
            pressed={caseSensitive}
            onPressedChange={setCaseSensitive}
            aria-label={t('ws_case_sensitive')}
          >
            Aa
          </Toggle>
          <Toggle pressed={regex} onPressedChange={setRegex} aria-label={t('ws_regex')}>
            .*
          </Toggle>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t('ws_find_previous')}
            onClick={() => find(false)}
          >
            <ArrowUpIcon />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t('ws_find_next')}
            onClick={() => find()}
          >
            <ArrowDownIcon />
          </Button>
          <CloseButton
            label={t('ws_close')}
            onClick={() => {
              setSearch(false);
              controller?.focus();
            }}
          />
        </div>
      ) : null}
      <div className="terminal-content">
        <TerminalNoticeOverlay
          hidden={
            !active ||
            !visible ||
            !!session.agentControl ||
            state.hiddenTerminalInfo.has(session.id)
          }
        >
          <TerminalConnectionInfo
            session={session}
            visible={
              active &&
              visible &&
              !session.agentControl &&
              !state.hiddenTerminalInfo.has(session.id)
            }
          />
        </TerminalNoticeOverlay>
        <ContextMenu>
          <ContextMenuTrigger className="terminal-surface" ref={element} />
          <ContextMenuContent>
            <ContextMenuGroup>
              <ContextMenuItem
                onClick={() =>
                  void window.vigilshell.copyText(controller?.terminal.getSelection() ?? '')
                }
              >
                {t('ws_copy')}
              </ContextMenuItem>
              <ContextMenuItem
                disabled={session.phase !== 'running'}
                onClick={() =>
                  void controller?.pasteClipboard().catch((e) => store.report(errorCode(e)))
                }
              >
                {t('ws_paste')}
              </ContextMenuItem>
              <ContextMenuItem
                onClick={() => {
                  setSearch(true);
                  requestAnimationFrame(() => input.current?.focus());
                }}
              >
                {t('ws_find')}
              </ContextMenuItem>
              <ContextMenuItem onClick={() => controller?.terminal.clear()}>
                {t('ws_clear_screen')}
              </ContextMenuItem>
            </ContextMenuGroup>
          </ContextMenuContent>
        </ContextMenu>
      </div>
    </section>
  );
}
