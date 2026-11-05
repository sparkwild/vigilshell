import { FeatureFeedback } from '../notifications/FeatureFeedback';
import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { AgentLogo } from './McpClientPicker';
import { useEffect, useMemo, useState } from 'react';
import type { ActivitySummary, ActivitySummaryFilter, ActivitySource, TaskPhase } from '@contracts';
import type { MessageKey } from '../../../shared/i18n/messages';
import type { McpNavigation } from '../../../contracts/mcp';
import { useDesktop } from '../../app/DesktopProvider';
import { ActivityReadError } from './ActivityReadError';
import { useActivityFeed } from './use-activity-feed';
import { taskSummary } from './activity-feed';
import { activityDuration, compactDuration } from './activity-duration';
import { taskPhaseKeys, toolKey } from './session-model';
import { CloseButton, IconButton } from '../../components/ui/icon-button';
import { Button } from '../../components/ui/button';
import { InputGroup, InputGroupInput, InputGroupAddon } from '../../components/ui/input-group';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../components/ui/tooltip';
import { HoverCard, HoverCardContent, HoverCardTrigger } from '../../components/ui/hover-card';
import { Empty, EmptyHeader, EmptyTitle, EmptyMedia } from '../../components/ui/empty';
import { Tabs, TabsList, TabsTrigger, TabsIndicator } from '../../components/ui/tabs';
import {
  ResizableTableContainer,
  Virtualizer,
  TableLayout,
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
} from '../../components/ui/data-table';
import { CopyIcon, HistoryIcon, SearchIcon } from 'lucide-react';
import { Spinner } from '../../components/ui/spinner';
import { FieldDescription } from '../../components/ui/field';

const activityPhaseKeys: Record<TaskPhase, MessageKey | null> = {
  running: 'mcp_task_running',
  succeeded: 'mcp_succeeded',
  denied: 'mcp_denied',
  failed: 'mcp_failed',
  timedOut: 'mcp_failed',
  cancelled: 'mcp_cancelled',
  queued: 'mcp_task_running',
  waitingReview: 'mcp_task_running',
  stopping: 'mcp_task_running',
  requestAccepted: null,
  interruptedUnknown: null,
};
function statusKey(item: ActivitySummary) {
  return item.taskId && !item.uncertain && item.phase ? activityPhaseKeys[item.phase] : null;
}
function statusNoteKey(item: ActivitySummary) {
  if (!item.taskId) return null;
  if (item.uncertain) return 'activity_unknown';
  return item.phase && taskPhaseKeys[item.phase] !== statusKey(item)
    ? taskPhaseKeys[item.phase]
    : null;
}

