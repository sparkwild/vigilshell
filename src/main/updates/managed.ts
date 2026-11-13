import type { UpdateProgress, UpdateSnapshot } from '@contracts';
import type { AvailableRelease } from '@contracts';
import type { PendingUpdate } from './cache';
import { UpdateService } from './service';

export type InstallUpdate = (prepare: () => Promise<void>, commit: () => void) => Promise<boolean>;
export interface UpdateEngine {
  cache: { load(): Promise<PendingUpdate | undefined>; clear(): Promise<void> };
  download(release: AvailableRelease, progress: (value: UpdateProgress) => void): Promise<void>;
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
  ) {
    super(current, platform, arch, fetcher);
    this.snapshot = { ...this.snapshot, canDownload: !!engine };
  }
  protected override publish(snapshot: UpdateSnapshot) {
    super.publish({ ...snapshot, canDownload: !!this.engine });
  }
  restore() {
    if (!this.restoring)
      this.restoring = this.restoreCache().finally(() => {
        this.restoring = undefined;
      });
    return this.restoring;
  }
  private async restoreCache() {
    if (!this.engine) return;
    try {
      const pending = await this.engine.cache.load();
      if (pending) {
        this.pending = { release: pending.release, failed: pending.installing };
        this.publish({
          phase: pending.installing ? 'installError' : 'ready',
          release: pending.release,
        });
      }
    } catch {
      await this.engine.cache.clear();
      this.publish({ phase: 'downloadError', release: null });
    }
  }
  override check(): Promise<UpdateSnapshot> {
    if (this.operation) return this.operation;
    if (this.checking) return this.checking;
    const check = this.restoring ? this.restoring.then(() => super.check()) : super.check();
    this.checking = check
      .then((result) => {
        if (
          this.pending &&
          result.phase === 'available' &&
          result.release?.version === this.pending.release.version
        )
          this.publish({ ...result, phase: this.pending.failed ? 'installError' : 'ready' });
        return this.snapshot;
      })
      .finally(() => {
        this.checking = undefined;
      });
    return this.checking;
  }
  download(): Promise<UpdateSnapshot> {
    if (this.restoring) return this.restoring.then(() => this.download());
    if (this.operation) return this.operation;
    const release = this.snapshot.release;
    if (!this.engine || !release || !['available', 'downloadError'].includes(this.snapshot.phase))
      return Promise.resolve(this.snapshot);
    this.cancelled = false;
    this.publish({ ...this.snapshot, phase: 'downloading', progress: undefined });
    this.operation = (async () => {
      try {
        await this.engine!.download(release, (progress: UpdateProgress) => {
          if (!this.cancelled) this.publish({ phase: 'downloading', release, progress });
        });
        if (!this.cancelled) this.pending = { release, failed: false };
        this.publish({ phase: this.cancelled ? 'available' : 'ready', release });
      } catch (error) {
        this.publish({
          phase: this.cancelled ? 'available' : 'downloadError',
          release,
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
          this.publish({ phase: 'downloadError', release: this.snapshot.release });
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
