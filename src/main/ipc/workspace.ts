import { WorkspaceNotificationCoordinator } from '../notifications/workspace';
import type { NotificationFeedbackCoordinator } from '../notifications/feedback';
import { TransferNotificationCoordinator } from '../notifications/transfers';
import type { SystemNotificationService } from '../notifications/service';
import { contractValidator } from '../../contracts/validation';
import { transferFeedbackKey } from '../../shared/transfer-feedback';
const validTransferFeedback =
  contractValidator<import('@contracts').TransferFeedbackReport>('TransferFeedbackReport');
import { workspaceProtocolVersion } from '../../contracts/generated/protocol-versions';
import { access, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import type { UiStateService } from '../platform/ui-state';
import type { DesktopSnapshot } from '@contracts';
import { translate } from '../../shared/i18n/messages';
import {
  clipboard,
  dialog,
  ipcMain,
  MessageChannelMain,
  type BrowserWindow,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
  type MessagePortMain,
} from 'electron';
import { dirname } from 'node:path';
import { randomUUID } from 'node:crypto';
import type {
  ConnectionAccessState,
  ConnectionAccessSnapshot,
  Appearance,
  WorkspaceCommand,
  WorkspaceReply,
  TerminalSessionView,
  WorkspaceOperation,
} from '@contracts';
import { BackendClient, BackendError } from '../backend/client';
import { validWorkspaceCommand } from '../../shared/workspace-validation';
import {
  workspaceMethods,
  terminalIsActive,
  type WorkspaceCallResult,
  type WorkspaceUiState,
} from '../../shared/workspace-api';
import type { TerminalFrame } from '../backend/terminal-frames';
type BoundPort = { port: MessagePortMain; runtime: string };
export class WorkspaceIpc {
  private transferNotifications?: TransferNotificationCoordinator;
  private eventNotifications?: WorkspaceNotificationCoordinator;
  private pickerDirectories: Partial<Record<'upload' | 'download', string>> = {};
  private owner = randomUUID();
  private rendererReloaded = false;
  private attachedRuntime: string | null = null;
  private attaching: Promise<void> | null = null;
  private connectionAccess: ConnectionAccessSnapshot = { runtimeId: null, state: null };
  private lateNegotiationOwners = new Set<string>();
  private appearance: Appearance = {
    locale: 'en',
    theme: 'light',
    reducedTransparency: false,
  };
  private ports = new Map<string, BoundPort>();
  private retired = new Set<string>();
  private transferTasks = new Map<string, import('@contracts').TransferView>();
  private pending = new Map<string, TerminalFrame[]>();
  private sessions = new Map<string, TerminalSessionView>();
  private operations = new Map<string, WorkspaceOperation>();
  ui: WorkspaceUiState = {
    dirty: false,
    modalOpen: false,
    activeSessionId: null,
    active: true,
  };
  constructor(
    private host: BackendClient,
    private window: () => BrowserWindow | undefined,
    private allowed: (event: IpcMainEvent | IpcMainInvokeEvent) => boolean,
    private settings?: {
      state: () => DesktopSnapshot | undefined;
      ui: UiStateService;
    },
  ) {
    host.on('workspace', (note) => {
      if (note.params.owner !== this.owner && note.params.event.kind !== 'catalogue') return;
      const e = note.params.event;
      if (note.params.runtimeId !== this.host.instanceId) return;
      if (e.kind === 'connectionAccess')
        this.acceptConnectionAccess(note.params.runtimeId, note.params.owner, e.state);
      if (e.kind === 'transfer') {
        if ((this.transferTasks.get(e.task.id)?.revision ?? -1) >= e.task.revision) return;
        this.transferTasks.set(e.task.id, e.task);
        this.transferNotifications?.observe(e.task);
      }
      if (e.kind === 'session') {
        this.eventNotifications?.session(e.session);
        this.sessions.set(e.session.id, e.session);
      }
      if (e.kind === 'tunnels') this.eventNotifications?.tunnel(e.connectionId, e.states);
      if (e.kind === 'operation') {
        this.eventNotifications?.operation(e.operation);
        this.operations.set(e.operation.id, e.operation);
        while (this.operations.size > 256)
          this.operations.delete(this.operations.keys().next().value!);
        if (
          e.operation.kind === 'credential' &&
          !['working', 'trust'].includes(e.operation.phase) &&
          this.window()?.isVisible()
        )
          this.window()?.focus();
      }
      this.window()?.webContents.send('workspace:event', note);
    });
    host.on('negotiationRecovered', (runtime, owner, access) => {
      if (!this.lateNegotiationOwners.delete(owner)) return;
      void this.completeAttachment(runtime, owner, access).catch(() => undefined);
    });
    host.on('terminal', (frame) => this.receive(frame));
    host.on('unavailable', () => {
      this.connectionAccess = { runtimeId: null, state: null };
      this.window()?.webContents.send('workspace:connection-access-changed', this.connectionAccess);
      this.lateNegotiationOwners.clear();
      this.attachedRuntime = null;
      this.attaching = null;
      this.closePorts();
      this.sessions.clear();
      this.operations.clear();
      this.transferTasks.clear();
      this.transferNotifications?.reset();
      this.eventNotifications?.reset();
    });
    ipcMain.handle('workspace:connection-access', async (event) => {
      if (!this.allowed(event)) throw new BackendError('invalid_sender');
      await this.ensure();
      if (!this.allowed(event)) throw new BackendError('invalid_sender');
      return this.connectionAccess;
    });
    for (const [name, method] of Object.entries(workspaceMethods))
      ipcMain.handle(`workspace:${name}`, async (event, params): Promise<WorkspaceCallResult> => {
        if (!this.allowed(event)) return { ok: false, code: 'invalid_sender' };
        const command = { method, params };
        if (!validWorkspaceCommand(command)) return { ok: false, code: 'invalid_input' };
        try {
          await this.ensure();
        } catch (error) {
          return {
            ok: false,
            code: error instanceof BackendError ? error.code : 'runtime_unavailable',
          };
        }
        let hostCommand: WorkspaceCommand = command;
        if (method === 'files.pick') {
          const owner = this.owner;
          const epoch = this.host.instanceId;
          const direction = (params as { direction: 'upload' | 'download' }).direction;
          const remembered = await this.settings?.ui.get('pickers').catch(() => null);
          if (remembered?.data?.kind === 'pickers')
            this.pickerDirectories = {
              upload: remembered.data.upload,
              download: remembered.data.download,
            };
          const preferences = this.settings?.state()?.preferences;
          let savedDirectory: string | undefined;
          if (direction === 'download' && preferences?.downloadMode === 'directory') {
            try {
              if (!(await stat(preferences.downloadDirectory)).isDirectory())
                throw Error('directory_unavailable');
              await access(preferences.downloadDirectory, constants.W_OK);
              savedDirectory = preferences.downloadDirectory;
            } catch {
              const options = {
                type: 'warning' as const,
                message: translate('settings_directory_unavailable', this.appearance.locale),
              };
              const target = this.window();
              if (target) await dialog.showMessageBox(target, options);
              else await dialog.showMessageBox(options);
            }
          }
          const options: Electron.OpenDialogOptions = {
            defaultPath: this.pickerDirectories[direction],
            properties:
              direction === 'upload'
                ? ['openFile', 'openDirectory', 'multiSelections']
                : ['openDirectory', 'createDirectory'],
          };
          const window = this.window();
          const picked = savedDirectory
            ? { canceled: false, filePaths: [savedDirectory] }
            : window
              ? await dialog.showOpenDialog(window, options)
              : await dialog.showOpenDialog(options);
          if (picked.canceled || this.owner !== owner || this.host.instanceId !== epoch)
            return { ok: false, code: 'cancelled' };
          if (picked.filePaths.length) {
            this.pickerDirectories[direction] =
              direction === 'upload' ? dirname(picked.filePaths[0]!) : picked.filePaths[0]!;
            if (!savedDirectory)
              void this.settings?.ui
                .set('pickers', {
                  kind: 'pickers',
                  [direction]: this.pickerDirectories[direction],
                })
                .catch(() => undefined);
          }
          hostCommand = {
            method: 'files.grant',
            params: { direction, paths: picked.filePaths },
          };
        }
        const runtime = this.host.instanceId ?? '';
        const requestOwner = this.owner;
        const request = this.host.workspaceCall(this.owner, hostCommand);
        try {
          const value = await request.result;
          if (requestOwner !== this.owner || runtime !== this.host.instanceId)
            return { ok: false, code: 'runtime_unavailable' };
          if (value.kind === 'connectionAccess')
            this.acceptConnectionAccess(runtime, requestOwner, value.state);
          if (value.kind === 'transfers' || value.kind === 'transferPage')
            for (const task of value.tasks) {
              if ((this.transferTasks.get(task.id)?.revision ?? -1) <= task.revision) {
                this.transferTasks.set(task.id, task);
                this.transferNotifications?.observe(task, true);
              }
            }
          if (value.kind === 'sessions')
            for (const item of value.sessions) this.eventNotifications?.session(item, true);
          return { ok: true, value };
        } catch (error) {
          return {
            ok: false,
            code: error instanceof BackendError ? error.code : 'runtime_unavailable',
            requestId: request.id,
            runtimeId: runtime,
          };
        }
      });
    ipcMain.on('workspace:transfer-feedback', (event, value: unknown) => {
      if (
        !this.allowed(event) ||
        !validTransferFeedback(value) ||
        value.runtimeId !== this.host.instanceId
      )
        return;
      const current = new Set([...this.transferTasks.values()].map(transferFeedbackKey));
      this.transferNotifications?.report(value.keys.filter((key) => current.has(key)));
    });
    ipcMain.on('workspace:terminal-port', (event, id: unknown) => {
      if (this.allowed(event) && typeof id === 'string') void this.port(event, id);
    });
    ipcMain.on('workspace:ui', (event, value: unknown) => {
      if (!this.allowed(event) || !value || typeof value !== 'object') return;
      const v = value as Partial<WorkspaceUiState>;
      if (
        typeof v.dirty === 'boolean' &&
        typeof v.modalOpen === 'boolean' &&
        typeof v.active === 'boolean' &&
        (typeof v.activeSessionId === 'string' || v.activeSessionId === null)
      )
        this.ui = v as WorkspaceUiState;
    });
    ipcMain.handle('workspace:copy', (event, text: unknown) => {
      if (
        !this.allowed(event) ||
        typeof text !== 'string' ||
        Buffer.byteLength(text) > 4 * 1024 * 1024
      )
        throw Error('invalid_input');
      return clipboard.writeText(text);
    });
    ipcMain.handle('workspace:clipboard', async (event) => {
      if (!this.allowed(event)) throw Error('invalid_sender');
      const text = await clipboard.readText();
      if (Buffer.byteLength(text) > 4 * 1024 * 1024) throw Error('input_backpressure');
      return text;
    });
    ipcMain.on('workspace:hide', (event) => {
      if (this.allowed(event)) this.window()?.hide();
    });
  }
  setTransferNotifications(
    service: SystemNotificationService,
    feedback?: NotificationFeedbackCoordinator,
    planned = () => false,
  ) {
    if (feedback)
      this.eventNotifications = new WorkspaceNotificationCoordinator({
        runtime: () => this.host.instanceId ?? '',
        locale: () => this.appearance.locale,
        feedback,
        planned,
      });
    this.transferNotifications = new TransferNotificationCoordinator({
      runtime: () => this.host.instanceId ?? '',
      locale: () => this.appearance.locale,
      focused: () => this.window()?.isFocused() ?? false,
      publish: (intent, cause) => {
        const target = this.transferNotifications?.target(intent.eventId);
        return feedback && target
          ? feedback.publish({
              ...intent,
              target: {
                kind: 'transfer',
                runtimeId: target.runtimeId,
                connectionId: target.connectionId,
                batchId: target.batchId,
                phase: target.phase,
              },
              runtimeId: target.runtimeId,
              connectionId: target.connectionId,
              cause,
            })
          : service.publish(intent);
      },
    });
  }
  transferNotificationTarget(id: string) {
    return this.transferNotifications?.target(id);
  }
  get activeFileOperationCount() {
    return [...this.operations.values()].filter(
      (op) => op.kind === 'files' && op.phase === 'working',
    ).length;
  }
  get activeTransferCount() {
    return [...this.transferTasks.values()].filter((t) =>
      ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(t.phase),
    ).length;
  }
  async cancelTransfers() {
    const tasks = [...this.transferTasks.values()].filter((t) =>
      ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(t.phase),
    );
    for (const task of tasks)
      await this.host.workspaceCall(this.owner, {
        method: 'transfers.cancel',
        params: { id: task.id },
      }).result;
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      let cursor: string | null = null,
        requestId: string | undefined,
        active = false;
      do {
        const reply: WorkspaceReply = await this.host.workspaceCall(this.owner, {
          method: 'transfers.list',
          params: { requestId, cursor: cursor ?? undefined },
        }).result;
        if (reply.kind !== 'transferPage') throw new Error('protocol_mismatch');
        active ||= reply.tasks.some((t) =>
          ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(t.phase),
        );
        cursor = reply.next;
        requestId = reply.requestId;
      } while (cursor !== null);
      if (!active) return;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error('transfer_cancel_timeout');
  }
  get activeCount() {
    return [...this.sessions.values()].filter(terminalIsActive).length;
  }
  command(command: 'new' | 'close' | 'find') {
    this.window()?.webContents.send('workspace:command', command);
  }
  context(appearance: Appearance) {
    this.appearance = appearance;
    if (this.host.instanceId && this.attachedRuntime === this.host.instanceId)
      void this.host
        .workspaceCall(this.owner, {
          method: 'workspace.context',
          params: { locale: appearance.locale, theme: appearance.theme },
        })
        .result.catch(() => undefined);
  }
  private ensure(): Promise<void> {
    if (this.attachedRuntime === this.host.instanceId && this.attachedRuntime)
      return Promise.resolve();
    if (this.attaching) return this.attaching;
    const runtime = this.host.instanceId,
      owner = this.owner;
    if (!runtime) return Promise.reject(new BackendError('runtime_unavailable'));
    const pending = (async () => {
      const result = await this.host
        .workspaceCall(owner, {
          method: 'workspace.negotiate',
          params: { version: workspaceProtocolVersion },
        })
        .result.catch((error: unknown) => {
          if (
            error instanceof BackendError &&
            error.code === 'timeout' &&
            this.host.instanceId === runtime
          ) {
            this.lateNegotiationOwners.add(owner);
            if (this.lateNegotiationOwners.size > 256)
              this.lateNegotiationOwners.delete(this.lateNegotiationOwners.values().next().value!);
          }
          throw error;
        });
      if (result.kind !== 'ready' || result.version !== workspaceProtocolVersion)
        throw new BackendError('protocol_mismatch');
      await this.completeAttachment(runtime, owner, result.connectionAccess);
    })();
    this.attaching = pending;
    void pending
      .finally(() => {
        if (this.attaching === pending) this.attaching = null;
      })
      .catch(() => undefined);
    return pending;
  }
  private acceptConnectionAccess(runtime: string, owner: string, state: ConnectionAccessState) {
    if (this.host.instanceId !== runtime || this.owner !== owner) return;
    if (
      this.connectionAccess.runtimeId === runtime &&
      (this.connectionAccess.state?.revision ?? -1) >= state.revision
    )
      return;
    this.connectionAccess = { runtimeId: runtime, state };
    this.window()?.webContents.send('workspace:connection-access-changed', this.connectionAccess);
  }
  private async completeAttachment(runtime: string, owner: string, access: ConnectionAccessState) {
    if (this.host.instanceId !== runtime) throw new BackendError('runtime_unavailable');
    if (this.owner !== owner) {
      await this.host
        .workspaceCall(owner, { method: 'workspace.detach', params: {} })
        .result.catch(() => undefined);
      throw new BackendError('runtime_unavailable');
    }
    this.attachedRuntime = runtime;
    this.acceptConnectionAccess(runtime, owner, access);
    if (this.rendererReloaded) {
      this.rendererReloaded = false;
      this.window()?.webContents.send('workspace:event', {
        jsonrpc: '2.0',
        method: 'workspace.changed',
        params: { runtimeId: runtime, owner, event: { kind: 'detached' } },
      });
    }
    await this.host.workspaceCall(owner, {
      method: 'workspace.context',
      params: { locale: this.appearance.locale, theme: this.appearance.theme },
    }).result;
    // Independent bootstrap returns immediately; it never queues the caller behind Keychain.
    void this.host
      .workspaceCall(owner, { method: 'connections.initialize', params: {} })
      .result.then((reply) => {
        if (reply.kind === 'connectionAccess')
          this.acceptConnectionAccess(runtime, owner, reply.state);
      })
      .catch(() => undefined);
  }
  async fileEditorSession(id: string) {
    await this.ensure();
    const reply = await this.host.workspaceCall(this.owner, {
      method: 'terminals.list',
      params: {},
    }).result;
    if (reply.kind !== 'sessions') throw new BackendError('invalid_response');
    const session = reply.sessions.find((s) => s.id === id && s.phase === 'running');
    if (!session) throw new BackendError('disconnected');
    return { connectionId: session.connectionId, groupId: session.groupId };
  }
  async beginFileEditor(
    command: Extract<import('@contracts').FileEditCommand, { action: 'open' }>,
  ) {
    await this.ensure();
    const owner = this.owner;
    const runtime = this.host.instanceId;
    const result = await this.host.call({ owner, method: 'files.edit', params: command }, 120000)
      .result;
    if (
      this.host.instanceId !== runtime ||
      result.kind !== 'workspace' ||
      result.result.kind !== 'fileEdit'
    )
      throw new BackendError('runtime_unavailable');
    return result.result.edit;
  }
  async navigation() {
    const owner = this.owner;
    const attachedRuntime = this.attachedRuntime;
    this.rendererReloaded ||= this.activeCount > 0;
    this.owner = randomUUID();
    this.attachedRuntime = null;
    this.attaching = null;
    this.closePorts();
    this.sessions.clear();
    this.operations.clear();
    this.transferTasks.clear();
    this.transferNotifications?.reset();
    this.eventNotifications?.reset();
    this.ui = {
      dirty: false,
      modalOpen: false,
      activeSessionId: null,
      active: true,
    };
    if (this.host.instanceId && attachedRuntime === this.host.instanceId)
      await this.host
        .workspaceCall(owner, { method: 'workspace.detach', params: {} })
        .result.catch(() => undefined);
  }
  cancelPrompts() {
    for (const o of this.operations.values())
      if (o.kind === 'credential' && o.phase === 'working')
        void this.host
          .workspaceCall(this.owner, {
            method: 'operations.cancel',
            params: { id: o.id },
          })
          .result.catch(() => undefined);
  }
  private closePorts() {
    for (const { port } of this.ports.values()) port.close();
    this.ports.clear();
    this.pending.clear();
    this.retired.clear();
  }
  private receive(frame: TerminalFrame) {
    if (frame.owner !== this.owner || this.retired.has(frame.session)) return;
    const bound = this.ports.get(frame.session);
    if (bound) {
      if (bound.runtime === this.host.instanceId) this.deliver(bound.port, frame);
      return;
    }
    const pending = this.pending.get(frame.session) ?? [];
    if (pending.reduce((n, f) => n + f.bytes.length, 0) + frame.bytes.length > 1024 * 1024) {
      void this.host
        .workspaceCall(this.owner, {
          method: 'terminals.close',
          params: { sessionId: frame.session },
        })
        .result.catch(() => undefined);
      return;
    }
    pending.push(frame);
    this.pending.set(frame.session, pending);
  }
  private deliver(port: MessagePortMain, frame: TerminalFrame) {
    if (frame.kind === 3)
      port.postMessage({
        kind: 'output',
        sequence: frame.sequence,
        bytes: new Uint8Array(frame.bytes),
      });
    else if (frame.kind === 4)
      port.postMessage({
        kind: 'inputAck',
        sequence: frame.sequence,
        length: frame.bytes.readUInt32BE(),
      });
    else if (frame.kind === 5) port.postMessage({ kind: 'end', sequence: frame.sequence });
  }
  private async port(event: IpcMainEvent, id: string) {
    try {
      await this.ensure();
      if (!this.allowed(event) || !this.host.instanceId) return;
      if (!this.sessions.has(id)) {
        const reply = await this.host.workspaceCall(this.owner, {
          method: 'terminals.list',
          params: {},
        }).result;
        if (reply.kind !== 'sessions' || !reply.sessions.some((s) => s.id === id)) return;
        for (const s of reply.sessions) this.sessions.set(s.id, s);
      }
      if (this.ports.has(id)) return;
      const { port1, port2 } = new MessageChannelMain();
      const owner = this.owner,
        runtime = this.host.instanceId;
      this.ports.set(id, { port: port1, runtime });
      port1.on('message', ({ data }: { data: unknown }) => {
        if (this.owner !== owner || this.host.instanceId !== runtime) return;
        try {
          if (!data || typeof data !== 'object') throw Error();
          const value = data as {
            kind?: unknown;
            sequence?: unknown;
            bytes?: unknown;
            length?: unknown;
          };
          if (!Number.isSafeInteger(value.sequence) || Number(value.sequence) <= 0) throw Error();
          if (
            value.kind === 'input' &&
            value.bytes instanceof Uint8Array &&
            value.bytes.length > 0 &&
            value.bytes.length <= 32768
          ) {
            if (this.sessions.get(id)?.phase !== 'running') throw Error();
            this.host.terminalFrame({
              kind: 1,
              owner,
              session: id,
              sequence: Number(value.sequence),
              bytes: Buffer.from(value.bytes),
            });
          } else if (
            value.kind === 'credit' &&
            Number.isInteger(value.length) &&
            Number(value.length) > 0 &&
            Number(value.length) <= 32768
          ) {
            const bytes = Buffer.alloc(4);
            bytes.writeUInt32BE(Number(value.length));
            this.host.terminalFrame({
              kind: 2,
              owner,
              session: id,
              sequence: Number(value.sequence),
              bytes,
            });
          } else throw Error();
        } catch {
          port1.postMessage({ kind: 'error', code: 'stream_unavailable' });
          void this.host
            .workspaceCall(owner, {
              method: 'terminals.close',
              params: { sessionId: id },
            })
            .result.catch(() => undefined);
        }
      });
      port1.on('close', () => {
        if (this.ports.get(id)?.port === port1) this.ports.delete(id);
        this.pending.delete(id);
        this.retired.add(id);
        if (this.retired.size > 512) this.retired.delete(this.retired.values().next().value!);
        if (this.sessions.has(id) && !terminalIsActive(this.sessions.get(id)!))
          this.sessions.delete(id);
      });
      port1.start();
      event.senderFrame?.postMessage(
        'workspace:terminal-port',
        { type: 'vigilshell:terminal-port', sessionId: id, runtimeId: runtime },
        [port2],
      );
      for (const frame of this.pending.get(id) ?? []) this.deliver(port1, frame);
      this.pending.delete(id);
    } catch {
      /* A dead owner cannot receive a usable terminal port. */
    }
  }
}
