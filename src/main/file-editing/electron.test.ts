import { EventEmitter } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { BrowserWindow } from 'electron';
import { expect, it, vi } from 'vitest';
import type { FileEditorReply } from '@contracts';
import type { BackendClient } from '../backend/client';
import type { WorkspaceIpc } from '../ipc/workspace';
import type { DesktopState } from '../platform/desktop-state';
const fixture = vi.hoisted(() => ({
  handlers: new Map<string, (event: unknown, value: unknown) => Promise<FileEditorReply>>(),
}));
vi.mock('electron', () => ({
  dialog: {},
  ipcMain: {
    handle: (name: string, handler: (event: unknown, value: unknown) => Promise<FileEditorReply>) =>
      fixture.handlers.set(name, handler),
  },
}));
import { registerFileEditors } from './electron';
it('delivers a pending native-menu show request exactly once when the renderer lists editors', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'vigil-editor-ipc-'));
  const send = vi.fn();
  let window: BrowserWindow | undefined;
  const service = registerFileEditors({
    host: new EventEmitter() as BackendClient,
    workspace: {} as WorkspaceIpc,
    state: {} as DesktopState,
    dataDirectory: directory,
    trusted: () => true,
    window: () => window,
  });
  try {
    service.show();
    expect(send).not.toHaveBeenCalled();
    window = { webContents: { send } } as unknown as BrowserWindow;
    const request = fixture.handlers.get('file-editors:request')!;
    expect((await request({}, { action: 'list' })).ok).toBe(true);
    expect(send.mock.calls.filter(([name]) => name === 'file-editors:show')).toHaveLength(1);
    await request({}, { action: 'list' });
    expect(send.mock.calls.filter(([name]) => name === 'file-editors:show')).toHaveLength(1);
  } finally {
    await service.stop();
    await rm(directory, { recursive: true, force: true });
  }
});
