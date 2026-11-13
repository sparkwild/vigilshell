import type { UpdateProgress, UpdateSnapshot } from '@contracts';
import type { AvailableRelease } from '@contracts';
import type { PendingUpdate } from './cache';
import { UpdateService, type DiscoveryOptions } from './service';

export type InstallUpdate = (prepare: () => Promise<void>, commit: () => void) => Promise<boolean>;
export interface UpdateEngine {
  cache: { load(): Promise<PendingUpdate | undefined>; clear(): Promise<void> };
  download(release: AvailableRelease, progress: (value: UpdateProgress) => void): Promise<void>;
  inspect(release: AvailableRelease): Promise<boolean>;
  prepare(): Promise<void>;
  commit(): void;
  cancel(): boolean;
  stop(): void;
}
export class ManagedUpdates extends UpdateService {
  private operation?: Promise<UpdateSnapshot>;
  private cancelled = false;
  private restoring?: Promise<void>;
  private checking?: Promise<UpdateSnapshot>;
  private pending?: { release: AvailableRelease; failed: boolean };
  constructor(
    current: string,
    platform: string,
    arch: string,
    fetcher: typeof fetch,
    private engine: UpdateEngine | undefined,
    private installUpdate: InstallUpdate,
    discovery: DiscoveryOptions = {},
  ) {
    super(current, platform, arch, fetcher, {
      ...discovery,
      inspect: engine ? (release) => engine.inspect(release) : undefined,
    });
    this.snapshot = { ...this.snapshot, canDownload: false };
  }
  protected override publish(snapshot: UpdateSnapshot) {
    super.publish({ ...snapshot, canDownload: !!this.engine && snapshot.canDownload === true });
  }
  restore() {
    if (!this.restoring)
      this.restoring = this.restoreCache().finally(() => {
        this.restoring = undefined;
      });
    return this.restoring;
  }
  private async restoreCache() {
    await this.initialized;
    if (!this.engine) return;
    try {
      const pending = await this.engine.cache.load();
      if (pending) {
        this.pending = { release: pending.release, failed: pending.installing };
        this.publish({
          phase: pending.installing ? 'installError' : 'ready',
          release: pending.release,
          canDownload: true,
        });
      }
    } catch {
      await this.engine.cache.clear();
      this.publish({ phase: 'downloadError', release: null });
    }
  }
  override check(origin: 'manual' | 'background' = 'manual'): Promise<UpdateSnapshot> {
    if (this.operation) return this.operation;
    if (this.checking) return this.checking;
    if (this.restoring) return this.restoring.then(() => this.check(origin));
    // A verified package remains the selected update until installed or replaced explicitly.
    if (this.pending) return Promise.resolve(this.snapshot);
    this.checking = super.check(origin).finally(() => {
      this.checking = undefined;
    });
    return this.checking;
  }
  download(): Promise<UpdateSnapshot> {
    if (this.restoring) return this.restoring.then(() => this.download());
    if (this.operation) return this.operation;
    if (this.checking) return this.checking.then(() => this.download());
    const release = this.snapshot.release;
    if (
      !this.engine ||
      !this.snapshot.canDownload ||
      !release ||
      !['available', 'downloadError'].includes(this.snapshot.phase)
    )
      return Promise.resolve(this.snapshot);
    this.cancelled = false;
    this.publish({ ...this.snapshot, phase: 'downloading', progress: undefined });
    this.operation = (async () => {
      try {
        await this.engine!.download(release, (progress: UpdateProgress) => {
          if (!this.cancelled)
            this.publish({ phase: 'downloading', release, progress, canDownload: true });
        });
        if (!this.cancelled) this.pending = { release, failed: false };
        this.publish({ phase: this.cancelled ? 'available' : 'ready', release, canDownload: true });
      } catch (error) {
        this.publish({
          phase: this.cancelled ? 'available' : 'downloadError',
          release,
          canDownload: true,
          issue:
            error instanceof Error && error.message === 'insufficient_space'
              ? 'insufficientSpace'
              : undefined,
        });
      }
      return this.snapshot;
    })().finally(() => {
      this.operation = undefined;
    });
    return this.operation;
  }
  cancel() {
    if (this.snapshot.phase === 'downloading') {
      if (this.engine?.cancel()) this.cancelled = true;
    }
    return this.snapshot;
  }
  install(): Promise<UpdateSnapshot> {
    if (this.restoring) return this.restoring.then(() => this.install());
    if (this.operation) return this.operation;
    if (
      !this.engine ||
      !this.snapshot.release ||
      !['ready', 'installError'].includes(this.snapshot.phase)
    )
      return Promise.resolve(this.snapshot);
    this.operation = (async () => {
      try {
        this.publish({ ...this.snapshot, phase: 'preparing' });
        await this.installUpdate(
          async () => {
            this.publish({ ...this.snapshot, phase: 'preparing' });
            await this.engine!.prepare();
          },
          () => {
            this.publish({ ...this.snapshot, phase: 'installing' });
            this.engine!.commit();
          },
        );
        if (this.snapshot.phase !== 'installing')
          this.publish({ ...this.snapshot, phase: 'ready' });
      } catch (error) {
        const message = error instanceof Error ? error.message : '';
        if (['invalid_cache', 'missing_update'].includes(message)) {
          this.pending = undefined;
          await this.engine!.cache.clear().catch((error) =>
            console.error('Update cache cleanup:', error.message),
          );
          this.publish({
            phase: 'downloadError',
            release: this.snapshot.release,
            canDownload: true,
          });
          return this.snapshot;
        }
        const issue =
          message === 'insufficient_space'
            ? ('insufficientSpace' as const)
            : message === 'read_only_application'
              ? ('readOnlyLocation' as const)
              : [
                    'ui_state_not_saved',
                    'preferences_not_saved',
                    'file_editing_busy',
                    'runtime_unavailable',
                  ].includes(message)
                ? ('unsavedState' as const)
                : undefined;
        this.publish({ ...this.snapshot, phase: 'installError', issue });
      }
      return this.snapshot;
    })().finally(() => {
      this.operation = undefined;
    });
    return this.operation;
  }
  override stop() {
    this.engine?.stop();
    super.stop();
  }
}
