import { desktopProtocolVersion } from '../../contracts/generated/protocol-versions';
import type { AgentEvent } from '@contracts';
import { contractValidator } from '../../contracts/validation';
const validAgentEvent = contractValidator<AgentEvent>('AgentEvent');
import { forwardDiagnostics } from './diagnostics';
import { agentDiagnostic, diagnosticFailure, workspaceDiagnostic } from './operation-diagnostics';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { TextDecoder } from 'node:util';
import type {
  Preferences,
  RpcCommand,
  RpcValue,
  RuntimeResponse,
  WorkspaceCommand,
  WorkspaceReply,
  WorkspaceNotification,
} from '@contracts';
import { isRuntimeResponse, isRuntimeNotification } from './validation';
import { isWorkspaceNotification } from '../../shared/workspace-validation';
import { TerminalFrameDecoder, encodeTerminalFrame, type TerminalFrame } from './terminal-frames';
type AnyCommand = RpcCommand | (WorkspaceCommand & { owner: string });
function controlMethod(method: string) {
  return [
    'runtime.hello',
    'runtime.shutdown',
    'preferences.reconcile',
    'uiState.reconcile',
    'requests.getStatus',
    'requests.cancel',
    'workspace.negotiate',
    'workspace.context',
    'workspace.detach',
    'operations.get',
    'operations.cancel',
    'hostTrust.confirm',
    'hostTrust.reject',
    'terminals.close',
  ].includes(method);
}
export class BackendError extends Error {
  constructor(
    readonly code: string,
    message = code,
    readonly preferences?: Preferences,
  ) {
    super(message);
    this.name = 'BackendError';
  }
}
type Completion = { id: string; result?: RpcValue; error?: BackendError };
type Pending = {
  command: AnyCommand;
  resolve: (r: RpcValue) => void;
  reject: (e: BackendError) => void;
  timer: ReturnType<typeof setTimeout>;
  expired: boolean;
};
type Connection = {
  child: ChildProcessWithoutNullStreams;
  pending: Map<string, Pending>;
  retired: Set<string>;
  buffer: Buffer;
  exit: Promise<void>;
  failed: boolean;
  instanceId: string | null;
  prefix: string;
  nextId: number;
};
/** 请求超时只结束等待；子进程和已提交操作不会因此被终止或重放。 */
export class BackendClient extends EventEmitter<{
  negotiationRecovered: [
    runtimeId: string,
    owner: string,
    state: import('@contracts').ConnectionAccessState,
  ];
  unavailable: [BackendError];
  changed: [Preferences];
  late: [Completion];
  workspace: [WorkspaceNotification];
  terminal: [TerminalFrame];
  agent: [AgentEvent];
}> {
  private connection: Connection | null = null;
  private stopping: Promise<void> | null = null;
  constructor(
    private readonly executable: string,
    private readonly dataDirectory: string,
    private readonly timeoutMs = 5000,
  ) {
    super();
  }
  get isStopping() {
    return this.stopping !== null;
  }
  get instanceId() {
    return this.connection?.instanceId ?? null;
  }
  async start(): Promise<RpcValue> {
    if (this.connection || this.stopping) await this.stop();
    const child = spawn(this.executable, ['--data-dir', this.dataDirectory, '--terminal-pipes'], {
      stdio: ['pipe', 'pipe', 'pipe', 'pipe', 'pipe'],
      windowsHide: true,
    }) as ChildProcessWithoutNullStreams;
    const c: Connection = {
      child,
      pending: new Map(),
      retired: new Set(),
      buffer: Buffer.alloc(0),
      exit: Promise.resolve(),
      failed: false,
      instanceId: null,
      prefix: randomUUID(),
      nextId: 0,
    };
    this.connection = c;
    c.exit = new Promise<void>((resolve) =>
      child.once('close', () => {
        if (this.connection === c) {
          if (!this.stopping) this.fail(c, new BackendError('runtime_unavailable'));
          this.connection = null;
        }
        this.rejectAll(c, new BackendError('runtime_unavailable'));
        resolve();
      }),
    );
    child.on('error', () => this.fail(c, new BackendError('runtime_unavailable')));
    child.stdin.on('error', () => this.fail(c, new BackendError('runtime_unavailable')));
    child.stdout.on('data', (chunk: Buffer) => this.consume(c, chunk));
    const decoder = new TerminalFrameDecoder();
    child.stdio[4]?.on('data', (chunk: Buffer) => {
      if (this.connection !== c || c.failed) return;
      try {
        for (const frame of decoder.push(chunk)) this.emit('terminal', frame);
      } catch {
        this.fail(c, new BackendError('protocol_mismatch'));
      }
    });
    child.stdio[4]?.on('end', () => {
      if (this.connection === c && !c.failed)
        this.fail(
          c,
          new BackendError(decoder.incomplete ? 'protocol_mismatch' : 'runtime_unavailable'),
        );
    });
    child.stdio[4]?.on('error', () => this.fail(c, new BackendError('runtime_unavailable')));
    child.stdio[3]?.on('error', () => this.fail(c, new BackendError('runtime_unavailable')));
    forwardDiagnostics(child.stderr, process.stderr);
    try {
      const ready = await this.request({
        method: 'runtime.hello',
        params: { application: 'vigilshell', contractVersion: desktopProtocolVersion },
      });
      if (
        ready.kind !== 'ready' ||
        ready.application !== 'vigilshell' ||
        ready.contractVersion !== desktopProtocolVersion ||
        !ready.runtimeId
      ) {
        const error = new BackendError('protocol_mismatch');
        this.fail(c, error);
        throw error;
      }
      c.instanceId = ready.runtimeId;
      return ready;
    } catch (error) {
      await this.stop();
      throw error;
    }
  }
  request(command: AnyCommand, timeoutMs = this.timeoutMs): Promise<RpcValue> {
    return this.call(command, timeoutMs).result;
  }
  call(command: AnyCommand, timeoutMs = this.timeoutMs): { id: string; result: Promise<RpcValue> } {
    const c = this.connection;
    const id = c ? `${c.prefix}:${++c.nextId}` : randomUUID();
    const control = controlMethod(command.method);
    const count = c
      ? [...c.pending.values()].filter(
          (p) =>
            (!p.expired || ['preferences.update', 'uiState.update'].includes(p.command.method)) &&
            controlMethod(p.command.method) === control,
        ).length
      : 0;
    const error =
      !c || c.failed || c.child.killed || !c.child.stdin.writable
        ? 'runtime_unavailable'
        : c.child.stdin.writableNeedDrain
          ? 'busy'
          : count >= (control ? 8 : 64)
            ? 'busy'
            : null;
    if (error || !c)
      return {
        id,
        result: Promise.reject(new BackendError(error ?? 'runtime_unavailable')),
      };
    const bytes = Buffer.from(JSON.stringify({ jsonrpc: '2.0', id, ...command }));
    if (bytes.length > (command.method === 'agent.request' ? 1024 * 1024 : 65536))
      return { id, result: Promise.reject(new BackendError('invalid_request')) };
    const result = new Promise<RpcValue>((resolve, reject) => {
      const pending: Pending = {
        command,
        resolve,
        reject,
        expired: false,
        timer: setTimeout(() => {
          pending.expired = true;
          reject(new BackendError('timeout'));
          // Retain a bounded correlation window for late answers, including timed-out writes.
          const expired = [...c.pending.entries()].filter(
            ([, p]) =>
              p.expired && !['preferences.update', 'uiState.update'].includes(p.command.method),
          );
          if (expired.length > 256) {
            const oldest = expired[0];
            if (oldest) {
              c.pending.delete(oldest[0]);
              this.retire(c, oldest[0]);
            }
          }
        }, timeoutMs),
      };
      c.pending.set(id, pending);
      c.child.stdin.write(Buffer.concat([bytes, Buffer.from('\n')]), (failure) => {
        if (failure) this.fail(c, new BackendError('runtime_unavailable'));
      });
    });
    return { id, result };
  }
  workspaceCall(owner: string, command: WorkspaceCommand) {
    const request = this.call({ ...command, owner });
    return {
      id: request.id,
      result: request.result.then((value): WorkspaceReply => {
        if (value.kind !== 'workspace') throw new BackendError('protocol_mismatch');
        return value.result;
      }),
    };
  }
  terminalFrame(frame: TerminalFrame): void {
    const c = this.connection,
      stream = c?.child.stdio[3];
    if (!c || c.failed || !stream || !('write' in stream) || !stream.writable)
      throw new BackendError('runtime_unavailable');
    if (stream.writableNeedDrain) throw new BackendError('busy');
    stream.write(encodeTerminalFrame(frame), (error) => {
      if (error) this.fail(c, new BackendError('runtime_unavailable'));
    });
  }
  release(id: string): void {
    const c = this.connection;
    const pending = c?.pending.get(id);
    if (c && pending?.expired) {
      clearTimeout(pending.timer);
      c.pending.delete(id);
      this.retire(c, id);
    }
  }
  stop(graceMs = 3000): Promise<void> {
    if (this.stopping) return this.stopping;
    const c = this.connection;
    if (!c) return Promise.resolve();
    this.stopping = (async () => {
      if (!c.failed && !c.child.killed)
        void this.request({ method: 'runtime.shutdown', params: {} }).catch(() => undefined);
      let timer: ReturnType<typeof setTimeout> | undefined;
      await Promise.race([
        c.exit,
        new Promise<void>((r) => {
          timer = setTimeout(r, graceMs);
        }),
      ]);
      if (timer) clearTimeout(timer);
      if (this.connection === c) c.child.kill('SIGKILL');
      await c.exit;
    })().finally(() => {
      this.stopping = null;
    });
    return this.stopping;
  }
  private retire(c: Connection, id: string) {
    c.retired.add(id);
    if (c.retired.size > 256) {
      const first = c.retired.values().next().value;
      if (first) c.retired.delete(first);
    }
  }
  private consume(c: Connection, chunk: Buffer): void {
    if (this.connection !== c || c.failed) return;
    let offset = 0;
    while (offset < chunk.length) {
      const end = chunk.indexOf(10, offset),
        finish = end < 0 ? chunk.length : end;
      const agentExpected =
        [...c.pending.values()].some((p) => p.command.method === 'agent.request') ||
        c.buffer.subarray(0, 256).toString().includes('"agent.changed"') ||
        chunk
          .subarray(offset, offset + 256)
          .toString()
          .includes('"agent.changed"');
      if (c.buffer.length + finish - offset > (agentExpected ? 1024 * 1024 : 65536)) {
        this.fail(c, new BackendError('protocol_mismatch', 'Runtime frame is too large'));
        return;
      }
      c.buffer = Buffer.concat([c.buffer, chunk.subarray(offset, finish)]);
      offset = finish + (end < 0 ? 0 : 1);
      if (end < 0) return;
      try {
        const value: unknown = JSON.parse(
          new TextDecoder('utf-8', { fatal: true }).decode(c.buffer),
        );
        const frameSize = c.buffer.length;
        c.buffer = Buffer.alloc(0);
        const agentFrame =
          value &&
          typeof value === 'object' &&
          (('method' in value && value.method === 'agent.changed') ||
            ('result' in value &&
              value.result &&
              typeof value.result === 'object' &&
              'result' in value.result &&
              value.result.result &&
              typeof value.result.result === 'object' &&
              'kind' in value.result.result &&
              value.result.result.kind === 'agent'));
        if (frameSize > 65536 && !agentFrame) throw new BackendError('protocol_mismatch');
        if (
          value &&
          typeof value === 'object' &&
          'method' in value &&
          value.method === 'agent.changed'
        ) {
          const note = value as {
            jsonrpc?: unknown;
            params?: { runtimeId?: unknown; event?: unknown };
          };
          if (note.jsonrpc !== '2.0' || !validAgentEvent(note.params?.event))
            throw new BackendError('protocol_mismatch');
          if (note.params?.runtimeId === c.instanceId) {
            agentDiagnostic(note.params.event as AgentEvent);
            this.emit('agent', note.params.event as AgentEvent);
          }
          continue;
        }
        if (isWorkspaceNotification(value)) {
          if (value.params.runtimeId === c.instanceId) {
            workspaceDiagnostic(value.params.event);
            this.emit('workspace', value);
          }
          continue;
        }
        if (isRuntimeNotification(value)) {
          if (
            value.jsonrpc !== '2.0' ||
            value.method !== 'preferences.changed' ||
            value.params.runtimeId !== c.instanceId
          )
            throw new BackendError('protocol_mismatch');
          this.emit('changed', value.params.preferences);
          continue;
        }
        if (
          !isRuntimeResponse(value) ||
          value.jsonrpc !== '2.0' ||
          !value.id ||
          'result' in value === 'error' in value
        )
          throw new BackendError('protocol_mismatch');
        const response: RuntimeResponse = value;
        const pending = c.pending.get(value.id);
        if (!pending) {
          const suffix = value.id.startsWith(c.prefix + ':')
            ? value.id.slice(c.prefix.length + 1)
            : '';
          const issued = Number(suffix);
          if (
            c.retired.has(value.id) ||
            (/^[1-9][0-9]*$/.test(suffix) && Number.isSafeInteger(issued) && issued <= c.nextId)
          )
            continue;
          throw new BackendError('protocol_mismatch');
        }
        if ('result' in response) {
          const kinds: Record<RpcCommand['method'], RpcValue['kind']> = {
            'runtime.hello': 'ready',
            'runtime.shutdown': 'stopped',
            'uiState.get': 'uiState',
            'uiState.update': 'uiState',
            'uiState.reconcile': 'uiReconciled',
            'preferences.get': 'preferences',
            'preferences.update': 'updated',
            'preferences.reconcile': 'reconciled',
            'requests.getStatus': 'requestStatus',
            'requests.cancel': 'cancelled',
          };
          if (
            response.result.kind !==
            ('owner' in pending.command ? 'workspace' : kinds[pending.command.method])
          )
            throw new BackendError('protocol_mismatch');
        }
        clearTimeout(pending.timer);
        c.pending.delete(value.id);
        this.retire(c, value.id);
        if ('error' in response)
          diagnosticFailure('backend_request', response.error.data.code, {
            requestId: value.id,
            method: pending.command.method,
          });
        const completion: Completion =
          'error' in response
            ? {
                id: value.id,
                error: new BackendError(
                  response.error.data.code,
                  response.error.message,
                  response.error.data.preferences ?? undefined,
                ),
              }
            : { id: value.id, result: response.result };
        // A late handshake admits its owner but does not prove the vault is unlocked.
        // Subscribers may resume reads; never resume the command that awaited the handshake.
        if (
          pending.expired &&
          pending.command.method === 'workspace.negotiate' &&
          completion.result?.kind === 'workspace' &&
          completion.result.result.kind === 'ready' &&
          completion.result.result.version === pending.command.params.version &&
          c.instanceId
        )
          this.emit(
            'negotiationRecovered',
            c.instanceId,
            pending.command.owner,
            completion.result.result.connectionAccess,
          );
        if (pending.expired) this.emit('late', completion);
        else if (completion.error) pending.reject(completion.error);
        else if (completion.result) pending.resolve(completion.result);
      } catch {
        this.fail(c, new BackendError('protocol_mismatch'));
        return;
      }
    }
  }
  private rejectAll(c: Connection, error: BackendError) {
    for (const p of c.pending.values()) {
      clearTimeout(p.timer);
      if (!p.expired) p.reject(error);
    }
    c.pending.clear();
  }
  private fail(c: Connection, error: BackendError) {
    if (this.connection !== c || c.failed) return;
    c.failed = true;
    this.rejectAll(c, error);
    if (!this.stopping) this.emit('unavailable', error);
    if (!c.child.killed) {
      c.child.stdin.destroy();
      c.child.kill('SIGKILL');
    }
  }
}
