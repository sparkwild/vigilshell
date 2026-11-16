import { autoUpdater } from 'electron';
import electronUpdater, { type MacUpdater } from 'electron-updater';
import { createServer, type Server } from 'node:http';
import { createReadStream, constants } from 'node:fs';
import { access, mkdir, statfs, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { randomUUID } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import type { AvailableRelease, UpdateProgress } from '@contracts';
import { UpdateCache, verifiedInfo } from './cache';
import { NativeUpdateUncertain } from './installation';
import { requiredInstallSpace } from './install-space';

export class MacUpdateEngine {
  readonly cache: UpdateCache;
  private updater?: MacUpdater;
  private token?: InstanceType<typeof electronUpdater.CancellationToken>;
  private stopping = false;
  private finalizing = false;
  constructor(
    private directory: string,
    private current: string,
    private arch: string,
    private development: boolean,
    private feed: (tag: string) => string,
  ) {
    this.cache = new UpdateCache(directory, current, arch);
  }
  private async getUpdater() {
    if (this.stopping) throw Error('updates_stopped');
    if (!this.updater) {
      await mkdir(this.directory, { recursive: true, mode: 0o700 });
      const configuration = join(this.directory, 'engine.yml');
      await writeFile(
        configuration,
        JSON.stringify({
          updaterCacheDirName: this.development ? 'vigilshell-dev-updater' : 'vigilshell-updater',
        }),
        { mode: 0o600 },
      );
      this.updater = new electronUpdater.MacUpdater();
      this.updater.updateConfigPath = configuration;
      this.updater.autoDownload = false;
      this.updater.autoInstallOnAppQuit = false;
      this.updater.allowDowngrade = false;
      this.updater.allowPrerelease = false;
      this.updater.disableDifferentialDownload = true;
      this.updater.on('error', (error) => console.error('Update engine:', error.message));
    }
    return this.updater;
  }
  async inspect(release: AvailableRelease): Promise<boolean> {
    const updater = await this.getUpdater();
    updater.setFeedURL({ provider: 'generic', url: this.feed(release.tag) });
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const result = await Promise.race([
        updater.checkForUpdates(),
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(Error('metadata_timeout')), 15000);
        }),
      ]);
      if (!result?.isUpdateAvailable) return false;
      verifiedInfo(result.updateInfo, release, this.arch);
      return true;
    } finally {
      clearTimeout(timer);
    }
  }
  async download(release: AvailableRelease, progress: (value: UpdateProgress) => void) {
    const token = new electronUpdater.CancellationToken();
    this.token = token;
    let updater: MacUpdater | undefined;
    let downloadedFile: string | undefined;
    let onProgress: ((value: UpdateProgress) => void) | undefined;
    const onDownloaded = (event: { downloadedFile: string }) => {
      downloadedFile = event.downloadedFile;
    };
    try {
      updater = await this.getUpdater();
      if (token.cancelled || this.stopping) throw Error('cancelled');
      updater.setFeedURL({ provider: 'generic', url: this.feed(release.tag) });
      const result = await token.createPromise<Awaited<ReturnType<MacUpdater['checkForUpdates']>>>(
        (resolve, reject, onCancel) => {
          const timer = setTimeout(() => reject(Error('metadata_timeout')), 15000);
          onCancel(() => clearTimeout(timer));
          updater!.checkForUpdates().then(
            (value) => {
              clearTimeout(timer);
              resolve(value);
            },
            (error) => {
              clearTimeout(timer);
              reject(error);
            },
          );
        },
      );
      if (!result?.isUpdateAvailable) throw Error('updater_unavailable');
      const info = verifiedInfo(result.updateInfo, release, this.arch);
      const space = await statfs(this.directory);
      if (space.bavail * space.bsize < info.files[0]!.size! * 4) throw Error('insufficient_space');
      if (token.cancelled || this.stopping) throw Error('cancelled');
      onProgress = (value: UpdateProgress) => {
        const total = info.files[0]!.size!;
        const transferred = Math.max(
          0,
          Math.min(total, Number.isFinite(value.transferred) ? value.transferred : 0),
        );
        progress({ transferred, total, percent: (transferred * 100) / total });
      };
      updater.on('download-progress', onProgress);
      updater.once('update-downloaded', onDownloaded);
      await updater.downloadUpdate(token);
      if (token.cancelled || this.stopping) throw Error('cancelled');
      if (!downloadedFile) throw Error('invalid_download');
      this.finalizing = true;
      progress({ transferred: info.files[0]!.size!, total: info.files[0]!.size!, percent: 100 });
      await this.cache.save(release, info, downloadedFile);
    } finally {
      if (onProgress) updater?.off('download-progress', onProgress);
      updater?.off('update-downloaded', onDownloaded);
      token.dispose();
      this.token = undefined;
      this.finalizing = false;
    }
  }
  cancel() {
    if (!this.token || this.finalizing) return false;
    this.token?.cancel();
    return true;
  }
  /** Rehydrate from the verified local ZIP so installing after restart works offline. */
  async prepare() {
    const pending = await this.cache.load().catch(() => {
      throw Error('invalid_cache');
    });
    if (!pending) throw Error('missing_update');
    const destination = dirname(resolve(process.resourcesPath, '../..'));
    try {
      await access(destination, constants.W_OK);
    } catch {
      throw Error('read_only_application');
    }
    const summary = await promisify(execFile)('/usr/bin/unzip', ['-Z', '-t', this.cache.archive], {
      timeout: 15000,
      maxBuffer: 32768,
    });
    const required = requiredInstallSpace(summary.stdout, pending.info.files[0]!.size!);
    for (const location of [this.directory, destination]) {
      const space = await statfs(location);
      if (space.bavail * space.bsize < required) throw Error('insufficient_space');
    }
    const updater = await this.getUpdater();
    const prefix = `/${randomUUID()}`;
    const filename = pending.info.files[0]!.url;
    const server = createServer((request, response) => {
      const path = new URL(request.url!, 'http://127.0.0.1').pathname;
      if (path === `${prefix}/latest-mac.yml`) {
        response.writeHead(200, { 'Content-Type': 'application/yaml' });
        response.end(JSON.stringify(pending.info));
      } else if (path === `${prefix}/${filename}`) {
        response.writeHead(200, {
          'Content-Type': 'application/zip',
          'Content-Length': pending.info.files[0]!.size!,
        });
        const stream = createReadStream(this.cache.archive);
        stream.on('error', () => response.destroy());
        stream.pipe(response);
      } else {
        response.writeHead(404);
        response.end();
      }
    });
    try {
      await new Promise<void>((resolve, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', resolve);
      });
      updater.setFeedURL({
        provider: 'generic',
        url: `http://127.0.0.1:${(server.address() as { port: number }).port}${prefix}/`,
      });
      if (!(await updater.checkForUpdates())?.isUpdateAvailable) throw Error('unsupported_update');
      await updater.downloadUpdate();
    } finally {
      await this.close(server);
    }
    await this.cache.write({ ...pending, installing: true });
    try {
      await new Promise<void>((resolve, reject) => {
        const done = (error?: Error) => {
          clearTimeout(timer);
          autoUpdater.off('error', failed);
          autoUpdater.off('update-downloaded', ready);
          if (error) reject(error);
          else resolve();
        };
        const failed = (error: Error) => done(error);
        const ready = () => done();
        const timer = setTimeout(
          () => done(new NativeUpdateUncertain('native_prepare_timeout')),
          90000,
        );
        autoUpdater.once('error', failed);
        autoUpdater.once('update-downloaded', ready);
        try {
          autoUpdater.checkForUpdates();
        } catch (error) {
          done(error as Error);
        }
      });
    } catch (error) {
      if (!(error instanceof NativeUpdateUncertain))
        await this.cache.write({ ...pending, installing: false });
      throw error;
    }
  }
  commit() {
    if (!this.updater) throw Error('updater_unavailable');
    this.updater.quitAndInstall();
  }
  stop() {
    this.stopping = true;
    this.cancel();
  }
  private close(server: Server) {
    return new Promise<void>((resolve) => server.close(() => resolve()));
  }
}
