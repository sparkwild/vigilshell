import { Card, CardHeader, CardTitle, CardContent } from '../../components/ui/card';
import { Skeleton } from '../../components/ui/skeleton';
import { MonitorTrends } from './MonitorTrends';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '../../components/ui/collapsible';
import { ConnectionFailure } from '../terminal/ConnectionFailure';
import { Meter } from '../../components/ui/progress';
import { useEffect, useRef, useState } from 'react';
import { ServerIcon, ActivityIcon, ArrowDownUpIcon, ChevronDownIcon } from 'lucide-react';
import type { TerminalSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorCode, errorKey } from '../connections/workspace-context';
import { Button } from '../../components/ui/button';
import { Tabs, TabsList, TabsTrigger, TabsContent, TabsIndicator } from '../../components/ui/tabs';
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '../../components/ui/empty';
import { TransferList } from './TransferPanel';
export const formatBytes = (n: number | null) =>
  n === null
    ? '—'
    : n >= 1073741824
      ? `${(n / 1073741824).toFixed(1)} GiB`
      : n >= 1048576
        ? `${(n / 1048576).toFixed(1)} MiB`
        : `${Math.round(n / 1024)} KiB`;
export function ServerToolsPanel({ visible }: { visible: boolean }) {
  const { state } = useWorkspace(),
    { t } = useDesktop();
  const session = state.sessions.find((s) => s.id === state.activeTab);
  if (!session)
    return (
      <Empty>
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <ServerIcon aria-hidden="true" />
          </EmptyMedia>
          <EmptyTitle>{t('tools_connect_first')}</EmptyTitle>
        </EmptyHeader>
      </Empty>
    );
  return (
    <ToolsContent
      key={`${state.runtimeId}:${session.groupId}`}
      session={session}
      visible={visible}
    />
  );
}
function ToolsContent({ session, visible }: { session: TerminalSessionView; visible: boolean }) {
  const { store, state } = useWorkspace(),
    { t, snapshot: desktopSnapshot } = useDesktop();
  const tab = state.toolsTab;
  const [foreground, setForeground] = useState(
    () => document.visibilityState === 'visible' && document.hasFocus(),
  );
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  useEffect(() => {
    setDetailsOpen(false);
  }, [visible, tab, session.id]);
  const scroll = useRef<HTMLDivElement>(null);
  const online = session.phase === 'running' && state.transportReady;
  const sample = state.tools.get(session.groupId)?.sample;
  useEffect(() => {
    const update = () =>
      setForeground(document.visibilityState === 'visible' && document.hasFocus());
    window.addEventListener('focus', update);
    window.addEventListener('blur', update);
    document.addEventListener('visibilitychange', update);
    return () => {
      window.removeEventListener('focus', update);
      window.removeEventListener('blur', update);
      document.removeEventListener('visibilitychange', update);
    };
  }, []);
  useEffect(() => {
    if (!online) return;
    const watch = visible && foreground && tab === 'overview';
    let alive = true;
    void store
      .call('subscribeTools', { groupId: session.groupId, visible: watch })
      .then(() => {
        if (alive) setError(null);
      })
      .catch((e) => {
        if (alive) setError(errorCode(e));
      });
    return () => {
      alive = false;
      void store
        .call('subscribeTools', { groupId: session.groupId, visible: false })
        .catch(() => undefined);
    };
  }, [
    online,
    visible,
    foreground,
    tab,
    session.groupId,
    store,
    desktopSnapshot.preferences?.monitorSampleSeconds,
    desktopSnapshot.preferences?.monitorFullSeconds,
  ]);
  useEffect(() => {
    if (scroll.current) scroll.current.scrollTop = store.toolScrollPosition(session.connectionId);
  }, [session.connectionId, store]);
  const current = state.connections.find((c) => c.id === session.connectionId);
  const host =
    session.host.includes(':') && !session.host.startsWith('[')
      ? `[${session.host}]`
      : session.host;
  return (
    <div
      ref={scroll}
      className="flex min-h-0 flex-1 flex-col gap-2 overflow-auto px-4 pb-4 pt-2"
      onScroll={(event) =>
        store.saveToolScrollPosition(session.connectionId, event.currentTarget.scrollTop)
      }
    >
      <Tabs
        value={tab}
        onValueChange={(value) => store.selectToolsTab(value as 'overview' | 'transfers')}
      >
        <TabsList variant="segmented" className="w-full">
          <TabsTrigger value="overview" className="flex-1">
            <ActivityIcon />
            {t('tools_overview')}
          </TabsTrigger>
          <TabsTrigger value="transfers" className="flex-1">
            <ArrowDownUpIcon />
            {t('files_transfer_tab')}
            {[...state.transfers.values()].some(
              (task) =>
                task.connectionId === session.connectionId &&
                ['failed', 'awaitingDecision'].includes(task.phase),
            ) ? (
              <span role="img" aria-label={t('files_attention')} className="text-status-warning">
                !
              </span>
            ) : null}
          </TabsTrigger>
          <TabsIndicator />
        </TabsList>
        <ConnectionFailure
          key={session.id}
          session={session}
          expanded={state.diagnosticsSessionId === session.id}
          revealToken={state.toolsReveal}
        />
        <TabsContent value="overview">
          <div className="flex flex-col gap-4">
            {current && current.revision !== session.connectionRevision ? (
              <p className="text-sm text-muted-foreground">{t('ws_config_updated')}</p>
            ) : null}
            {!online && sample ? (
              <p role="status" className="text-sm text-muted-foreground">
                {t('tools_monitoring_stopped')}
              </p>
            ) : null}
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
              {[
                ['connection_target', `${session.username}@${host}:${session.port}`],
                ['tools_distribution', sample?.distribution],
                [
                  'tools_uptime',
                  sample?.uptime == null
                    ? null
                    : `${Math.floor(sample.uptime / 3600)} h ${Math.floor(sample.uptime / 60) % 60} min`,
                ],
              ].map(([key, value]) => (
                <div className="contents" key={key}>
                  <dt className="text-muted-foreground">{t(key as Parameters<typeof t>[0])}</dt>
                  <dd className="min-w-0 select-text break-words">{value ?? '—'}</dd>
                </div>
              ))}
            </dl>
            <Collapsible
              open={detailsOpen}
              onOpenChange={setDetailsOpen}
              className="flex flex-col gap-2"
            >
              <CollapsibleContent>
                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
                  {(
                    [
                      ['tools_hostname', sample?.hostname],
                      ['tools_kernel', sample?.kernel],
                      ['tools_architecture', sample?.architecture],
                    ] as const
                  ).map(([key, value]) => (
                    <div className="contents" key={key}>
                      <dt className="text-muted-foreground">{t(key)}</dt>
                      <dd className="min-w-0 select-text break-words">{value ?? '—'}</dd>
                    </div>
                  ))}
                </dl>
              </CollapsibleContent>
              {!detailsOpen ? (
                <div className="flex justify-center">
                  <CollapsibleTrigger render={<Button variant="ghost" size="sm" />}>
                    {t('tools_host_details')}
                    <ChevronDownIcon data-icon="inline-end" />
                  </CollapsibleTrigger>
                </div>
              ) : null}
            </Collapsible>
            <MonitorTrends
              collecting={
                online && visible && foreground && tab === 'overview' && !sample?.error && !error
              }
              sample={sample}
              points={store.monitorHistory?.get(session.groupId) ?? []}
            />
            <Card size="sm" role="region" aria-label={t('tools_disks')} className="gap-2">
              <CardHeader>
                <CardTitle>{t('tools_disks')}</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                {sample?.disks.length ? (
                  sample.disks.map((disk) => (
                    <Metric
                      key={disk.mount}
                      label={disk.mount}
                      value={100 * (1 - disk.available / disk.total)}
                      text={`${formatBytes(disk.total - disk.available)} / ${formatBytes(disk.total)}`}
                    />
                  ))
                ) : (
                  <Skeleton
                    className="h-1.5 w-full motion-reduce:animate-none"
                    style={
                      online && !sample?.error && !sample?.collectedAt
                        ? undefined
                        : { animation: 'none' }
                    }
                  />
                )}
              </CardContent>
            </Card>
            {sample?.error || error ? (
              <p role="status" className="text-sm text-destructive">
                {t('tools_unavailable')} ·{' '}
                {t(errorKey(sample?.error ?? error ?? 'tools_unavailable'))}
              </p>
            ) : null}
            {sample?.error || error ? (
              <Button
                variant="outline"
                disabled={!online || refreshing}
                onClick={() => {
                  setError(null);
                  setRefreshing(true);
                  void store
                    .call('refreshTools', { sessionId: session.id })
                    .catch((e) => setError(errorCode(e)))
                    .finally(() => setRefreshing(false));
                }}
              >
                {t('ws_retry')}
              </Button>
            ) : null}
          </div>
        </TabsContent>
        <TabsContent value="transfers">
          <TransferList connectionId={session.connectionId} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
function Metric({ label, value, text }: { label: string; value: number | null; text: string }) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-between gap-2 text-sm">
        <span className="truncate">{label}</span>
        <span className="shrink-0 text-muted-foreground">{text}</span>
      </div>
      {value !== null ? (
        <Meter
          appearance="resource"
          aria-label={label}
          value={Math.min(100, Math.max(0, value))}
          min={0}
          max={100}
        />
      ) : null}
    </div>
  );
}
