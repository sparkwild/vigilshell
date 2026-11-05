import { dialog, ipcMain, type BrowserWindow } from 'electron';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { lstat, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import type {
  FileEditCommand,
  FileEditRemote,
  FileEditorReply,
  FileEditorRequest,
} from '@contracts';
import { contractValidator } from '../../contracts/validation';
import { workspaceProtocolVersion } from '../../contracts/generated/protocol-versions';
import type { BackendClient } from '../backend/client';
import type { DesktopState } from '../platform/desktop-state';
import type { WorkspaceIpc } from '../ipc/workspace';
import { FileEditingService } from './service';
const execute = promisify(execFile);
export function registerFileEditors(options: {
  host: BackendClient;
  workspace: WorkspaceIpc;
  state: DesktopState;
  dataDirectory: string;
  window(): BrowserWindow | undefined;
  trusted(event: Electron.IpcMainInvokeEvent): boolean;
}) {
  const valid = contractValidator<FileEditorRequest>('FileEditorRequest');
  let attached: string | null = null;
  async function ensure() {
    if (attached === options.host.instanceId && attached) return;
    const runtime = options.host.instanceId;
    if (!runtime) throw Error('runtime_unavailable');
    const reply = await options.host.workspaceCall(service.owner, {
      method: 'workspace.negotiate',
      params: { version: workspaceProtocolVersion },
    }).result;
    if (options.host.instanceId !== runtime || reply.kind !== 'ready')
      throw Error('runtime_unavailable');
    attached = runtime;
  }
  async function chooseApplication(save: boolean) {
    if (process.platform !== 'darwin') throw Error('files_unsupported');
    const args: Electron.OpenDialogOptions = {
      properties: ['openFile'],
      filters: [{ name: 'Applications', extensions: ['app'] }],
      defaultPath: '/Applications',
    };
    const parent = options.window();
    const picked = parent
      ? await dialog.showOpenDialog(parent, args)
      : await dialog.showOpenDialog(args);
    if (picked.canceled || !picked.filePaths[0]) return null;
    const application = await validateApplication(picked.filePaths[0]);
    if (save) {
      const current = options.state.getSnapshot().preferences;
      if (!current) throw Error('runtime_unavailable');
      const result = await options.state.update(
        {
          expectedRevision: current.revision,
          patch: { fileEditorApplication: application },
          base: { fileEditorApplication: current.fileEditorApplication },
        },
        true,
      );
      if (result.error || result.save.phase !== 'idle')
        throw Error(result.error ?? 'result_unknown');
    }
    return application;
  }
  const service: FileEditingService = new FileEditingService(
    join(options.dataDirectory, 'file-editing'),
    {
      runtime: () => options.host.instanceId,
      session: (id) => options.workspace.fileEditorSession(id),
      application: () => options.state.getSnapshot().preferences?.fileEditorApplication ?? '',
      chooseApplication,
      launch: async (application, file) => {
        if (process.platform !== 'darwin') throw Error('files_unsupported');
        await execute('/usr/bin/open', ['-a', await validateApplication(application), '--', file], {
          timeout: 10000,
        });
      },
      begin: async (command) => {
        await ensure();
        return options.workspace.beginFileEditor(command);
      },
      call: (command: FileEditCommand) => {
        const runtimeId = options.host.instanceId ?? '';
        const request = options.host.call(
          { owner: service.owner, method: 'files.edit', params: command },
          120000,
        );
        const result = request.result.then((r): FileEditRemote => {
          if (r.kind !== 'workspace' || r.result.kind !== 'fileEdit')
            throw Error('invalid_response');
          return r.result.edit;
        });
        void result.catch(() => undefined);
        return { id: request.id, runtimeId, result };
      },
      reconcile: async (requestId, runtimeId) => {
        await ensure();
        return options.host.workspaceCall(service.owner, {
          method: 'connections.reconcile',
          params: { requestId, runtimeId },
        }).result;
      },
      chooseExport: async (name) => {
        const parent = options.window(),
          args = { defaultPath: name };
        const picked = parent
          ? await dialog.showSaveDialog(parent, args)
          : await dialog.showSaveDialog(args);
        return picked.canceled ? null : (picked.filePath ?? null);
      },
    },
  );
  const ready = service.restore();
  service.on('changed', () => options.window()?.webContents.send('file-editors:changed'));
  service.on('show', () => options.window()?.webContents.send('file-editors:show'));
  options.host.on('unavailable', () => {
    attached = null;
    void service.disconnect();
  });
  ipcMain.handle(
    'file-editors:request',
    async (event, value: unknown): Promise<FileEditorReply> => {
      if (!options.trusted(event) || !valid(value))
        return { ok: false, sessions: [], error: 'invalid_request' };
      try {
        await ready;
        switch (value.action) {
          case 'list':
            service.consumeShow();
            break;
          case 'show':
            service.show();
            break;
          case 'chooseEditor':
            await chooseApplication(true);
            break;
          case 'open':
            await service.open(value.sessionId, value.path, value.alternative);
            break;
          case 'retry':
            await service.retry(value.id);
            break;
          case 'reopen':
            await service.reopen(value.id);
            break;
          case 'recover':
            await service.recover(value.id, value.sessionId);
            break;
          case 'resolve':
            await service.resolve(value.id, value.choice, value.destination ?? null);
            break;
          case 'export':
            await service.export(value.id);
            break;
          case 'end':
            await service.end(value.id, value.discard);
            break;
        }
        return { ok: true, sessions: service.list(), error: null };
      } catch (error) {
        return {
          ok: false,
          sessions: service.list(),
          error: error instanceof Error ? error.message : 'files_unsupported',
        };
      }
    },
  );
  return service;
}
async function validateApplication(path: string) {
  const resolved = await realpath(path);
  if (!resolved.endsWith('.app') || !(await lstat(resolved)).isDirectory())
    throw Error('files_unsupported');
  const { stdout } = await execute(
    '/usr/bin/plutil',
    ['-extract', 'CFBundleIdentifier', 'raw', '-o', '-', join(resolved, 'Contents', 'Info.plist')],
    { timeout: 5000, maxBuffer: 4096 },
  );
  if (!stdout.trim()) throw Error('files_unsupported');
  return resolved;
}