function eventKind(kind: string) {
  const keys = {
    session: 'activity_kind_session',
    policy: 'activity_kind_policy',
    decision: 'activity_kind_decision',
    control: 'activity_kind_control',
    fileChanged: 'activity_kind_fileChanged',
    gap: 'activity_kind_gap',
  } as const;
  return keys[kind as keyof typeof keys] ?? 'activity_kind_historical';
}
function sessionKind(value: string) {
  const keys = {
    'session:Ready': 'activity_session_ready',
    'session:Connecting': 'activity_session_connecting',
    'session:WaitingUser': 'activity_session_waitingUser',
    'session:Closing': 'activity_session_closing',
    'session:Closed': 'activity_session_closed',
    'session:Failed': 'activity_session_failed',
  } as const;
  return keys[value as keyof typeof keys] ?? 'activity_unknown';
}
function FullField({ value }: { value: string }) {
  return (
    <Tooltip>
      <TooltipTrigger render={<span tabIndex={0} className="block max-w-72 truncate" />}>
        {value || '—'}
      </TooltipTrigger>
      <TooltipContent>{value || '—'}</TooltipContent>
    </Tooltip>
  );
}
function OperationSummary({ value, note }: { value: string; note?: string }) {
  const { t } = useDesktop();
  if (!value && !note) return <span>—</span>;
  return (
    <HoverCard>
      <HoverCardTrigger render={<span tabIndex={0} className="block w-full min-w-0 truncate" />}>
        {value || '—'}
      </HoverCardTrigger>
      <HoverCardContent
        align="start"
        className="w-max max-w-[min(36rem,var(--available-width),calc(100vw-2rem))] max-h-(--available-height) overflow-y-auto"
        role="region"
        aria-label={t('activity_summary')}
      >
        <div className="flex min-w-0 items-start gap-2">
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            {value ? (
              <pre className="min-w-0 max-w-full max-h-80 overflow-x-hidden overflow-y-auto whitespace-pre-wrap wrap-anywhere select-text">
                {value}
              </pre>
            ) : null}
            {note ? <FieldDescription>{note}</FieldDescription> : null}
          </div>
          {value ? (
            <IconButton
              label={t('copy')}
              className="shrink-0"
              onClick={() => void window.vigilshell.copyText(value)}
            >
              <CopyIcon />
            </IconButton>
          ) : null}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}
export function AgentActivity({
  target,
  onBack,
}: {
  target: Extract<McpNavigation, { view: 'activity' }>;
  onBack?: () => void;
}) {
  const { t, snapshot } = useDesktop();
  const api = window.vigilshell.mcp;
  const [sources, setSources] = useState<ActivitySource[]>([]);
  const clients = useMemo(
    () => new Map(sources.map((source) => [source.id, source.client])),
    [sources],
  );
  const sourceReady = !snapshot?.runtime || snapshot.runtime.status === 'ready';
  const sourceRuntime = snapshot?.runtime?.instanceId;
  useEffect(() => {
    let alive = true;
    let sequence = 0;
    setSources([]);
    if (!sourceReady) return;
    const read = async () => {
      const current = ++sequence;
      try {
        const result = await api.request({ action: 'activitySources' });
        if (alive && current === sequence && result.ok && result.reply?.kind === 'activitySources')
          setSources(result.reply.sources);
      } catch {
        /* Missing optional logo metadata uses the existing generic symbol. */
      }
    };
    const stop = api.onChanged((event) => {
      if (event.kind === 'configuration' || event.kind === 'accessChanged') void read();
    });
    void read();
    return () => {
      alive = false;
      stop();
    };
  }, [api, sourceReady, sourceRuntime]);
  const [filter, setFilter] = useState<ActivitySummaryFilter>({});
  const { items: feedItems, more, next, busy, error } = useActivityFeed(filter, snapshot?.runtime);
  const [locatedTask, setLocatedTask] = useState<string>();
  const [locatedSummary, setLocatedSummary] = useState<ActivitySummary | null>(null);
  const [locating, setLocating] = useState(false);
  const [locationError, setLocationError] = useState<string | null>(null);
  useEffect(() => {
    setLocatedTask(target.taskId);
    if (target.taskId || target.contextId)
      setFilter(target.contextId && !target.taskId ? { sessionId: target.contextId } : {});
  }, [target]);
  useEffect(() => {
    let alive = true;
    setLocatedSummary(null);
    setLocationError(null);
    setLocating(false);
    if (!locatedTask || !sourceReady) return;
    setLocating(true);
    void api
      .request({ action: 'task', taskId: locatedTask })
      .then((result) => {
        if (!alive) return;
        if (!result.ok) setLocationError(result.code);
        else if (result.reply?.kind !== 'task') setLocationError('invalid_response');
        else setLocatedSummary(taskSummary(result.reply.task));
      })
      .catch(() => {
        if (alive) setLocationError('runtime_unavailable');
      })
      .finally(() => {
        if (alive) setLocating(false);
      });
    return () => {
      alive = false;
    };
  }, [locatedTask, api, sourceReady, sourceRuntime]);
  const locatedItem = feedItems.find((item) => item.taskId === locatedTask) ?? locatedSummary;
  const items = useMemo(
    () => (locatedTask ? (locatedItem ? [locatedItem] : []) : feedItems),
    [locatedTask, locatedItem, feedItems],
  );
  const stripes = useMemo(
    () => new Map(items.map((item, index) => [item.id, index % 2 ? 'even' : 'odd'])),
    [items],
  );
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  function duration(item: ActivitySummary) {
    const value = activityDuration(item, now);
    return value === 'unknown' || value == null ? '—' : compactDuration(value);
  }
  return (
    <section
      className="relative flex h-full min-h-0 flex-col gap-3 p-4"
      aria-label={t('mcp_activity')}
    >
      {onBack ? (
        <CloseButton
          className="absolute top-4 right-4"
          label={t('activity_close')}
          onClick={onBack}
          data-testid="close-activity"
        />
      ) : null}
      <div className="flex min-h-0 flex-1 flex-col gap-3">
        <FeatureFeedback kinds={['agentTask', 'agentContext', 'agentActivity']} />
        <div className="flex flex-wrap items-center gap-3 pr-12">
          <Tabs
            value={filter.active == null ? 'all' : filter.active ? 'active' : 'ended'}
            onValueChange={(value) => {
              setLocatedTask(undefined);
              setFilter((old) => ({
                query: old.query,
                active: value === 'all' ? null : value === 'active',
              }));
            }}
          >
            <TabsList variant="segmented">
              <TabsIndicator />
              {(['all', 'active', 'ended'] as const).map((value) => (
                <TabsTrigger key={value} value={value}>
                  {t(
                    value === 'all'
                      ? 'mcp_all'
                      : value === 'active'
                        ? 'activity_active'
                        : 'activity_ended',
                  )}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <InputGroup className="w-64 max-w-full">
            <InputGroupAddon>
              <SearchIcon />
            </InputGroupAddon>
            <InputGroupInput
              value={filter.query ?? ''}
              placeholder={t('activity_search')}
              onChange={(event) => {
                setLocatedTask(undefined);
                setFilter((old) => ({ active: old.active, query: event.target.value }));
              }}
            />
          </InputGroup>
        </div>
        <ActivityReadError code={locationError ?? error} />
        <div className="relative min-h-0 flex-1 overflow-auto" aria-busy={busy || locating}>
          <ResizableTableContainer className="file-table-scroll">
            <Virtualizer layout={TableLayout} layoutOptions={{ rowHeight: 36, headingHeight: 28 }}>
              <Table
                className="file-table"
                data-appearance="striped"
                aria-label={t('activity_events')}
                selectionMode="none"
              >
                <TableHeader>
                  {(
                    [
                      'activity_agent',
                      'activity_type',
                      'activity_summary',
                      'mcp_connection_filter',
                      'mcp_state_filter',
                      'activity_initiated',
                      'activity_duration',
                    ] as const
                  ).map((key, index) => (
                    <Column
                      key={key}
                      id={key}
                      defaultWidth={index === 2 ? '2fr' : index === 5 ? 170 : '1fr'}
                      minWidth={90}
                      data-align={index >= 4 ? 'center' : undefined}
                      isRowHeader={index === 0}
                    >
                      {t(key)}
                    </Column>
                  ))}
                </TableHeader>
                <TableBody items={items}>
                  {(item) => (
                    <Row id={item.id} data-stripe={stripes.get(item.id)}>
                      <Cell>
                        <span className="flex min-w-0 items-center gap-2">
                          <AgentLogo
                            client={item.entryId ? clients.get(item.entryId) : undefined}
                          />
                          <FullField value={item.entryName} />
                        </span>
                      </Cell>
                      <Cell>
                        <FullField
                          value={item.taskId ? t(toolKey(item.kind)) : t(eventKind(item.kind))}
                        />
                      </Cell>
                      <Cell>
                        <OperationSummary
                          note={statusNoteKey(item) ? t(statusNoteKey(item)!) : undefined}
                          value={
                            item.summary.startsWith('session:')
                              ? t(sessionKind(item.summary))
                              : item.summary
                          }
                        />
                      </Cell>
                      <Cell>
                        <FullField value={item.connectionName} />
                      </Cell>
                      <Cell data-align="center">
                        <ActivityNotificationStatus
                          item={item}
                          value={statusKey(item) ? t(statusKey(item)!) : '—'}
                        />
                      </Cell>
                      <Cell data-align="center">
                        <FullField value={new Date(item.createdAt).toLocaleString()} />
                      </Cell>
                      <Cell data-align="center">
                        <FullField value={duration(item)} />
                      </Cell>
                    </Row>
                  )}
                </TableBody>
              </Table>
            </Virtualizer>
          </ResizableTableContainer>
          {!items.length && !error && !locationError ? (
            <div className="file-table-state" role="status">
              <Empty>
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    {busy || locating ? (
                      <Spinner />
                    ) : filter.query ? (
                      <SearchIcon />
                    ) : (
                      <HistoryIcon />
                    )}
                  </EmptyMedia>
                  <EmptyTitle>
                    {t(
                      busy || locating
                        ? 'ws_working'
                        : filter.query
                          ? 'ws_no_results'
                          : 'activity_empty',
                    )}
                  </EmptyTitle>
                </EmptyHeader>
              </Empty>
            </div>
          ) : null}
        </div>
        {more && !locatedTask ? (
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => setFilter((old) => ({ ...old, ...next }))}
          >
            {t('agent_load_more')}
          </Button>
        ) : null}
      </div>
    </section>
  );
}

function ActivityNotificationStatus({ item, value }: { item: ActivitySummary; value: string }) {
  const ref = useNotificationSurface<HTMLSpanElement>(
    item.taskId ? `agentTask:${item.taskId}` : null,
  );
  return (
    <span ref={ref}>
      <FullField value={value} />
    </span>
  );
}
