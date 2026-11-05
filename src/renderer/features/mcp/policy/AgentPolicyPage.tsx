import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';
import type { ConnectionSummary } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { useWorkspace } from '../../connections/workspace-context';
import { Empty, EmptyHeader, EmptyTitle } from '../../../components/ui/empty';
import { Button, buttonVariants } from '../../../components/ui/button';
import { defaultPolicySettings, useMcp } from '../model';
import { McpFeedback } from '../McpFeedback';
import type { LeaveGuard, RegisterLeaveGuard } from '../SettingsParts';
import { Badge } from '../../../components/ui/badge';
import { Alert, AlertDescription } from '../../../components/ui/alert';
import { ConnectionUnlock } from '../../connections/ConnectionUnlock';
import { setPolicyAccess } from './use-policy-settings';
import { policySummary } from './policy-model';
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardAction,
  CardContent,
  CardFooter,
} from '../../../components/ui/card';
import { CloseButton } from '../../../components/ui/icon-button';
import { InputGroup, InputGroupInput, InputGroupAddon } from '../../../components/ui/input-group';
import { TruncatedText } from '../../../components/ui/tooltip';
import { Label } from '../../../components/ui/label';
import { Switch } from '../../../components/ui/switch';
import { Spinner } from '../../../components/ui/spinner';
import { EnvironmentBadge } from '../../connections/EnvironmentBadge';
import {
  LinkIcon,
  UnlinkIcon,
  SearchIcon,
  EyeIcon,
  ShieldCheckIcon,
  ArrowDownUpIcon,
  ArrowLeftIcon,
  FileCheckIcon,
} from 'lucide-react';

const summaryIcons = [EyeIcon, ShieldCheckIcon, ArrowDownUpIcon, FileCheckIcon];
import { PolicyEditor } from './PolicyEditor';
import type { PolicySession } from './policy-model';

