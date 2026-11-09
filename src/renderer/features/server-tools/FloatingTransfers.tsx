import { useEffect, useId, useState } from 'react';
import { ArrowDownUpIcon } from 'lucide-react';
import { useWorkspace } from '../connections/workspace-context';
import { useDesktop } from '../../app/DesktopProvider';
import { Popover, PopoverTrigger, PopoverContent, PopoverTitle } from '../../components/ui/popover';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../components/ui/tooltip';
import { Button } from '../../components/ui/button';
import { CloseButton } from '../../components/ui/icon-button';
import { Badge } from '../../components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
} from '../../components/ui/dropdown-menu';
import { Empty, EmptyHeader, EmptyTitle } from '../../components/ui/empty';
import { TransferList } from './TransferPanel';
import { transferActive } from './file-format';
import {
  floatingCandidates,
  transferBatchKey,
  transferIsCovered,
  updateFloatingState,
  type FloatingState,
} from './floating-transfers';

export function FloatingTransfers({
  active,
  sidebarVisible,
  anchor,
}: {
  active: boolean;
  sidebarVisible: boolean;
  anchor?: HTMLElement | null;
}) {
  const { state, store } = useWorkspace();
  const { t } = useDesktop();
  const triggerId = useId();
  const [manual, setManual] = useState(false);
  const [clock, setClock] = useState(Date.now);
  const [floating, setFloating] = useState<FloatingState>({
    runtimeId: state.runtimeId,
    batches: new Map(),
  });
  const [foreground, setForeground] = useState(
    () => document.visibilityState === 'visible' && document.hasFocus(),
  );
  const [modal, setModal] = useState(false);
  useEffect(() => {
    const measure = () =>
      setForeground(document.visibilityState === 'visible' && document.hasFocus());
    window.addEventListener('focus', measure);
    window.addEventListener('blur', measure);
    document.addEventListener('visibilitychange', measure);
    return () => {
      window.removeEventListener('focus', measure);
      window.removeEventListener('blur', measure);
      document.removeEventListener('visibilitychange', measure);
    };
  }, []);
  useEffect(() => {
    const measure = () =>
      setModal(
        [
          ...document.querySelectorAll<HTMLElement>(
            '[aria-modal="true"], [data-slot="dialog-content"][data-mode="modal"]',
          ),
        ].some(
          (node) =>
            !node.hasAttribute('data-closed') &&
            !node.closest('[hidden]') &&
            (node.hasAttribute('data-open') ||
              (node.checkVisibility?.({ checkVisibilityCSS: true }) ?? !node.hidden)),
        ),
      );
    measure();
    const observer = new MutationObserver(measure);
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: [
        'aria-modal',
        'hidden',
        'style',
        'class',
        'data-mode',
        'data-open',
        'data-closed',
      ],
    });
    return () => observer.disconnect();
  }, []);
  const connectionIds = new Set(state.sessions.map((session) => session.connectionId));
  const connectionKey = [...connectionIds].sort().join('|');
  const tasks = [...state.transfers.values()].filter((task) =>
    connectionIds.has(task.connectionId),
  );
  const session = state.sessions.find((value) => value.id === state.activeTab);
  const hasCurrentTransfers = tasks.some((task) => task.connectionId === session?.connectionId);
  const sidebarConnection =
    sidebarVisible && state.toolsTab === 'transfers' ? (session?.connectionId ?? null) : null;
  const windowConnection = state.transferWindows.at(-1);
  const windowTarget = windowConnection ? store.transferTargets.get(windowConnection) : undefined;
  const covered = new Set(
    tasks
      .filter((task) =>
        transferIsCovered(task, state.runtimeId, sidebarConnection, windowConnection, windowTarget),
      )
      .map((task) => task.id),
  );
  const coverageKey = [...covered].sort().join('|');
  useEffect(() => {
    const now = Date.now();
    setClock(now);
    const connections = new Set(connectionKey ? connectionKey.split('|') : []);
    setFloating((old) =>
      updateFloatingState(
        old,
        state.runtimeId,
        [...state.transfers.values()].filter((task) => connections.has(task.connectionId)),
        new Set(coverageKey ? coverageKey.split('|') : []),
        now,
      ),
    );
  }, [state.transfers, state.runtimeId, coverageKey, connectionKey]);
  useEffect(() => {
    setManual(false);
  }, [state.runtimeId, coverageKey, active, connectionKey, state.activeTab, hasCurrentTransfers]);
  useEffect(() => {
    const deadlines = [...floating.batches.values()]
      .map((batch) => batch.until)
      .filter((until): until is number => until !== null && until > clock);
    if (!deadlines.length) return;
    const timer = setTimeout(
      () => setClock(Date.now()),
      Math.max(1, Math.min(...deadlines) - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [floating, clock]);
  const allowed =
    active &&
    foreground &&
    state.transportReady &&
    !modal &&
    !state.transferConflictTarget &&
    !state.editor;
  const automatic =
    floating.runtimeId === state.runtimeId
      ? floatingCandidates(tasks, floating, covered, clock)
      : [];
  const selected = (manual ? tasks : automatic).sort(
    (a, b) =>
      Number(transferActive(b.phase)) - Number(transferActive(a.phase)) ||
      b.createdAt - a.createdAt ||
      a.id.localeCompare(b.id),
  );
  const open = allowed && ((manual && hasCurrentTransfers) || (!manual && selected.length > 0));
  const activeCount = tasks.filter((task) => transferActive(task.phase)).length;
  const needsAttention = tasks.some((task) => ['failed', 'awaitingDecision'].includes(task.phase));
  const previewIds = new Set(selected.slice(0, 3).map((task) => task.id));
  const connections = [...new Set((manual ? tasks : selected).map((task) => task.connectionId))];
  const label = (id: string) =>
    state.connections.find((connection) => connection.id === id)?.name ?? store.connectionLabel(id);
  function dismiss() {
    setManual(false);
    setFloating((old) => ({
      ...old,
      batches: new Map(
        [...old.batches].map(([key, batch]) => [
          key,
          selected.some((task) => transferBatchKey(task) === key)
            ? { ...batch, dismissed: true }
            : batch,
        ]),
      ),
    }));
  }
  function viewAll(id: string) {
    setManual(false);
    store.showTransfersInSidebar(id);
  }
  if (!state.sessions.length) return null;
  return (
    <Popover
      open={open}
      modal={false}
      triggerId={triggerId}
      onOpenChange={(next) => {
        if (next) setManual(true);
        else if (open) dismiss();
      }}
    >
      {hasCurrentTransfers ? (
        <>
          <Tooltip>
            <PopoverTrigger
              id={triggerId}
              render={
                <TooltipTrigger
                  render={
                    <Button variant="titlebar" size="icon-sm" aria-label={t('tools_transfers')} />
                  }
                />
              }
            >
              <ArrowDownUpIcon />
            </PopoverTrigger>
            <TooltipContent>{t('tools_transfers')}</TooltipContent>
          </Tooltip>
          {activeCount > 0 ? (
            <Badge
              variant="secondary"
              aria-label={t('transfer_float_active', { count: activeCount })}
            >
              {activeCount}
            </Badge>
          ) : needsAttention ? (
            <Badge variant="warning" aria-label={t('files_attention')}>
              !
            </Badge>
          ) : null}
        </>
      ) : null}
      <PopoverContent
        align="end"
        positionerProps={{ anchor: anchor ?? undefined }}
        initialFocus={false}
        finalFocus={false}
        className="w-96 max-h-[min(32rem,var(--available-height))]"
        aria-label={t('transfer_float_title')}
      >
        <div className="flex min-w-0 flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <PopoverTitle>{t('transfer_float_title')}</PopoverTitle>
              {selected.some((task) => transferActive(task.phase)) ? (
                <Badge variant="secondary">
                  {selected.filter((task) => transferActive(task.phase)).length}
                </Badge>
              ) : null}
            </div>
            <CloseButton label={t('ws_close')} onClick={dismiss} />
          </div>
          {selected.length ? (
            <TransferList previewIds={previewIds} />
          ) : (
            <Empty>
              <EmptyHeader>
                <EmptyTitle>{t('files_no_transfers')}</EmptyTitle>
              </EmptyHeader>
            </Empty>
          )}
          {connections.length === 1 ? (
            <Button size="sm" variant="ghost" onClick={() => viewAll(connections[0]!)}>
              {t('transfer_notification_all')}
            </Button>
          ) : connections.length > 1 ? (
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button size="sm" variant="ghost" />}>
                {t('transfer_notification_all')}
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuGroup>
                  {connections.map((id) => (
                    <DropdownMenuItem key={id} onClick={() => viewAll(id)}>
                      {label(id)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
        </div>
      </PopoverContent>
    </Popover>
  );
}
