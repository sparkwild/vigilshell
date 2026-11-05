import { appendMonitorPoint, type MonitorPoint } from '../server-tools/monitor-history';
import { transferFeedbackKey } from '../../../shared/transfer-feedback';
import { terminalEnded, unexpectedTerminalEnd } from './session-state';
import type { ToolSnapshot, TransferView } from '@contracts';

import { OperationStore, type OperationRequest } from './operations';

import { WorkspaceFailure } from './workspace-failure';

import type {
  ConnectionAccessState,
  ConnectionAccessSnapshot,
  AgentSessionView,
  TunnelStatus,
  ConnectionDetails,
  ConnectionSummary,
  ConnectionGroupView,
  GroupLayout,
  TerminalSessionView,
  TrustView,
  WorkspaceOperation,
  WorkspaceReply,
} from '@contracts';

import type { TerminalController } from '../terminal/terminal-controller';

import type {
  WorkspaceApi,
  WorkspaceApiName,
  WorkspaceCallResult,
} from '../../../shared/workspace-api';

import { terminalIsActive } from '../../../shared/workspace-api';

import { GroupCoordinator } from './model/group-coordinator';

export { WorkspaceFailure } from './workspace-failure';

export type WorkbenchTab =
  | { kind: 'terminal'; id: string; session: TerminalSessionView }
  | { kind: 'agent'; id: string; view: AgentSessionView };
export const agentTabId = (sessionId: string) => `agent:${sessionId}`;
export type Resource = { kind: 'connection'; id: string };

export type Editor = {
  kind: 'connection';
  token: string;
  connection: ConnectionDetails | null;
  groupId: string | null;
};

export interface NavigationOptions {
  view: 'connections' | 'groups';
  search: string;
  status: 'all' | 'connected' | 'disconnected';
  sortBy: 'createdAt' | 'name';
  direction: 'asc' | 'desc';
}

export const defaultNavigation: NavigationOptions = {
  view: 'connections',
  search: '',
  status: 'all',
  sortBy: 'createdAt',
  direction: 'asc',
};

export interface GroupEditing {
  id: string;
  surfaceId: string;
  draft: string;
  error: string | null;
}

type Update<T> = T | ((previous: T) => T);

export interface TransferConflictTarget {
  runtimeId: string;
  connectionId: string;
  batchId: string;
  taskId: string;
}
export interface WorkspaceSnapshot {
  transferConflictTarget: TransferConflictTarget | null;
  transferWindows: string[];
  toolsTab: 'overview' | 'transfers';
  toolsReveal: number;
  diagnosticsSessionId: string | null;
  tools: ReadonlyMap<string, ToolSnapshot>;
  transfers: ReadonlyMap<string, TransferView>;
  tunnelStates: ReadonlyMap<string, TunnelStatus[]>;
  /** Communication availability is independent from encrypted catalogue loading. */
  transportReady: boolean;
  connectionAccess: ConnectionAccessState | null;
  ready: boolean;
  error: string | null;
  groups: ConnectionGroupView[];
  groupsReady: boolean;
  groupsError: string | null;
  layoutRevision: number;
  navigation: NavigationOptions;
  groupExpanded: Set<string>;
  groupEditing: GroupEditing | null;
  groupBusy: boolean;
  interactionSurface: string | null;
  loading: boolean;
  connections: ConnectionSummary[];
  sessions: TerminalSessionView[];
  agentViews: AgentSessionView[];
  hiddenTerminalInfo: ReadonlySet<string>;
  selected: Resource | null;
  details: ConnectionDetails | null;
  trust: TrustView | null;
  activeTab: string;
  operations: ReadonlyMap<string, WorkspaceOperation>;
  editor: Editor | null;
  version: number;
  runtimeId: string | null;
}

export class WorkspaceStore {
  private readonly grouping: GroupCoordinator;
  readonly feedback: OperationStore;
  private editorCloser: (() => Promise<void>) | null = null;
  setEditorCloser(close: (() => Promise<void>) | null) {
    this.editorCloser = close;
  }
  run = (request: OperationRequest) => this.feedback.run(request);
  connectionLabel = (id: string) => this.value.connections.find((c) => c.id === id)?.name ?? 'SSH';
  readonly terminals = new Map<string, TerminalController>();
  private listeners = new Set<() => void>();
  private value: WorkspaceSnapshot = {
    transferConflictTarget: null,
    tools: new Map(),
    transferWindows: [],
    toolsTab: 'overview',
    toolsReveal: 0,
    diagnosticsSessionId: null,
    transfers: new Map(),
    tunnelStates: new Map(),
    transportReady: false,
    connectionAccess: null,
    ready: false,
    error: null,
    groups: [],
    groupsReady: false,
    groupsError: null,
    layoutRevision: 0,
    navigation: defaultNavigation,
    groupExpanded: new Set(['u:root']),
    groupEditing: null,
    groupBusy: false,
    interactionSurface: null,
    loading: true,
    connections: [],
    sessions: [],
    agentViews: [],
    hiddenTerminalInfo: new Set(),
    selected: null,
    details: null,
    trust: null,
    activeTab: 'overview',
    operations: new Map(),
    editor: null,
    version: 0,
    runtimeId: null,
  };
  readonly monitorHistory = new Map<string, MonitorPoint[]>();
  private toolScrollPositions = new Map<string, number>();
  toolScrollPosition(id: string) {
    return this.toolScrollPositions.get(id) ?? 0;
  }
  saveToolScrollPosition(id: string, scroll: number) {
    this.toolScrollPositions.set(id, scroll);
  }
  private terminalCloseGuard:
    | ((session: TerminalSessionView) => Promise<'cancel' | 'background' | 'cancelTransfers'>)
    | null = null;
  setTerminalCloseGuard(guard: typeof this.terminalCloseGuard) {
    this.terminalCloseGuard = guard;
  }
  readonly transferSamples = new Map<
    string,
    { attempt: number; points: { time: number; bytes: number }[] }
  >();
  transferTargets = new Map<string, import('@contracts').TransferNotificationTarget>();
  showTransferNotification(target: import('@contracts').TransferNotificationTarget) {
    this.transferTargets.set(target.connectionId, target);
    this.showTransfers(target.connectionId, true);
  }
  showTransfers(connectionId: string, preserveTarget = false) {
    if (!preserveTarget) this.transferTargets.delete(connectionId);
    this.set({
      transferWindows: [
        ...this.value.transferWindows.filter((id) => id !== connectionId),
        connectionId,
      ],
    });
  }
  private shownTransferConflicts = new Map<string, string | null>();
  showTransferConflicts(id: string) {
    const task = this.value.transfers.get(id);
    if (!this.value.runtimeId || task?.phase !== 'awaitingDecision') return;
    this.set({
      transferConflictTarget: {
        runtimeId: this.value.runtimeId,
        connectionId: task.connectionId,
        batchId: task.batchId,
        taskId: id,
      },
    });
    this.presentTransferConflicts();
  }
  closeTransferConflicts() {
    this.set({ transferConflictTarget: null });
    this.presentTransferConflicts();
  }
  private presentTransferConflicts() {
    const { runtimeId, transfers, transferConflictTarget: target } = this.value;
    if (!runtimeId) return;
    for (const id of this.shownTransferConflicts.keys()) {
      if (!transfers.has(id)) this.shownTransferConflicts.delete(id);
    }
    const pending = [...transfers.values()].filter((task) => task.phase === 'awaitingDecision');
    if (target) {
      if (target.runtimeId === runtimeId) {
        for (const task of pending) {
          if (task.batchId === target.batchId && task.connectionId === target.connectionId)
            this.shownTransferConflicts.set(task.id, transferFeedbackKey(task));
        }
      }
      return;
    }
    const next = pending.find(
      (task) => this.shownTransferConflicts.get(task.id) !== transferFeedbackKey(task),
    );
    if (next) this.showTransferConflicts(next.id);
  }
  closeTransfers(connectionId: string) {
    this.set({
      transferWindows: this.value.transferWindows.filter((id) => id !== connectionId),
    });
  }
  showTransfersInSidebar(connectionId: string) {
    const current = this.value.sessions.find(
      (session) => session.id === this.value.activeTab && session.connectionId === connectionId,
    );
    const session =
      current ?? this.value.sessions.find((session) => session.connectionId === connectionId);
    if (!session) return;
    this.tab(session.id);
    this.selectToolsTab('transfers', true);
  }