export function AgentPolicyPage({
  target,
  session,
  setSession,
  onBack,
  register,
}: {
  target: { connectionId: string | null; version: number };
  session: PolicySession;
  setSession: Dispatch<SetStateAction<PolicySession>>;
  onBack(): void;
  register: RegisterLeaveGuard;
}) {
  const { t } = useDesktop(),
    { state, store } = useWorkspace(),
    model = useMcp();
  const connections = state.connections.filter((c) => !c.deleting);
  const selected = session.selected;
  const [pending, setPending] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const flight = useRef(false);
  const guard = useRef<LeaveGuard | null>(null),
    navigating = useRef(false);
  const list = useRef<HTMLDivElement | null>(null);
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const restoreFocus = useRef<string | null>(null);
  const leave = useCallback(async () => {
    if (flight.current) return false;
    return guard.current ? guard.current() : true;
  }, []);
  useEffect(() => {
    register(leave);
    return () => register(null);
  }, [register, leave]);
  useLayoutEffect(() => {
    if (selected || !list.current) return;
    list.current.scrollTop = session.scrollTop;
    if (restoreFocus.current) {
      buttons.current.get(restoreFocus.current)?.focus({ preventScroll: true });
      restoreFocus.current = null;
    }
  }, [selected, session.scrollTop]);
  const registerEditor = useCallback((value: LeaveGuard | null) => {
    guard.current = value;
  }, []);
  useEffect(() => {
    setSession((s) => ({ ...s, selected: target.connectionId, initialized: true }));
  }, [target, setSession]);
  const current = connections.find((c) => c.id === selected);
  const connection = current;
  const policy = model.snapshot?.policies.find((p) => p.connectionId === selected);
  const select = async (id: string) => {
    if (flight.current || !state.ready || !model.snapshot || id === selected || navigating.current)
      return;
    navigating.current = true;
    try {
      if (!guard.current || (await guard.current('switch')))
        setSession((s) => ({
          ...s,
          selected: id,
          initialized: true,
          scrollTop: list.current?.scrollTop ?? s.scrollTop,
        }));
    } finally {
      navigating.current = false;
    }
  };
  const loaded = state.ready && !!model.snapshot && !state.error;
  const readError = !failure && !pending ? model.error : null;
  const unavailable = !loaded || !!readError || state.transportReady === false;
  const toggle = async (c: ConnectionSummary, enabled: boolean) => {
    if (flight.current || unavailable) return;
    flight.current = true;
    setPending(c.id);
    setFailure(null);
    try {
      const p =
        model.snapshot!.policies.find((p) => p.connectionId === c.id) ??
        defaultPolicySettings(c.id);
      const ok = await setPolicyAccess(model, c, p, enabled);
      if (ok === false) setFailure(c.id);
    } finally {
      flight.current = false;
      setPending(null);
    }
  };
  const backToList = async () => {
    if (flight.current || (guard.current && !(await guard.current()))) return;
    restoreFocus.current = selected;
    setSession((s) => ({ ...s, selected: null }));
  };
  return (
    <section
      className="flex h-full min-h-0 min-w-0 flex-col p-4 gap-3"
      aria-label={t('agent_policy_title')}
    >
      <div className="flex items-center gap-3">
        {selected ? (
          <Button variant="ghost" onClick={() => void backToList()}>
            <ArrowLeftIcon data-icon="inline-start" />
            {t('policy_back_list')}
          </Button>
        ) : (
          <InputGroup className="w-64 max-w-full">
            <InputGroupAddon>
              <SearchIcon />
            </InputGroupAddon>
            <InputGroupInput
              value={session.query}
              placeholder={t('agent_search_connections')}
              aria-label={t('agent_search_connections')}
              onChange={(e) => setSession((s) => ({ ...s, query: e.target.value }))}
            />
          </InputGroup>
        )}
        <CloseButton
          className="ml-auto"
          label={t('ws_close')}
          disabled={!!pending}
          onClick={onBack}
        />
      </div>
      {unavailable ? (
        <div className="min-h-0 overflow-auto">
          {!state.ready ? <ConnectionUnlock /> : null}
          {readError ? <McpFeedback code={readError} /> : null}
          {state.error ? (
            <Alert>
              <AlertDescription>{state.error}</AlertDescription>
            </Alert>
          ) : null}
          {readError || state.error ? (
            <Button
              variant="outline"
              onClick={() => {
                void model.refresh();
                if (state.error) void store.refresh();
              }}
            >
              {t('retry')}
            </Button>
          ) : (
            <Empty>
              <EmptyHeader>
                <Spinner />
                <EmptyTitle>
                  {t(!state.ready ? 'ws_connections_unlocking' : 'ws_loading')}
                </EmptyTitle>
              </EmptyHeader>
            </Empty>
          )}
        </div>
      ) : null}
      {selected ? (
        connection && model.snapshot ? (
          <PolicyEditor
            key={connection.id}
            connection={connection}
            policy={policy ?? defaultPolicySettings(connection.id)}
            model={model}
            register={registerEditor}
            unavailable={unavailable}
          />
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t('agent_connection_changed')}</EmptyTitle>
            </EmptyHeader>
          </Empty>
        )
      ) : (
        <div
          ref={list}
          className="min-h-0 flex-1 overflow-auto"
          onScroll={(e) => {
            const scrollTop = e.currentTarget.scrollTop;
            setSession((s) => ({ ...s, scrollTop }));
          }}
        >
          {model.snapshot ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),22.5rem))] gap-4">
              {connections
                .filter((c) =>
                  `${c.name} ${c.host}`.toLowerCase().includes(session.query.toLowerCase()),
                )
                .map((c) => {
                  const p =
                    model.snapshot!.policies.find((p) => p.connectionId === c.id) ??
                    defaultPolicySettings(c.id);
                  const rows = [
                    [t('policy_operation_level'), t(policySummary(p))],
                    [
                      t('policy_system_protection'),
                      t(p.execution.protectSystem ? 'policy_on' : 'policy_off'),
                    ],
                    [
                      t('policy_file_transfer'),
                      t(
                        !p.execution.fileTransfer
                          ? 'policy_off'
                          : p.execution.level === 'readOnly'
                            ? 'policy_download_only'
                            : 'policy_upload_download',
                      ),
                    ],
                    [
                      t('policy_script_review'),
                      t(
                        p.execution.level === 'readOnly'
                          ? 'policy_not_applicable'
                          : p.execution.scriptReview
                            ? 'policy_on'
                            : 'policy_off',
                      ),
                    ],
                  ];
                  return (
                    <Card
                      key={c.id}
                      size="sm"
                      interactive={!pending && !unavailable}
                      className="data-[size=sm]:[--card-spacing:--spacing(2.5)] gap-2"
                      onDoubleClick={(e) => {
                        if (
                          e.currentTarget.contains(e.target as Node) &&
                          !(e.target as HTMLElement).closest('button,input,label,[role="switch"]')
                        )
                          void select(c.id);
                      }}
                    >
                      <CardHeader>
                        <CardTitle>
                          <TruncatedText className="block truncate">{c.name}</TruncatedText>
                        </CardTitle>
                        <CardDescription>
                          <TruncatedText className="block truncate">{c.host}</TruncatedText>
                        </CardDescription>
                        <CardAction>
                          <EnvironmentBadge environment={c.environment} />
                        </CardAction>
                      </CardHeader>
                      <CardContent className="flex flex-1 flex-col items-start gap-2">
                        <div className="grid w-full min-w-0 grid-cols-2 gap-x-3 gap-y-2">
                          {rows.map(([label, value], index) => {
                            const Icon = summaryIcons[index]!;
                            return (
                              <div key={label} className="flex min-w-0 flex-col items-start gap-1">
                                <CardDescription className="flex min-w-0 items-center gap-1.5">
                                  <Icon className="size-4 shrink-0" aria-hidden="true" />
                                  <span>{label}</span>
                                </CardDescription>
                                <Badge
                                  variant={
                                    index === 1 && p.execution.protectSystem
                                      ? 'success'
                                      : 'secondary'
                                  }
                                >
                                  {value}
                                </Badge>
                              </div>
                            );
                          })}
                        </div>
                        {p.accessEnabled && p.connectionRevision !== c.revision ? (
                          <McpFeedback code="connection_changed" />
                        ) : null}
                        {failure === c.id ? (
                          <McpFeedback code={model.error ?? 'policy_access_check_failed'} />
                        ) : null}
                      </CardContent>
                      <CardFooter appearance="plain" className="justify-between gap-2">
                        <Button
                          ref={(node) => {
                            if (node) buttons.current.set(c.id, node);
                            else buttons.current.delete(c.id);
                          }}
                          variant="link"
                          disabled={!!pending || unavailable}
                          onClick={() => void select(c.id)}
                        >
                          {t('policy_configure')}
                        </Button>
                        <Label
                          htmlFor={`policy-card-access-${c.id}`}
                          className={buttonVariants({
                            variant: 'ghost',
                            className: 'gap-1.5 motion-reduce:transition-none',
                          })}
                          aria-disabled={!!pending || unavailable}
                          aria-busy={pending === c.id}
                        >
                          {p.accessEnabled ? (
                            <LinkIcon data-icon="inline-start" />
                          ) : (
                            <UnlinkIcon data-icon="inline-start" />
                          )}
                          <span>{t('policy_access_switch')}</span>
                          <Switch
                            id={`policy-card-access-${c.id}`}
                            checked={p.accessEnabled}
                            disabled={!!pending || unavailable}
                            onCheckedChange={(enabled) => void toggle(c, enabled)}
                          />
                        </Label>
                      </CardFooter>
                    </Card>
                  );
                })}
            </div>
          ) : null}
          {loaded &&
          !connections.some((c) =>
            `${c.name} ${c.host}`.toLowerCase().includes(session.query.toLowerCase()),
          ) ? (
            <Empty>
              <EmptyHeader>
                <EmptyTitle>
                  {t(connections.length ? 'nav_noMatches' : 'ws_no_connections')}
                </EmptyTitle>
              </EmptyHeader>
              {session.query ? (
                <Button variant="outline" onClick={() => setSession((s) => ({ ...s, query: '' }))}>
                  {t('nav_clearSearch')}
                </Button>
              ) : null}
            </Empty>
          ) : null}
        </div>
      )}
    </section>
  );
}
