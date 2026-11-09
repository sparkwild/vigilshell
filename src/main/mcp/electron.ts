import { workspaceProtocolVersion } from '../../contracts/generated/protocol-versions';
import { app, clipboard, dialog, ipcMain, safeStorage, type BrowserWindow } from 'electron';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';
import type { AgentEvent, AgentOperation, AgentReply, AgentUiRequest } from '@contracts';
import { contractValidator } from '../../contracts/validation';
import type { McpUiResult } from '../../contracts/mcp';
import { BackendClient } from '../backend/client';
import type { NotificationFeedbackCoordinator } from '../notifications/feedback';
import { translate } from '../../shared/i18n/messages';
import { detectClients, detectionPorts } from './detection';
import { McpController } from './controller';
import { IdentityVerifier, type NativeIdentityBridge } from './identity';
import { AgentNotificationCoordinator } from './notifications';
import { authorizationReason } from './authorization-reason';

interface IdentityBridge extends NativeIdentityBridge {
  findApplication(bundleId: string, candidatePath?: string): string | undefined;
}
interface Options {
  host: BackendClient;
  bridgePath: string;
  trusted(event: Electron.IpcMainInvokeEvent): boolean;
  window(): BrowserWindow | undefined;
  locale(): 'en' | 'zh-CN';
  scrollback(): number;
  notifications: NotificationFeedbackCoordinator;
  tray(visible: boolean): void;
}
export function registerMcp(options: Options) {
  const owner = randomUUID();
  const notifications = new AgentNotificationCoordinator({
    locale: options.locale,
    runtime: () => options.host.instanceId,
    publish: (intent) => options.notifications.publish(intent),
  });
  let attached: string | null = null;
  let attaching: Promise<void> | null = null;
  let identity: IdentityBridge | undefined;
  const verifier = new IdentityVerifier({
    bridge: () =>
      (identity ??= createRequire(import.meta.url)(options.bridgePath) as IdentityBridge),
    visible: () => {
      const window = options.window();
      return !!window && !window.isDestroyed() && window.isVisible();
    },
    supported: () => process.platform === 'darwin',
    log: (value) => console.info(JSON.stringify(value)),
  });
  const valid = contractValidator<AgentUiRequest>('AgentUiRequest');
  const request = async (operation: AgentOperation): Promise<AgentReply> => {
    const runtime = options.host.instanceId;
    if (!runtime) throw new Error('runtime_unavailable');
    if (attached !== runtime) {
      if (!attaching) {
        const pending = options.host
          .workspaceCall(owner, {
            method: 'workspace.negotiate',
            params: { version: workspaceProtocolVersion },
          })
          .result.then((reply) => {
            if (options.host.instanceId !== runtime) throw new Error('runtime_unavailable');
            if (reply.kind !== 'ready' || reply.version !== workspaceProtocolVersion)
              throw new Error('protocol_mismatch');
            attached = runtime;
          })
          .finally(() => {
            if (attaching === pending) attaching = null;
          });
        attaching = pending;
      }
      await attaching;
    }
    const result = await options.host.request({
      owner,
      method: 'agent.request',
      params: { request: operation },
    });
    if (
      options.host.instanceId !== runtime ||
      result.kind !== 'workspace' ||
      result.result.kind !== 'agent'
    )
      throw new Error('invalid_response');
    return result.result.result;
  };
  const controller = new McpController({
    directory: async () => {
      const window = options.window();
      if (!window) return null;
      const result = await dialog.showOpenDialog(window, {
        properties: ['openDirectory', 'createDirectory'],
      });
      return result.canceled ? null : (result.filePaths[0] ?? null);
    },
    request,
    detect: () =>
      detectClients(
        detectionPorts((id, candidate) => {
          identity ??= createRequire(import.meta.url)(options.bridgePath) as IdentityBridge;
          return identity.findApplication(id, candidate);
        }),
      ),
    locale: options.locale,
    scrollback: options.scrollback,
    authorize: async (purpose, change) => {
      const runtime = options.host.instanceId;
      const reason = await authorizationReason(change, options.locale(), {
        request,
        connectionName: async (id) => {
          const reply = await options.host.workspaceCall(owner, {
            method: 'connections.get',
            params: { id },
          }).result;
          if (reply.kind !== 'connection' || reply.connection.id !== id)
            throw Error('invalid_response');
          return reply.connection.config.name;
        },
      });
      if (!runtime || options.host.instanceId !== runtime) throw Error('authorization_interrupted');
      return verifier.verify(reason, purpose);
    },
    seal: (token) => {
      if (!safeStorage.isEncryptionAvailable()) throw new Error('secure_storage_unavailable');
      return safeStorage.encryptString(token).toString('base64');
    },
    unseal: (value) => {
      if (!safeStorage.isEncryptionAvailable()) throw new Error('secure_storage_unavailable');
      return safeStorage.decryptString(Buffer.from(value, 'base64'));
    },
    copy: (value) => clipboard.writeText(value),
    changed: (event) => {
      const window = options.window();
      if (window && !window.isDestroyed()) window.webContents.send('mcp:changed', event);
    },
    tray: options.tray,
    cancelAuthorization: () => verifier.cancel(),
    serviceFailure: (relevant) => {
      void options.notifications.publish({
        eventId: `mcp-service:${options.host.instanceId}:${randomUUID()}`,
        runtimeId: options.host.instanceId ?? undefined,
        category: 'executionIssue',
        target: { kind: 'mcp' },
        title: translate('notification_mcp_failed', options.locale()),
        body: translate('notification_mcp_failed_body', options.locale()),
        stillRelevant: relevant,
      });
    },
    notify: (event) => notifications.observe(event),
  });
  ipcMain.handle('mcp:request', async (event, payload: unknown): Promise<McpUiResult> => {
    if (!options.trusted(event) || !valid(payload)) return { ok: false, code: 'invalid_request' };
    const result = await controller.ui(payload);
    return options.trusted(event) ? result : { ok: false, code: 'invalid_request' };
  });
  const changed = (event: AgentEvent) => {
    if (event.kind === 'context')
      notifications.observeContext(event.context.id, event.context.phase);
    controller.event(event);
    options.notifications.refresh();
  };
  const unavailable = () => {
    notifications.reset();
    attached = null;
    attaching = null;
    verifier.cancel();
    void controller.unavailable();
  };
  const maintenance = setInterval(() => {
    void controller.refresh().catch(() => undefined);
  }, 60_000);
  maintenance.unref();
  const negotiationRecovered = (runtime: string, recoveredOwner: string) => {
    if (options.host.instanceId !== runtime || recoveredOwner !== owner) return;
    attached = runtime;
  };
  options.host.on('negotiationRecovered', negotiationRecovered);
  options.host.on('agent', changed);
  options.host.on('unavailable', unavailable);
  app.once('will-quit', () => {
    notifications.reset();
    clearInterval(maintenance);
    verifier.cancel();
    ipcMain.removeHandler('mcp:request');
    options.host.off('negotiationRecovered', negotiationRecovered);
    options.host.off('agent', changed);
    options.host.off('unavailable', unavailable);
  });
  return controller;
}
