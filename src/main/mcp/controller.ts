import type { IdentityPurpose } from './identity';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { createServer } from 'node:net';
import { AgentScreens } from './agent-screens';
import { diagnosticFailure } from '../backend/operation-diagnostics';
import type {
  AccessSecret,
  AgentCall,
  AgentConfiguration,
  AgentEvent,
  AgentMutation,
  AgentOperation,
  AgentReply,
  AgentSnapshot,
  AgentUiRequest,
  AgentClientDetection,
  AccessObservation,
  AgentClient,
  AgentDelivery,
  AgentSessionView,
} from '@contracts';
import type { McpUiResult } from '../../contracts/mcp';
import { LocalMcpServer } from './server';
import { mcpInstructions, mcpHttpConfiguration } from '../../shared/mcp-instructions';

export interface McpPorts {
  serviceFailure?(relevant: () => boolean): void;
  directory(): Promise<string | null>;
  detect(): Promise<AgentClientDetection[]>;
  locale(): 'en' | 'zh-CN';
  scrollback(): number;
  request(operation: AgentOperation): Promise<AgentReply>;
  authorize(purpose: IdentityPurpose, change: AgentMutation): Promise<boolean>;
  seal(token: string): string;
  unseal(value: string): string;
  copy(value: string): void;
  changed(event: AgentEvent): void;
  tray(visible: boolean): void;
  cancelAuthorization(): void;
  notify(event: Extract<AgentEvent, { kind: 'attention' | 'task' }>): void;
}
export class McpController {
  private configuration: AgentConfiguration | null = null;
  private protections: NonNullable<AgentSnapshot['protections']> = [];
  private contexts: AgentSnapshot['contexts'] = [];
  private stopped = false;
  private updating = false;
  setUpdating(value: boolean) {
    this.updating = value;
    if (value) {
      this.epoch++;
      this.ports.cancelAuthorization();
    }
  }
  private clients: AgentClientDetection[] = [];
  private observations: AccessObservation[] = [];
  private readonly observed = new Map<string, number>();
  private readonly prepared = new Map<
    string,
    { fingerprint: string; id: string; generation: number; delivered: boolean }
  >();
  private epoch = 0;
  private error: string | null = null;
  private recordingError: string | null = null;
  private refreshing: Promise<void> | null = null;
  private mutationPending = false;
  private readonly server: LocalMcpServer;
  private readonly screens: AgentScreens;
  private readonly views = new Map<string, AgentSessionView>();
  private readonly seenSessions = new Set<string>();
  private readonly active = new Set<string>();
  private readonly waitingReviews = new Set<string>();
  private readonly taskRevisions = new Map<string, number>();
  private readonly taskContexts = new Map<string, string>();
  constructor(private readonly ports: McpPorts) {
    this.screens = new AgentScreens(
      (operation) => this.request(operation),
      () => this.ports.scrollback(),
    );
    this.server = new LocalMcpServer({
      unavailable: () => {
        if (this.stopped) return;
        this.error = 'listener_unavailable';
        if (this.activeCount)
          this.ports.serviceFailure?.(() => !this.stopped && this.error === 'listener_unavailable');
      },
      rejected: async (digest, tool, code) => {
        await this.request({ operation: 'reject', digest, tool, code });
      },
      authenticate: async (digest) => {
        if (this.updating) throw Error('service_unavailable');
        const result = await this.request({ operation: 'authenticate', digest });
        if (result.kind !== 'authenticated') throw new Error('access_denied');
        return { ...result.entry, endpointRevision: result.endpoint_revision };
      },
      invoke: (digest, call) => this.invoke(digest, call),
      observed: async (entryId, generation, port, endpointRevision) => {
        const key = `${entryId}:${generation}:${port}:${endpointRevision}`,
          at = Date.now();
        if (this.stopped || at - (this.observed.get(key) ?? 0) < 60_000) return;
        await this.request({
          operation: 'observe',
          entry_id: entryId,
          generation,
          port,
          endpoint_revision: endpointRevision,
        });
        this.observed.set(key, at);
        if (this.observed.size > 256) this.observed.delete(this.observed.keys().next().value!);
      },
    });
  }
  displaySettingsChanged() {
    this.screens.refreshPreferences();
  }
  get activeCount() {
    return (
      this.active.size +
      this.contexts.filter((c) => c.phase === 'waitingUser' || c.phase === 'connecting').length
    );
  }
  get showTray() {
    return this.configuration?.settings.showTray ?? true;
  }
  private async request(operation: AgentOperation): Promise<AgentReply> {
    if (this.stopped) throw new Error('service_stopped');
    const result = await this.ports.request(operation);
    if (result.kind === 'error') throw new Error(result.code);
    return result;
  }
  async refresh() {
    if (this.refreshing) return this.refreshing;
    this.refreshing = this.load().finally(() => {
      this.refreshing = null;
    });
    return this.refreshing;
  }
  private async load() {
    const epoch = this.epoch;
    const result = await this.request({ operation: 'configuration' });
    if (result.kind !== 'configuration') throw new Error('invalid_response');
    if (this.stopped || epoch !== this.epoch) return;
    this.configuration = result.configuration;
    this.protections = result.protections ?? [];
    this.recordingError = result.recording_error ?? null;
    const contexts = await this.request({ operation: 'contexts' });
    if (contexts.kind === 'contexts') {
      this.contexts = mergeContexts(this.contexts, contexts.contexts);
      for (const context of this.contexts) this.observeContext(context);
    }
    const observations = await this.request({ operation: 'observations' });
    if (this.stopped || epoch !== this.epoch) return;
    if (observations.kind === 'observations') this.observations = observations.observations;
    this.ports.tray(this.showTray);
    const { enabled, port } = this.configuration.settings;
    if (!enabled) this.error = null;
    if (!enabled || (this.server.port && this.server.port !== port)) await this.server.stop();
    if (enabled && !this.server.port) {
      try {
        if (!port) throw new Error('invalid_port');
        await this.server.start(port);
        if (this.stopped || epoch !== this.epoch) await this.server.stop();
        this.error = null;
      } catch {
        this.error = 'listener_unavailable';
      }
    }
  }
  private observeContext(context: AgentSnapshot['contexts'][number]) {
    const existing = this.views.has(context.id);
    if (existing || !this.seenSessions.has(context.id))
      this.views.set(context.id, {
        context,
        runningTasks: 0,
      });
    this.seenSessions.add(context.id);
  }
  private snapshot(): AgentSnapshot {
    if (!this.configuration) throw new Error('runtime_unavailable');
    const { settings, entries, policies } = this.configuration;
    return {
      protections: this.protections,
      settings,
      views: [...this.views.values()].map((view) => ({
        ...view,
        runningTasks: [...this.active].filter(
          (id) => this.taskContexts.get(id) === view.context.id && !this.waitingReviews.has(id),
        ).length,
      })),
      entries: entries.map((e) => e.entry),
      policies,
      contexts: this.contexts,
      clients: this.clients,
      observations: this.observations,
      endpoint: settings.port ? `http://127.0.0.1:${settings.port}/mcp` : '',
      running: this.server.port !== 0,
      error: this.recordingError ?? this.error,
    };
  }
  private createSecret(name: string, previous?: AccessSecret, client?: AgentClient): AccessSecret {
    const token = randomBytes(32).toString('base64url');
    return {
      entry: {
        client: client ?? previous?.entry.client ?? null,
        id: previous?.entry.id ?? randomUUID(),
        name,
        enabled: true,
        generation: (previous?.entry.generation ?? 0) + 1,
      },
      digest: createHash('sha256').update(token).digest('hex'),
      sealed: this.ports.seal(token),
    };
  }
  private async mutate(change: AgentMutation) {
    const challenge = await this.request({ operation: 'prepare', change });
    if (challenge.kind !== 'challenge') throw new Error('invalid_response');
    let accepted = false;
    const epoch = this.epoch;
    const purpose = {
      challengeId: challenge.id,
      action: change.kind,
      ...(change.kind === 'approveConnection' ? { sessionId: change.context_id } : {}),
    };
    try {
      if (requiresIdentity(change) && !(await this.ports.authorize(purpose, change)))
        throw new Error('authorization_cancelled');
      if (epoch !== this.epoch || this.stopped) throw new Error('authorization_interrupted');
      await this.request({ operation: 'commit', challenge_id: challenge.id });
      accepted = true;
      if (requiresIdentity(change)) {
        try {
          console.info(
            JSON.stringify({
              application: 'vigilshell',
              timestamp: new Date().toISOString(),
              stage: 'identity_commit',
              ...purpose,
              outcome: 'applied',
            }),
          );
        } catch {
          /* Logging cannot undo an applied authorization. */
        }
      }
    } catch (error) {
      if (requiresIdentity(change))
        diagnosticFailure(
          'identity_commit',
          error instanceof Error ? error.message : 'unclassified_error',
          { requestId: challenge.id, sessionId: purpose.sessionId },
        );
      throw error;
    } finally {
      if (!accepted && epoch === this.epoch)
        await this.ports
          .request({ operation: 'abort', challenge_id: challenge.id })
          .catch(() => undefined);
    }
    await this.refresh();
  }
  async ui(request: AgentUiRequest): Promise<McpUiResult> {
    try {
      if (request.action === 'activitySources') {
        const reply = await this.request({ operation: 'activitySources' });
        if (reply.kind !== 'activitySources') throw Error('invalid_response');
        return { ok: true, reply };
      }
      const historical = ['task', 'tasks', 'recordings'].includes(request.action);
      if (request.action === 'activitySummaries') {
        const reply = await this.request({
          operation: 'activitySummaries',
          filter: request.filter,
        });
        if (reply.kind !== 'activitySummaries') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (
        !historical &&
        (!this.configuration ||
          !['task', 'tasks', 'recordings', 'hideSessionView', 'closeSession'].includes(
            request.action,
          ))
      )
        await this.refresh();
      const config = this.configuration!;
      if (request.action === 'rejectScript') {
        const reply = await this.request({ operation: 'rejectScript', task_id: request.taskId });
        if (reply.kind !== 'task') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'retryRecording') {
        await this.request({ operation: 'retryRecording' });
        await this.refresh();
        return { ok: true, snapshot: this.snapshot() };
      }
      if (request.action === 'selectLocalDirectory')
        return { ok: true, directory: await this.ports.directory() };
      if (request.action === 'previewPolicy') {
        const reply = await this.request({ operation: 'previewPolicy', policy: request.policy });
        if (reply.kind !== 'policyCapabilities') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'openPolicyBrowser') {
        const reply = await this.request({
          operation: 'openPolicyBrowser',
          connection_id: request.connectionId,
          connection_revision: request.connectionRevision,
        });
        if (reply.kind !== 'policyBrowser') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'readPolicyResources') {
        const reply = await this.request({
          operation: 'readPolicyResources',
          browser_id: request.browserId,
          query: request.query,
        });
        if (reply.kind !== 'policyResources') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'closePolicyBrowser') {
        await this.request({ operation: 'closePolicyBrowser', browser_id: request.browserId });
        return { ok: true };
      }
      if (request.action === 'recordings') {
        const reply = await this.request({ operation: 'recordings', filter: request.filter });
        if (reply.kind !== 'recordings') throw Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'closeSession') {
        await this.request({ operation: 'closeSession', session_id: request.sessionId });
        return { ok: true };
      }
      if (request.action === 'hideSessionView') {
        this.views.delete(request.sessionId);
        this.ports.changed({ kind: 'viewsChanged', selected_view: null });
        return { ok: true, snapshot: this.snapshot() };
      }
      if (request.action === 'detectClients') {
        this.clients = await this.ports.detect();
        return { ok: true, snapshot: this.snapshot() };
      }
      if (request.action === 'snapshot') return { ok: true, snapshot: this.snapshot() };
      if (request.action === 'task') {
        const reply = await this.request({ operation: 'task', task_id: request.taskId });
        if (reply.kind !== 'task') throw new Error('invalid_response');
        return { ok: true, reply };
      }
      if (request.action === 'tasks') {
        const reply = await this.request({
          operation: 'tasks',
          session_id: request.sessionId,
          offset: request.offset,
          connection_id: request.connectionId,
          entry_id: request.entryId,
          phase: request.phase,
        });
        if (reply.kind !== 'tasks') throw new Error('invalid_response');
        return { ok: true, reply };
      }
      if (this.mutationPending) throw new Error('authorization_busy');
      this.mutationPending = true;
      try {
        const entry =
          'id' in request ? config.entries.find((e) => e.entry.id === request.id) : undefined;
        switch (request.action) {
          case 'updateSettings': {
            const patch = { ...request.patch };
            if (patch.group === 'service') {
              if (patch.enabled && !patch.port) patch.port = await availablePort();
              if (patch.port && patch.port !== config.settings.port) await checkPort(patch.port);
            }
            await this.mutate({ kind: 'settingsPatch', patch });
            break;
          }
          case 'prepareAccess': {
            const fingerprint = JSON.stringify([
              request.client,
              request.name,
              request.id,
              request.delivery,
            ]);
            const previous = this.prepared.get(request.requestKey);
            if (previous && previous.fingerprint !== fingerprint)
              throw new Error('request_conflict');
            let secret = previous
              ? config.entries.find(
                  (e) =>
                    e.entry.id === previous.id &&
                    e.entry.generation === previous.generation &&
                    e.entry.enabled,
                )
              : undefined;
            if (previous && !secret) throw new Error('access_revoked');
            if (!secret) {
              const target = request.id
                ? config.entries.find((e) => e.entry.id === request.id)
                : undefined;
              if (request.id && !target) throw new Error('not_found');
              if (target?.entry.enabled) throw new Error('access_already_prepared');
              secret = this.createSecret(request.name.trim(), target, request.client);
              const port = config.settings.port || (await availablePort());
              if (!this.server.port) await checkPort(port);
              await this.mutate({ kind: 'provision', secret, port, delivery: request.delivery });
              this.prepared.set(request.requestKey, {
                fingerprint,
                id: secret.entry.id,
                generation: secret.entry.generation,
                delivered: false,
              });
              if (this.prepared.size > 128)
                this.prepared.delete(this.prepared.keys().next().value!);
            }
            const prepared = this.prepared.get(request.requestKey)!;
            if (!prepared.delivered) {
              await this.deliver(secret, request.delivery);
              prepared.delivered = true;
            }
            break;
          }
          case 'deliverAccess': {
            if (!entry?.entry.enabled) throw new Error('access_revoked');
            await this.ensureService();
            await this.mutate({
              kind: 'export',
              id: entry.entry.id,
              client: request.client,
              delivery: request.delivery,
              generation: entry.entry.generation,
              port: this.configuration!.settings.port,
            });
            await this.deliver(entry, request.delivery);
            break;
          }
          case 'updatePolicySettings':
            await this.mutate({
              kind: 'updatePolicySettings',
              connection_id: request.connectionId,
              expected_revision: request.expectedRevision,
              connection_revision: request.connectionRevision,
              patch: request.patch,
            });
            break;
          case 'setPolicyAccess':
            await this.mutate({
              kind: 'setPolicyAccess',
              connection_id: request.connectionId,
              expected_revision: request.expectedRevision,
              connection_revision: request.connectionRevision,
              enabled: request.enabled,
            });
            break;
          case 'renameAccess':
            if (!entry?.entry.enabled) throw new Error('access_revoked');
            if (entry.entry.client && entry.entry.client !== 'generic')
              throw new Error('invalid_access_entry');
            await this.mutate({
              kind: 'access',
              secret: { ...entry, entry: { ...entry.entry, name: request.name } },
            });
            break;
          case 'rotateAccess':
            if (!entry?.entry.enabled) throw new Error('access_revoked');
            await this.mutate({
              kind: 'access',
              secret: this.createSecret(entry.entry.name, entry),
            });
            break;
          case 'revokeAccess':
            await this.mutate({ kind: 'revoke', id: request.id });
            break;
          case 'deleteAccess':
            if (entry?.entry.enabled) throw new Error('access_not_revoked');
            await this.mutate({
              kind: 'deleteAccess',
              id: request.id,
              generation: request.generation,
            });
            break;
          case 'approveScript':
            await this.mutate({ kind: 'approveScript', task_id: request.taskId });
            break;
          case 'approveConnection':
            await this.mutate({ kind: 'approveConnection', context_id: request.contextId });
            break;
        }
      } finally {
        this.mutationPending = false;
      }
      return { ok: true, snapshot: this.snapshot() };
    } catch (error) {
      diagnosticFailure('agent_ui', error instanceof Error ? error.message : 'mcp_unavailable');
      return { ok: false, code: error instanceof Error ? error.message : 'mcp_unavailable' };
    }
  }
  private async ensureService() {
    const settings = this.configuration!.settings;
    if (!settings.enabled) {
      const port = settings.port || (await availablePort());
      await checkPort(port);
      await this.mutate({
        kind: 'settingsPatch',
        patch: {
          group: 'service',
          enabled: true,
          port,
          baseEnabled: settings.enabled,
          basePort: settings.port,
        },
      });
    }
    if (!this.server.port) throw new Error(this.error ?? 'service_stopped');
  }
  private async deliver(secret: AccessSecret, delivery: AgentDelivery) {
    if (!this.configuration?.settings.enabled || !this.server.port)
      throw new Error(this.error ?? 'service_stopped');
    const current = this.configuration.entries.find(
      (e) =>
        e.entry.id === secret.entry.id &&
        e.entry.generation === secret.entry.generation &&
        e.entry.enabled,
    );
    if (!current) throw new Error('access_revoked');
    const token = this.ports.unseal(current.sealed),
      endpoint = this.snapshot().endpoint;
    try {
      this.ports.copy(
        delivery === 'prompt'
          ? mcpInstructions(this.ports.locale(), endpoint, token)
          : mcpHttpConfiguration(endpoint, token),
      );
    } catch {
      throw new Error('access_copy_failed');
    }
  }

  async invoke(digest: string, call: AgentCall): Promise<AgentReply> {
    if (this.updating) return { kind: 'error', code: 'service_unavailable' };
    try {
      const result = await this.request({ operation: 'invoke', digest, call });
      if (
        call.tool === 'tasks_query' &&
        call.action === 'output' &&
        call.view === 'screen' &&
        result.kind === 'output'
      )
        result.output.screen = (await this.screens.text(call.task_id)) || null;
      return result;
    } catch (error) {
      return { kind: 'error', code: error instanceof Error ? error.message : 'mcp_unavailable' };
    }
  }
  event(event: AgentEvent) {
    if (this.stopped) return;
    if (event.kind === 'configuration' || event.kind === 'accessChanged')
      void this.refresh().catch(() => undefined);
    if (event.kind === 'context') {
      const current = this.contexts.find((c) => c.id === event.context.id);
      if (current && current.revision > event.context.revision) return;
      this.observeContext(event.context);
      this.contexts = mergeContexts(this.contexts, [event.context]);
    }
    if (event.kind === 'task') {
      const revision = event.task.revision ?? 0;
      const previous = this.taskRevisions.get(event.task.id);
      if (previous !== undefined && previous >= revision) return;
      this.taskRevisions.set(event.task.id, revision);
    }
    this.screens.event(event);
    if (event.kind === 'task') {
      this.taskContexts.set(event.task.id, event.task.contextId);
      if (['queued', 'waitingReview', 'running', 'stopping'].includes(event.task.phase))
        this.active.add(event.task.id);
      else this.active.delete(event.task.id);
      if (event.task.phase === 'waitingReview') this.waitingReviews.add(event.task.id);
      else this.waitingReviews.delete(event.task.id);
      if (!this.active.has(event.task.id)) this.taskContexts.delete(event.task.id);
      this.ports.notify(event);
    }
    if (event.kind === 'attention') this.ports.notify(event);
    if (!(event.kind === 'activity' && event.record.body.kind === 'output'))
      this.ports.changed(event);
  }
  async unavailable() {
    this.epoch++;
    this.ports.cancelAuthorization();
    this.configuration = null;
    this.contexts = this.contexts.map((context) =>
      ['closed', 'failed', 'disconnected'].includes(context.phase)
        ? context
        : {
            ...context,
            phase: 'disconnected',
            revision: context.revision + 1,
            error: 'runtime_unavailable',
          },
    );
    for (const context of this.contexts) this.observeContext(context);
    this.error = 'runtime_unavailable';
    this.active.clear();
    this.waitingReviews.clear();
    this.taskRevisions.clear();
    await this.server.stop();
  }
  async stop() {
    this.stopped = true;
    this.epoch++;
    this.ports.cancelAuthorization();
    await this.server.stop();
    await this.ports.request({ operation: 'stop' }).catch(() => undefined);
    await this.screens.close();
    await this.server.dispose();
  }
}
// Native verification protects server permissions; local access setup uses the same
// version-bound commit path without asking the user to authenticate again.
function requiresIdentity(change: AgentMutation): boolean {
  switch (change.kind) {
    case 'updatePolicySettings':
      return false;
    case 'setPolicyAccess':
      return change.enabled;
    case 'access':
    case 'provision':
    case 'export':
    case 'revoke':
    case 'deleteAccess':
      return false;
    case 'settingsPatch':
      return false;
    default:
      return true;
  }
}
async function availablePort(): Promise<number> {
  const reservation = createServer((socket) => socket.destroy());
  await new Promise<void>((resolve, reject) => {
    reservation.once('error', reject);
    reservation.listen(0, '127.0.0.1', resolve);
  });
  const address = reservation.address();
  const port = address && typeof address !== 'string' ? address.port : 0;
  await new Promise<void>((resolve) => reservation.close(() => resolve()));
  if (!port) throw new Error('listener_unavailable');
  return port;
}

async function checkPort(port: number) {
  const reservation = createServer((socket) => socket.destroy());
  try {
    await new Promise<void>((resolve, reject) => {
      reservation.once('error', reject);
      reservation.listen(port, '127.0.0.1', resolve);
    });
    await new Promise<void>((resolve) => reservation.close(() => resolve()));
  } catch {
    throw new Error('listener_unavailable');
  }
}

function mergeContexts(current: AgentSnapshot['contexts'], incoming: AgentSnapshot['contexts']) {
  const merged = new Map(current.map((context) => [context.id, context]));
  for (const context of incoming) {
    const old = merged.get(context.id);
    if (!old || context.revision >= old.revision) merged.set(context.id, context);
  }
  return [...merged.values()];
}
