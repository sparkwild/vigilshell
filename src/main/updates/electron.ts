import { app, ipcMain, shell, net } from 'electron';
import { contractValidator } from '../../contracts/validation';
import type { ApplicationRequest } from '@contracts';
import { join } from 'node:path';
import { DiscoveryStore } from './discovery-store';
import { ManagedUpdates, type InstallUpdate } from './managed';
import { MacUpdateEngine } from './mac-engine';
import { updateSource } from './source';
export function registerApplication(
  allowed: (event: Electron.IpcMainInvokeEvent) => boolean,
  options: { development: boolean; packaged: boolean; install: InstallUpdate },
) {
  const source = updateSource(options.development, process.env);
  const engine =
    options.packaged && process.platform === 'darwin' && source.downloadable
      ? new MacUpdateEngine(
          join(app.getPath('userData'), 'updates'),
          app.getVersion(),
          process.arch,
          options.development,
          source.feed,
        )
      : undefined;
  const fetcher: typeof fetch = (input, init) =>
    net.fetch(input instanceof URL ? input.href : input, init);
  const updates = new ManagedUpdates(
    app.getVersion(),
    process.platform,
    process.arch,
    fetcher,
    engine,
    options.install,
    {
      releases: source.releases,
      notes: source.notes,
      store: new DiscoveryStore(
        join(app.getPath('userData'), 'updates', 'discovery.json'),
        `${source.releases}|${process.platform}|${process.arch}`,
        app.getVersion(),
      ),
    },
  );
  void app
    .whenReady()
    .then(() => updates.restore())
    .catch((error) => console.error('Update cache:', error.message));
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
      case 'download':
        return updates.download();
      case 'cancel':
        return updates.cancel();
      case 'install':
        return updates.install();
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