  selectToolsTab(tab: 'overview' | 'transfers', reveal = false) {
    this.set({
      toolsTab: tab,
      toolsReveal: this.value.toolsReveal + (reveal ? 1 : 0),
    });
  }
  private transferRefresh: Promise<void> | null = null;
  refreshTransfers(): Promise<void> {
    if (this.transferRefresh) return this.transferRefresh;
    const runtime = this.value.runtimeId;
    const work = (async () => {
      let cursor: string | null = null,
        requestId: string | undefined;
      do {
        const reply: WorkspaceReply = await this.call('listTransfers', {
          requestId,
          cursor: cursor ?? undefined,
        });
        if (runtime !== this.value.runtimeId) return;
        if (reply.kind !== 'transferPage') return;
        requestId = reply.requestId;
        cursor = reply.next;
      } while (cursor !== null);
    })();
    this.transferRefresh = work;
    void work
      .finally(() => {
        if (this.transferRefresh === work) this.transferRefresh = null;
      })
      .catch(() => {});
    return work;
  }
  private acceptToolReply(reply: WorkspaceReply) {
    if (reply.kind === 'toolSnapshot') {
      const history = this.monitorHistory.get(reply.snapshot.groupId) ?? [];
      this.monitorHistory.set(
        reply.snapshot.groupId,
        appendMonitorPoint(history, reply.snapshot.sample),
      );
      if (this.monitorHistory.size > 128)
        this.monitorHistory.delete(this.monitorHistory.keys().next().value!);
      const tools = new Map(this.value.tools);
      tools.set(reply.snapshot.groupId, reply.snapshot);
      this.set({ tools });
    }
    if (reply.kind === 'transfers' || reply.kind === 'transferPage') {
      const transfers = new Map(this.value.transfers);
      for (const task of reply.tasks) {
        const previous = transfers.get(task.id);
        if (previous && previous.revision > task.revision) continue;
        transfers.set(task.id, task);
        if (!previous || previous.revision < task.revision) {
          const sample = this.transferSamples.get(task.id);
          const now = performance.now();
          const points =
            sample && sample.attempt === task.attempt
              ? sample.points.filter((p) => now - p.time <= 5000)
              : [];
          points.push({ time: now, bytes: task.bytes });
          this.transferSamples.set(task.id, { attempt: task.attempt, points });
        }
      }
      for (const [id, task] of transfers) {
        if (transfers.size <= 300) break;
        if (!['queued', 'running', 'awaitingDecision', 'cancelling'].includes(task.phase)) {
          transfers.delete(id);
          this.transferSamples.delete(id);
        }
      }
      this.set({ transfers });
      this.presentTransferConflicts();
    }
  }
  private credentialNotices = new Set<string>();
  private requestEpoch = 0;
  private editorGeneration = 0;
  private dismissed = new Set<string>();
  private opening = new Map<string, Promise<string | null>>();
  private tabVisits = new Map<string, number>();
  private sessionOrder = new Map<string, number>();
  private terminalInfoChoices = new Set<string>();
  private streamEnds = new Map<string, string | null>();
  private reconnecting = new Set<string>();
  private terminalInfoCountdowns = new Map<
    string,
    {
      remaining: number;
      started: number | null;
      timer: ReturnType<typeof setTimeout> | null;
    }
  >();
  private terminalOrdinals = new Map<string, number>();
  private connectionOrdinals = new Map<string, number>();
  private activitySequence = 0;
  private unsubscribe: () => void;
  constructor(readonly api: WorkspaceApi = window.vigilshell.workspace) {
    this.feedback = new OperationStore({
      runtimeId: () => this.value.runtimeId,
      get: async (id) => {
        const reply = await this.call('getOperation', { id });
        if (reply.kind === 'operation') {
          const operations = new Map(this.value.operations);
          operations.set(id, reply.operation);
          this.set({ operations });
        }
        return reply;
      },
      cancel: (id) => this.call('cancelOperation', { id }),
      reconcile: (requestId, runtimeId) => this.call('reconcile', { requestId, runtimeId }),
    });
    this.grouping = new GroupCoordinator({
      getSnapshot: this.getSnapshot,
      set: (update) => this.set(update),
      feedback: this.feedback,
      run: this.run.bind(this),
      mutation: this.mutation.bind(this),
      refresh: this.refresh.bind(this),
      setNavigation: this.setNavigation.bind(this),
      setGroupExpanded: this.setGroupExpanded.bind(this),
      setGroupEditing: this.setGroupEditing.bind(this),
    });
    this.unsubscribe = window.vigilshell.onWorkspaceEvent((note) => {
      if (this.value.runtimeId && note.params.runtimeId !== this.value.runtimeId) return;
      const e = note.params.event;
      if (e.kind === 'toolSnapshot')
        this.acceptToolReply({ kind: 'toolSnapshot', snapshot: e.snapshot });
      if (e.kind === 'transfer') this.acceptToolReply({ kind: 'transfers', tasks: [e.task] });
      if (e.kind === 'detached') this.report('renderer_restarted');
      if (e.kind === 'catalogue') {
        void this.refresh().catch(() => undefined);
      } else if (e.kind === 'operation') {
        const operations = new Map(this.value.operations);
        operations.set(e.operation.id, e.operation);
        while (operations.size > 256) operations.delete(operations.keys().next().value!);
        this.set({ operations });
      } else if (e.kind === 'tunnels') {
        const tunnelStates = new Map(this.value.tunnelStates);
        tunnelStates.set(e.connectionId, e.states);
        this.set({ tunnelStates });
      } else if (e.kind === 'inputRejected') {
        this.report(e.code, 'session:' + e.sessionId);
      } else if (e.kind === 'session') {
        if (this.dismissed.has(e.session.id)) return;
        this.terminals.get(e.session.id)?.state(e.session);
        const sessions = this.value.sessions.filter((s) => s.id !== e.session.id);
        sessions.push(e.session);
        this.set({ sessions });
      }
    });
  }
  async loadTunnels(connectionId: string) {
    const r = await this.call('listTunnels', { connectionId });
    if (r.kind === 'tunnels') {
      const tunnelStates = new Map(this.value.tunnelStates);
      tunnelStates.set(connectionId, r.states);
      this.set({ tunnelStates });
    }
  }
  getSnapshot = () => this.value;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private readonly attachedAgentTabs = new Map<string, string>();
  agentTerminal(contextId: string) {
    return this.attachedAgentTabs.get(contextId);
  }
  private set(update: Partial<WorkspaceSnapshot>) {
    if (update.runtimeId !== undefined && update.runtimeId !== this.value.runtimeId) {
      this.attachedAgentTabs.clear();
      this.shownTransferConflicts.clear();
      this.tabVisits.clear();
      this.sessionOrder.clear();
      this.terminalOrdinals.clear();
      this.connectionOrdinals.clear();
      this.activitySequence = 0;
    }
    if (update.sessions) {
      for (const session of update.sessions) {
        if (session.agentControl) {
          this.attachedAgentTabs.set(session.agentControl.contextId, session.id);
          if (
            (update.activeTab ?? this.value.activeTab) ===
            agentTabId(session.agentControl.contextId)
          )
            update.activeTab = session.id;
        }
      }
      update.sessions = update.sessions.map((session) => {
        const previous = this.value.sessions.find((item) => item.id === session.id);
        if (previous && terminalEnded(previous) && !terminalEnded(session)) return previous;
        const streamError = this.streamEnds.get(session.id);
        if (streamError && terminalEnded(session))
          return {
            ...session,
            phase: 'failed',
            error: streamError,
            endReason: { kind: 'transportError', message: null },
          };
        return session;
      });
      const ids = new Set(update.sessions.map((session) => session.id));
      const hiddenInfo = new Set([...this.value.hiddenTerminalInfo].filter((id) => ids.has(id)));
      for (const id of this.streamEnds.keys()) if (!ids.has(id)) this.streamEnds.delete(id);
      if (this.value.diagnosticsSessionId && !ids.has(this.value.diagnosticsSessionId))
        update.diagnosticsSessionId = null;
      for (const id of this.terminalInfoChoices.keys())
        if (!ids.has(id)) this.terminalInfoChoices.delete(id);
      for (const id of this.terminalInfoCountdowns.keys()) {
        if (!ids.has(id)) {
          this.pauseTerminalInfoCountdown(id);
          this.terminalInfoCountdowns.delete(id);
        }
      }
      for (const session of update.sessions) {
        const previous = this.value.sessions.find((item) => item.id === session.id);
        const connected = session.phase === 'running' && previous?.phase !== 'running';
        if (session.phase !== 'running' || connected) this.pauseTerminalInfoCountdown(session.id);
        if (connected) this.terminalInfoCountdowns.delete(session.id);
        const newlyUnexpected =
          unexpectedTerminalEnd(session) && (!previous || !unexpectedTerminalEnd(previous));
        const newlyDisconnected =
          terminalEnded(session) &&
          session.endReason?.kind === 'userDisconnected' &&
          previous?.endReason?.kind !== 'userDisconnected';
        if (newlyUnexpected || newlyDisconnected) this.terminalInfoChoices.delete(session.id);
        if (
          newlyUnexpected ||
          newlyDisconnected ||
          (connected && !this.terminalInfoChoices.has(session.id))
        )
          hiddenInfo.delete(session.id);
      }
      if (
        hiddenInfo.size !== this.value.hiddenTerminalInfo.size ||
        [...hiddenInfo].some((id) => !this.value.hiddenTerminalInfo.has(id))
      )
        update.hiddenTerminalInfo = hiddenInfo;
      for (const session of this.value.sessions) {
        if (!ids.has(session.id)) {
          this.tabVisits.delete(session.id);
          this.sessionOrder.delete(session.id);
          this.terminalOrdinals.delete(session.id);
          this.terminals.get(session.id)?.dispose();
          this.terminals.delete(session.id);
        }
      }
      const connections = new Set(update.sessions.map((s) => s.connectionId));
      for (const id of this.connectionOrdinals.keys())
        if (!connections.has(id)) this.connectionOrdinals.delete(id);
      for (const session of update.sessions) {
        if (!this.sessionOrder.has(session.id)) {
          this.sessionOrder.set(session.id, ++this.activitySequence);
          const ordinal = (this.connectionOrdinals.get(session.connectionId) ?? 0) + 1;
          this.connectionOrdinals.set(session.connectionId, ordinal);
          this.terminalOrdinals.set(session.id, ordinal);
        }
      }
      update.sessions = [...update.sessions].sort(
        (a, b) => this.sessionOrder.get(a.id)! - this.sessionOrder.get(b.id)!,
      );
    }
    if (update.activeTab && update.activeTab !== 'overview')
      this.tabVisits.set(update.activeTab, ++this.activitySequence);
    this.value = { ...this.value, ...update };
    if (
      (this.value.sessions.length || this.value.agentViews.length) &&
      this.value.activeTab === 'overview'
    ) {
      this.value = { ...this.value, activeTab: this.tabs()[0]!.id };
      this.tabVisits.set(this.value.activeTab, ++this.activitySequence);
    }
    for (const listener of this.listeners) listener();
  }
  report(code: string, owner = 'workspace:' + crypto.randomUUID()) {
    if (['saved', 'deleted', 'cancelled'].includes(code)) return;
    this.feedback.report(
      { owner, action: 'ws_overview', label: 'VigilShell' },
      new WorkspaceFailure(code),
    );
  }
  async call<K extends WorkspaceApiName>(
    name: K,
    params: Parameters<WorkspaceApi[K]>[0],
  ): Promise<WorkspaceReply> {
    const fn = this.api[name] as (
      value: Parameters<WorkspaceApi[K]>[0],
    ) => Promise<WorkspaceCallResult>;
    const runtimeId = this.value.runtimeId;
    const result = await fn(params);
    if (!result.ok) throw new WorkspaceFailure(result.code, result.requestId, result.runtimeId);
    if (result.value.kind === 'operation' && runtimeId !== this.value.runtimeId)
      throw new WorkspaceFailure('result_unknown');
    this.acceptToolReply(result.value);
    return result.value;
  }
  async mutation<K extends WorkspaceApiName>(
    name: K,
    params: Parameters<WorkspaceApi[K]>[0],
  ): Promise<WorkspaceReply> {
    try {
      return await this.call(name, params);
    } catch (error) {
      if (
        !(error instanceof WorkspaceFailure) ||
        !['timeout', 'runtime_unavailable'].includes(error.code) ||
        !error.requestId ||
        !error.runtimeId
      )
        throw error;
      let reply: WorkspaceReply;
      try {
        reply = await this.call('reconcile', {
          requestId: error.requestId,
          runtimeId: error.runtimeId,
        });
      } catch {
        throw new WorkspaceFailure('result_unknown', error.requestId, error.runtimeId);
      }
      if (reply.kind !== 'reconciled' || !reply.result)
        throw new WorkspaceFailure(
          reply.kind === 'reconciled' ? (reply.error ?? 'result_unknown') : 'result_unknown',
          error.requestId,
          error.runtimeId,
        );
      return reply.result;
    }
  }
  async waitOperation(id: string): Promise<WorkspaceOperation> {
    const runtime = this.value.runtimeId;
    for (;;) {
      if (this.value.runtimeId !== runtime) throw new WorkspaceFailure('runtime_unavailable');
      const cached = this.value.operations.get(id);
      if (cached && !['working', 'trust'].includes(cached.phase)) return cached;
      const result = await this.call('getOperation', { id });
      if (result.kind !== 'operation') throw new WorkspaceFailure('protocol_mismatch');
      const operations = new Map(this.value.operations);
      operations.set(id, result.operation);
      this.set({ operations });
      if (!['working', 'trust'].includes(result.operation.phase)) return result.operation;
      await new Promise<void>((resolve) => setTimeout(resolve, 200));
    }
  }
  async settled(reply: WorkspaceReply): Promise<WorkspaceOperation | null> {
    if (reply.kind !== 'operation') return null;
    if (!['working', 'trust'].includes(reply.operation.phase)) {
      if (reply.operation.phase !== 'success')
        throw new WorkspaceFailure(reply.operation.error ?? 'cancelled');
      return reply.operation;
    }
    let op: WorkspaceOperation;
    try {
      op = await this.waitOperation(reply.operation.id);
    } catch (error) {
      if (['save', 'delete'].includes(reply.operation.kind))
        throw new WorkspaceFailure('result_unknown');
      throw error;
    }
    if (op.phase !== 'success') throw new WorkspaceFailure(op.error ?? 'cancelled');
    return op;
  }
  acceptConnectionAccess(snapshot: ConnectionAccessSnapshot) {
    if (!snapshot.runtimeId || snapshot.runtimeId !== this.value.runtimeId || !snapshot.state)
      return;
    const state = snapshot.state;
    const old = this.value.connectionAccess;
    if (old && state.revision <= old.revision) return;
    this.set({ connectionAccess: state });
    if (state.phase === 'ready') {
      if (!old || old.phase !== 'ready') void this.initialize();
      return;
    }
    this.requestEpoch++;
    this.set({
      ready: false,
      loading: state.phase === 'locked' || state.phase === 'unlocking',
      error: state.phase === 'blocked' ? (state.code ?? 'credential_unavailable') : null,
      connections: [],
      details: null,
      trust: null,
    });
    if (this.value.transportReady && !this.value.groupsReady) void this.refreshGroups();
  }
  private groupsPending: Promise<void> | null = null;
  private groupsEpoch = 0;
  private refreshGroups(): Promise<void> {
    if (this.groupsPending) return this.groupsPending;
    const runtimeId = this.value.runtimeId;
    const epoch = ++this.groupsEpoch;
    const work = (async () => {
      try {
        let offset: number | null = 0;
        let version: number | null = null;
        const groups: ConnectionGroupView[] = [];
        while (offset !== null) {
          const reply: WorkspaceReply = await this.call('listCatalogue', {
            kind: 'groups',
            query: '',
            offset,
            version,
          });
          if (epoch !== this.groupsEpoch || runtimeId !== this.value.runtimeId) return;
          if (reply.kind !== 'catalogue') throw new WorkspaceFailure('protocol_mismatch');
          version = reply.page.version;
          groups.push(...reply.page.groups);
          if (reply.page.next !== null && reply.page.next <= offset)
            throw new WorkspaceFailure('protocol_mismatch');
          offset = reply.page.next;
        }
        this.set({ groups, groupsReady: true, groupsError: null });
      } catch (error) {
        if (epoch === this.groupsEpoch && runtimeId === this.value.runtimeId)
          this.set({
            groupsError: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable',
          });
      }
    })();
    this.groupsPending = work;
    void work.finally(() => {
      if (this.groupsPending === work) this.groupsPending = null;
    });
    return work;
  }
  backend(runtimeId: string | null, ready: boolean) {
    const refreshTransfers =
      ready && (runtimeId !== this.value.runtimeId || !this.value.transportReady);
    if (runtimeId !== this.value.runtimeId) {
      this.requestEpoch++;
      this.groupsEpoch++;
      this.groupsPending = null;
      this.dismissed.clear();
      this.opening.clear();
      this.toolScrollPositions.clear();
      this.monitorHistory.clear();
      this.transferSamples.clear();
      const sessions = this.value.sessions.map((s) =>
        terminalIsActive(s) ? { ...s, phase: 'failed' as const, error: 'runtime_unavailable' } : s,
      );
      for (const session of sessions) this.terminals.get(session.id)?.state(session);
      this.set({
        runtimeId,
        transportReady: ready,
        connectionAccess: null,
        loading: ready,
        ready: false,
        error: null,
        connections: [],
        groups: [],
        groupsReady: false,
        groupsError: null,
        selected: null,
        operations: new Map(),
        tools: new Map(),
        transfers: new Map(
          [...this.value.transfers].map(([id, task]) => [
            id,
            ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(task.phase)
              ? {
                  ...task,
                  phase: 'failed',
                  error: 'runtime_unavailable',
                  canRetry: false,
                }
              : task,
          ]),
        ),
        sessions,
      });
    } else if (this.value.transportReady !== ready) {
      this.set({ transportReady: ready });
    }
    if (refreshTransfers) void this.refreshTransfers().catch(() => undefined);
  }
  async initialize() {
    if (
      (this.value.transportReady && !this.value.connectionAccess) ||
      (this.value.connectionAccess && this.value.connectionAccess.phase !== 'ready')
    )
      return;
    const runtimeId = this.value.runtimeId;
    try {
      await this.refresh();
      if (
        runtimeId === this.value.runtimeId &&
        (!this.value.connectionAccess || this.value.connectionAccess.phase === 'ready')
      )
        this.set({ ready: true, error: null });
    } catch (error) {
      if (runtimeId !== this.value.runtimeId) return;
      this.set({
        error: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable',
        ready: false,
      });
    }
  }
  async retryConnections() {
    const runtimeId = this.value.runtimeId;
    try {
      const result = await this.call('unlockConnections', {});
      if (runtimeId !== this.value.runtimeId || result.kind !== 'connectionAccess') return;
      this.acceptConnectionAccess({ runtimeId, state: result.state });
    } catch (error) {
      if (runtimeId === this.value.runtimeId)
        this.set({ error: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable' });
    }
  }
  private refreshPending: Promise<void> | null = null;
  private refreshAgain = false;
  refresh = (): Promise<void> => {
    if (
      (this.value.transportReady && !this.value.connectionAccess) ||
      (this.value.connectionAccess && this.value.connectionAccess.phase !== 'ready')
    )
      return this.value.transportReady ? this.refreshGroups() : Promise.resolve();
    if (this.refreshPending) {
      this.refreshAgain = true;
      return this.refreshPending;
    }
    const work = this.run({
      owner: 'navigation:connections',
      action: 'ws_loading',
      label: 'SSH',
      execute: async () => {
        do {
          this.refreshAgain = false;
          await this.readCatalogue();
        } while (this.refreshAgain);
        return { kind: 'done' };
      },
    }).then(() => undefined);
    this.refreshPending = work;
    void work
      .finally(() => {
        if (this.refreshPending === work) this.refreshPending = null;
      })
      .catch(() => undefined);
    return work;
  };
  private async readCatalogue() {
    if (this.value.connectionAccess && this.value.connectionAccess.phase !== 'ready') return;
    this.groupsEpoch++;
    const epoch = ++this.requestEpoch;
    this.set({ loading: !this.value.ready });
    try {
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          let version: number | null = null,
            layoutRevision = 0;
          const groups: ConnectionGroupView[] = [],
            connections: ConnectionSummary[] = [];
          for (const kind of ['groups', 'connections'] as const) {
            let offset: number | null = 0;
            while (offset !== null) {
              const reply: WorkspaceReply = await this.call('listCatalogue', {
                kind,
                query: '',
                offset,
                version,
              });
              if (epoch !== this.requestEpoch) return;
              if (reply.kind !== 'catalogue') throw new WorkspaceFailure('protocol_mismatch');
              version = reply.page.version;
              layoutRevision = reply.page.layoutRevision;
              groups.push(...reply.page.groups);
              connections.push(...reply.page.connections);
              if (reply.page.next !== null && reply.page.next <= offset)
                throw new WorkspaceFailure('protocol_mismatch');
              offset = reply.page.next;
            }
          }
          this.set({
            groups,
            groupsReady: true,
            groupsError: null,
            connections,
            version: version ?? 0,
            ready: true,
            layoutRevision,
            error: null,
            loading: false,
          });
          const pendingCleanup = new Set(
            connections.filter((c) => c.credentialPending).map((c) => c.id),
          );
          for (const id of pendingCleanup)
            if (!this.credentialNotices.has(id))
              this.report('credential_cleanup_pending', 'connection:' + id);
          for (const id of this.credentialNotices)
            if (!pendingCleanup.has(id)) {
              for (const operation of this.feedback.getSnapshot().operations)
                if (
                  operation.owner === 'connection:' + id &&
                  operation.error?.code === 'credential_cleanup_pending'
                )
                  this.feedback.dismiss(operation.id);
            }
          this.credentialNotices = pendingCleanup;
          if (this.value.selected) await this.details(this.value.selected.id);
          return;
        } catch (error) {
          if (!(error instanceof WorkspaceFailure) || error.code !== 'conflict' || attempt === 2)
            throw error;
        }
      }
    } catch (error) {
      if (epoch !== this.requestEpoch) return;
      if (epoch === this.requestEpoch)
        this.set({
          loading: false,
          ...(this.value.ready &&
          !(error instanceof WorkspaceFailure && isConfigurationFailure(error.code))
            ? {}
            : {
                error: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable',
                ready: false,
                connections: [],
                selected: null,
              }),
        });
      throw error;
    }
  }
  setNavigation = (update: Update<NavigationOptions>) => {
    const navigation = typeof update === 'function' ? update(this.value.navigation) : update;
    this.set({ navigation });
  };
  setGroupExpanded = (update: Update<Set<string>>) =>
    this.set({
      groupExpanded: typeof update === 'function' ? update(this.value.groupExpanded) : update,
    });
  setGroupEditing = (update: Update<GroupEditing | null>) =>
    this.set({
      groupEditing: typeof update === 'function' ? update(this.value.groupEditing) : update,
    });
  currentLayout = (): GroupLayout => {
    return this.grouping.currentLayout();
  };
  commitLayout = async (layout: GroupLayout) => {
    return this.grouping.commitLayout(layout);
  };
  createGroup = async (surfaceId: string, name: string) => {
    return this.grouping.createGroup(surfaceId, name);
  };
  renameGroup = async (group: ConnectionGroupView, draft: string) => {
    return this.grouping.renameGroup(group, draft);
  };
  removeGroup = async (group: ConnectionGroupView) => {
    return this.grouping.removeGroup(group);
  };
  async select(resource: Resource) {
    this.set({
      selected: resource,
      ...(this.value.selected?.id !== resource.id ? { details: null, trust: null } : {}),
    });
    await this.details(resource.id);
  }
  private async details(id: string) {
    if (this.feedback.isBusy('connection-read:' + id)) return;
    try {
      const reply = await this.run({
        owner: 'connection-read:' + id,
        action: 'ws_loading',
        label: this.connectionLabel(id),
        execute: () => this.call('getConnection', { id }),
      });
      if (
        reply.kind === 'connection' &&
        this.value.selected?.kind === 'connection' &&
        this.value.selected.id === id
      )
        this.set({ details: reply.connection, trust: reply.trust });
    } catch (error) {
      if (error instanceof WorkspaceFailure && error.code === 'not_found')
        this.set({ selected: null, details: null, trust: null });
    }
  }
  search(query: string) {
    this.setNavigation((p) => ({ ...p, search: query }));
  }
  async newConnection(groupId: string | null = null) {
    if (this.value.editor) {
      await this.editorCloser?.();
      if (this.value.editor) return;
    }
    this.editorGeneration++;
    this.set({
      editor: {
        kind: 'connection',
        token: crypto.randomUUID(),
        connection: null,
        groupId,
      },
    });
  }
  async editConnection(id: string) {
    if (this.value.editor?.connection?.id === id) return;
    if (this.value.editor) {
      await this.editorCloser?.();
      if (this.value.editor) return;
    }
    const generation = ++this.editorGeneration;
    try {
      const reply = await this.run({
        owner: 'connection:' + id,
        action: 'ws_edit_connection',
        label: this.connectionLabel(id),
        execute: () => this.call('getConnection', { id }),
      });
      if (reply.kind === 'connection' && generation === this.editorGeneration)
        this.set({
          editor: {
            kind: 'connection',
            token: crypto.randomUUID(),
            connection: reply.connection,
            groupId: reply.connection.groupId,
          },
        });
    } catch {
      /* The operation retains its own failure. */
    }
  }
  closeEditor() {
    const editor = this.value.editor;
    if (editor) this.feedback.dismissOwnerFailures('editor:' + editor.token);
    this.editorGeneration++;
    this.set({ editor: null });
  }
  tabs(): WorkbenchTab[] {
    return [
      ...this.value.sessions.map((session) => ({
        kind: 'terminal' as const,
        id: session.id,
        session,
      })),
      ...this.value.agentViews
        .filter((view) => !this.agentTerminal(view.context.id))
        .map((view) => ({
          kind: 'agent' as const,
          id: agentTabId(view.context.id),
          view,
        })),
    ].sort((a, b) => (this.sessionOrder.get(a.id) ?? 0) - (this.sessionOrder.get(b.id) ?? 0));
  }
  observeAgentViews(views: AgentSessionView[], selected?: string | null) {
    for (const view of views) {
      if (view.context.terminal)
        this.attachedAgentTabs.set(view.context.id, view.context.terminal.id);
      const id = agentTabId(view.context.id);
      if (!this.sessionOrder.has(id)) {
        this.sessionOrder.set(id, ++this.activitySequence);
        const key = `agent:${view.context.connectionId}:${view.context.entryId}`;
        const ordinal = (this.connectionOrdinals.get(key) ?? 0) + 1;
        this.connectionOrdinals.set(key, ordinal);
        this.terminalOrdinals.set(id, ordinal);
      }
    }
    let activeTab = this.value.activeTab;
    if (selected) activeTab = this.agentTerminal(selected) ?? agentTabId(selected);
    else if (
      activeTab.startsWith('agent:') &&
      !views.some((view) => agentTabId(view.context.id) === activeTab)
    ) {
      activeTab =
        this.value.sessions[0]?.id ?? (views[0] ? agentTabId(views[0].context.id) : 'overview');
    }
    this.set({ agentViews: views, activeTab });
  }
  closeView(id: string) {
    return id.startsWith('agent:') ? this.hideAgentView(id.slice(6)) : this.closeTerminal(id);
  }
  async hideAgentView(sessionId: string) {
    const result = await window.vigilshell.mcp.request({ action: 'hideSessionView', sessionId });
    if (!result.ok) throw new WorkspaceFailure(result.code);
    this.observeAgentViews(
      result.snapshot?.views ??
        this.value.agentViews.filter((view) => view.context.id !== sessionId),
    );
  }
  tab(id: string) {
    this.set({ activeTab: id });
    const s = this.value.sessions.find((s) => s.id === id);
    const agent = this.value.agentViews.find((view) => agentTabId(view.context.id) === id);
    if (s || agent)
      void this.select({ kind: 'connection', id: s?.connectionId ?? agent!.context.connectionId });
  }
  hideTerminalInfo(id: string) {
    if (!this.value.sessions.some((session) => session.id === id)) return;
    this.terminalInfoChoices.add(id);
    this.pauseTerminalInfoCountdown(id);
    this.set({ hiddenTerminalInfo: new Set([...this.value.hiddenTerminalInfo, id]) });
  }
  showTerminalDiagnostics(id: string) {
    if (!this.value.sessions.some((session) => session.id === id)) return;
    this.tab(id);
    this.set({
      diagnosticsSessionId: id,
      toolsTab: 'overview',
      toolsReveal: this.value.toolsReveal + 1,
    });
  }
  terminalStreamEnded(id: string, code: string | null = null) {
    this.streamEnds.set(id, code);
    this.set({
      sessions: this.value.sessions.map((session) =>
        session.id !== id || terminalEnded(session)
          ? session
          : {
              ...session,
              phase: code ? 'failed' : 'closed',
              error: code,
              endReason: code ? { kind: 'transportError', message: null } : null,
            },
      ),
    });
  }
  canReconnectTerminal(id: string) {
    const session = this.value.sessions.find((item) => item.id === id);
    return (
      !!session &&
      terminalEnded(session) &&
      this.value.ready &&
      !this.reconnecting.has(id) &&
      !this.opening.has(session.connectionId) &&
      !this.feedback.isBusy('session:' + id) &&
      !this.feedback.isBusy('connection:' + session.connectionId)
    );
  }
  async reconnectTerminal(id: string) {
    if (!this.canReconnectTerminal(id)) return null;
    const session = this.value.sessions.find((item) => item.id === id)!;
    this.reconnecting.add(id);
    try {
      return await this.connect(session.connectionId, true, id);
    } finally {
      this.reconnecting.delete(id);
    }
  }
  private pauseTerminalInfoCountdown(id: string) {
    const countdown = this.terminalInfoCountdowns.get(id);
    if (!countdown || countdown.started === null) return;
    if (countdown.timer !== null) clearTimeout(countdown.timer);
    countdown.remaining = Math.max(
      0,
      countdown.remaining - (performance.now() - countdown.started),
    );
    countdown.started = null;
    countdown.timer = null;
  }
  /** 由可见浮窗租用计时；清理租用时保留余下时间，重挂载不重置五秒。 */
  startTerminalInfoCountdown(id: string): () => void {
    if (
      this.terminalInfoChoices.has(id) ||
      this.value.hiddenTerminalInfo.has(id) ||
      this.value.activeTab !== id ||
      this.value.sessions.find((session) => session.id === id)?.phase !== 'running'
    )
      return () => {};
    this.pauseTerminalInfoCountdown(id);
    const countdown = this.terminalInfoCountdowns.get(id) ?? {
      remaining: 5000,
      started: null,
      timer: null,
    };
    this.terminalInfoCountdowns.set(id, countdown);
    countdown.started = performance.now();
    const timer = setTimeout(() => {
      if (countdown.timer !== timer) return;
      countdown.timer = null;
      countdown.started = null;
      countdown.remaining = 0;
      if (
        this.terminalInfoChoices.has(id) ||
        this.value.activeTab !== id ||
        this.value.sessions.find((session) => session.id === id)?.phase !== 'running'
      )
        return;
      const hiddenTerminalInfo = new Set(this.value.hiddenTerminalInfo);
      hiddenTerminalInfo.add(id);
      this.set({ hiddenTerminalInfo });
    }, countdown.remaining);
    countdown.timer = timer;
    return () => {
      if (countdown.timer === timer) this.pauseTerminalInfoCountdown(id);
    };
  }
  terminalOrdinal(id: string) {
    return this.terminalOrdinals.get(id) ?? 1;
  }
  private terminalCloseBatch: Promise<{
    failed: { id: string; code: string }[];
    cancelled: boolean;
  }> | null = null;
  /** 串行复用单会话关闭及传输保护；只操作确认时冻结的会话集合。 */
  closeTerminals(ids: readonly string[]) {
    if (this.terminalCloseBatch) return this.terminalCloseBatch;
    const runtime = this.value.runtimeId;
    const work = (async () => {
      const failed: { id: string; code: string }[] = [];
      for (const id of new Set(ids)) {
        if (runtime !== this.value.runtimeId) return { failed, cancelled: true };
        if (!this.value.sessions.some((s) => s.id === id)) continue;
        if (this.feedback.isBusy('session:' + id)) {
          failed.push({ id, code: 'busy' });
          continue;
        }
        try {
          await this.closeTerminal(id);
          if (this.value.sessions.some((s) => s.id === id)) return { failed, cancelled: true };
        } catch (error) {
          failed.push({
            id,
            code: error instanceof WorkspaceFailure ? error.code : 'generic',
          });
        }
      }
      return { failed, cancelled: false };
    })();
    this.terminalCloseBatch = work;
    void work.finally(() => {
      if (this.terminalCloseBatch === work) this.terminalCloseBatch = null;
    });
    return work;
  }
  private dismissTerminals(ids: Set<string>) {
    if (!ids.size) return;
    const previous = this.value.sessions;
    const sessions = previous.filter((s) => !ids.has(s.id));
    let activeTab = this.value.activeTab;
    if (ids.has(activeTab)) {
      const recent = sessions
        .filter((s) => this.tabVisits.has(s.id))
        .sort((a, b) => this.tabVisits.get(b.id)! - this.tabVisits.get(a.id)!)[0];
      const index = previous.findIndex((s) => s.id === activeTab);
      const neighbor =
        previous
          .slice(0, index)
          .reverse()
          .find((s) => !ids.has(s.id)) ?? previous.slice(index + 1).find((s) => !ids.has(s.id));
      activeTab =
        recent?.id ??
        neighbor?.id ??
        (this.value.agentViews[0] ? agentTabId(this.value.agentViews[0].context.id) : 'overview');
    }
    const switched = activeTab !== this.value.activeTab;
    for (const id of ids) this.dismissed.add(id);
    this.set({ sessions, ...(switched ? { activeTab } : {}) });
    const selected = sessions.find((s) => s.id === activeTab);
    if (selected && switched) void this.select({ kind: 'connection', id: selected.connectionId });
  }
  isOpening(id: string) {
    return this.opening.has(id);
  }
  existingTerminal(id: string, phase: 'running' | 'establishing') {
    const candidates = this.value.sessions.filter(
      (session) =>
        session.connectionId === id &&
        (phase === 'running'
          ? session.phase === 'running'
          : ['connecting', 'trust', 'authenticating', 'opening'].includes(session.phase)),
    );
    return (
      candidates.find((session) => session.id === this.value.activeTab) ??
      candidates.sort(
        (a, b) =>
          (this.tabVisits.get(b.id) ?? 0) - (this.tabVisits.get(a.id) ?? 0) ||
          (this.sessionOrder.get(b.id) ?? 0) - (this.sessionOrder.get(a.id) ?? 0),
      )[0]
    );
  }
  async connect(id: string, fresh = false, replacing?: string): Promise<string | null> {
    if (!fresh) {
      const establishing = this.existingTerminal(id, 'establishing');
      if (establishing) {
        this.tab(establishing.id);
        return establishing.id;
      }
      if (this.opening.has(id)) return this.opening.get(id)!;
      const existing = this.existingTerminal(id, 'running');
      if (existing) {
        this.tab(existing.id);
        return existing.id;
      }
    }
    let accepted: string | null = null;
    const work = this.run({
      owner: 'connection:' + id,
      resources: ['connection:' + id],
      action: fresh ? 'ws_connect_new' : 'ws_connect',
      label: this.connectionLabel(id),
      execute: () =>
        this.mutation('openTerminal', {
          connectionId: id,
          columns: 80,
          rows: 24,
        }),
      onProgress: (job) => {
        if (!job.sessionId || accepted) return;
        accepted = job.sessionId;
        if (replacing) {
          this.dismissed.add(replacing);
          this.set({
            sessions: this.value.sessions.filter((s) => s.id !== replacing),
          });
        }
        this.tab(job.sessionId);
        const runtime = this.value.runtimeId;
        void this.call('listTerminals', {})
          .then((list) => {
            if (list.kind !== 'sessions' || runtime !== this.value.runtimeId) return;
            const sessions = this.value.sessions.filter(
              (s) => !list.sessions.some((n) => n.id === s.id),
            );
            sessions.push(...list.sessions.filter((s) => !this.dismissed.has(s.id)));
            this.set({ sessions });
          })
          .catch(() => undefined);
      },
    });
    const pending = work
      .then(() => {
        if (replacing) {
          for (const operation of this.feedback.getSnapshot().operations)
            if (operation.phase === 'failed' && operation.job?.sessionId === replacing)
              this.feedback.dismiss(operation.id);
        }
        return accepted;
      })
      .catch(async (error) => {
        if (
          error instanceof WorkspaceFailure &&
          error.code === 'credential_missing' &&
          !this.value.editor &&
          this.value.selected?.id === id
        )
          await this.editConnection(id);
        return null;
      });
    if (!this.opening.has(id)) this.opening.set(id, pending);
    try {
      return await pending;
    } finally {
      if (this.opening.get(id) === pending) this.opening.delete(id);
    }
  }
  closeTerminal(id: string) {
    return this.endTerminal(id, true);
  }
  disconnectTerminal(id: string) {
    return this.endTerminal(id, false);
  }
  private async endTerminal(id: string, removeTab: boolean) {
    if (this.feedback.isBusy('session:' + id)) return;
    const session = this.value.sessions.find((s) => s.id === id);
    if (session) {
      const tasks = [...this.value.transfers.values()].filter(
        (t) =>
          t.groupId === session.groupId &&
          ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(t.phase),
      );
      const others = this.value.sessions.some(
        (s) => s.id !== id && s.groupId === session.groupId && terminalIsActive(s),
      );
      if (tasks.length && !others) {
        const choice = (await this.terminalCloseGuard?.(session)) ?? 'cancel';
        if (choice === 'cancel') return;
        if (choice === 'cancelTransfers') {
          for (const task of tasks) await this.call('cancelTransfer', { id: task.id });
          const deadline = Date.now() + 30000;
          while (true) {
            await this.refreshTransfers();
            if (
              ![...this.value.transfers.values()].some(
                (t) =>
                  t.groupId === session.groupId &&
                  ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(t.phase),
              )
            )
              break;
            if (Date.now() >= deadline) throw new WorkspaceFailure('result_unknown');
            await new Promise((resolve) => setTimeout(resolve, 100));
          }
        }
      }
    }
    if (session?.agentControl) {
      const result = await window.vigilshell.mcp.request({
        action: 'closeSession',
        sessionId: session.agentControl.contextId,
      });
      if (!result.ok) throw new WorkspaceFailure(result.code);
    }
    if (session && terminalIsActive(session))
      await this.run({
        owner: 'session:' + id,
        action: removeTab ? 'ws_close_terminal' : 'ws_disconnect',
        label: session.name,
        execute: () => this.mutation('closeTerminal', { sessionId: id }),
      });
    if (!removeTab) return;
    this.dismissTerminals(new Set([id]));
  }
  async removeConnection(id: string, version: number, sessionIds: string[]) {
    const result = await this.run({
      owner: 'connection:' + id,
      resources: sessionIds.map((id) => 'session:' + id),
      action: 'ws_delete',
      label: this.connectionLabel(id),
      execute: () =>
        this.mutation('deleteConnection', {
          id,
          version,
          disconnectSessions: true,
          sessionIds,
        }),
    });
    await this.settled(result);
    const sessions = this.value.sessions.filter((s) => s.connectionId !== id);
    this.set({
      sessions,
      selected: this.value.selected?.id === id ? null : this.value.selected,
      details: null,
      activeTab:
        sessions.some((s) => s.id === this.value.activeTab) ||
        this.value.agentViews.some((view) => agentTabId(view.context.id) === this.value.activeTab)
          ? this.value.activeTab
          : 'overview',
    });
    await this.refresh();
  }

  dispose() {
    this.requestEpoch++;
    this.groupsEpoch++;
    for (const id of this.terminalInfoCountdowns.keys()) this.pauseTerminalInfoCountdown(id);
    this.terminalInfoCountdowns.clear();
    this.terminalInfoChoices.clear();
    this.streamEnds.clear();
    this.reconnecting.clear();
    this.feedback.dispose();
    this.unsubscribe();
    for (const controller of this.terminals.values()) controller.dispose();
    this.terminals.clear();
    this.tabVisits.clear();
    this.sessionOrder.clear();
    this.terminalOrdinals.clear();
    this.connectionOrdinals.clear();
    this.opening.clear();
    this.listeners.clear();
  }
}

function isConfigurationFailure(code: string) {
  return (
    code.startsWith('configuration_') ||
    code.startsWith('vault_key_') ||
    [
      'credential_denied',
      'credential_unavailable',
      'credential_corrupt',
      'credential_format_unsupported',
    ].includes(code)
  );
}
