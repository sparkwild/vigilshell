import { app, ipcMain, shell } from 'electron';
import { contractValidator } from '../../contracts/validation';
import type { ApplicationRequest } from '@contracts';
import { UpdateService } from './service';
export function registerApplication(allowed: (event: Electron.IpcMainInvokeEvent) => boolean) {
  const updates = new UpdateService(app.getVersion(), process.platform, process.arch);
  const validate = contractValidator<ApplicationRequest>('ApplicationRequest');
  ipcMain.handle('application:request', async (event, request: unknown) => {
    if (!allowed(event) || !validate(request)) throw Error('invalid_request');
    switch (request.action) {
      case 'info':
        return {
          name: 'VigilShell',
          version: app.getVersion(),
        };
      case 'state':
        return updates.snapshot;
      case 'check':
        return updates.check();
      case 'project':
        return shell.openExternal('https://github.com/sparkwild/vigilshell');
      case 'issues':
        return shell.openExternal('https://github.com/sparkwild/vigilshell/issues');
      case 'openRelease':
        if (updates.snapshot.release)
          return shell.openExternal(
            `https://github.com/sparkwild/vigilshell/releases/tag/${encodeURIComponent(updates.snapshot.release.tag)}`,
          );
    }
  });
  return updates;
}
