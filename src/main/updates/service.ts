import { EventEmitter } from 'node:events';
import type { AvailableRelease, UpdateCheckPhase, UpdateSnapshot } from '@contracts';
import { automaticUpdateChecksEnabled } from '../../shared/application-features';
import type { DiscoveryStore } from './discovery-store';
import { UpdateScheduler, rateLimitUntil } from './scheduler';
import { selectRelease } from './releases';
import { loadReleaseNotes } from './release-notes';
export { selectRelease } from './releases';
export interface DiscoveryOptions {
  store?: DiscoveryStore;
  releases?: string;
  notes?(tag: string): string;
  inspect?(release: AvailableRelease): Promise<boolean>;
}
export class UpdateService extends EventEmitter<{ changed: [UpdateSnapshot] }> {
  snapshot: UpdateSnapshot = { phase: 'idle', release: null };
  private flight?: Promise<UpdateSnapshot>;
  private stopped = false;
  protected readonly scheduler: UpdateScheduler;
  protected readonly initialized: Promise<void>;
  constructor(
    private current: string,
    private platform: string,
    private arch: string,
    private fetcher = fetch,
    private discovery: DiscoveryOptions = {},
  ) {
    super();
    this.scheduler = new UpdateScheduler(() => this.check('background'));
    this.initialized = this.restoreDiscovery();
  }
  private async restoreDiscovery() {
    const record = await this.discovery.store?.load();
    if (record) {
      this.scheduler.restore(record.schedule);
      this.publish({
        phase: record.release ? 'available' : 'idle',
        release: record.release,
        canDownload: record.canDownload,
      });
    }
  }
  configure(enabled: boolean) {
    void this.initialized.then(() =>
      this.scheduler.configure(automaticUpdateChecksEnabled && enabled),
    );
  }
  resume() {
    this.scheduler.resume();
  }
  stop() {
    this.stopped = true;
    this.scheduler.stop();
  }
  protected publish(snapshot: UpdateSnapshot) {
    if (this.stopped) return;
    this.snapshot = snapshot;
    this.scheduler.pause(
      ['downloading', 'ready', 'preparing', 'installing', 'installError'].includes(snapshot.phase),
    );
    this.emit('changed', snapshot);
  }
  check(origin: 'manual' | 'background' = 'manual'): Promise<UpdateSnapshot> {
    if (
      origin === 'background' &&
      ['downloading', 'ready', 'preparing', 'installing', 'installError'].includes(
        this.snapshot.phase,
      )
    )
      return Promise.resolve(this.snapshot);
    if (this.flight) return this.flight;
    this.flight = this.initialized
      .then(async () => {
        if (this.stopped) return this.snapshot;
        if (Date.now() < this.scheduler.record.notBefore) {
          this.publish({ ...this.snapshot, checkPhase: 'rateLimited' });
          return this.snapshot;
        }
        this.scheduler.pause(true);
        this.publish({ ...this.snapshot, checkPhase: 'checking' });
        this.scheduler.pause(true);
        await this.run();
        return this.snapshot;
      })
      .finally(() => {
        this.flight = undefined;
      });
    return this.flight;
  }
  private async run() {
    let success = false,
      notBefore = 0;
    try {
      const response = await this.fetcher(
        this.discovery.releases ??
          'https://api.github.com/repos/sparkwild/vigilshell/releases?per_page=100',
        {
          headers: { Accept: 'application/vnd.github+json' },
          signal: AbortSignal.timeout(15000),
          credentials: 'omit',
        },
      );
      if (
        response.status === 429 ||
        (response.status === 403 &&
          (response.headers.get('x-ratelimit-remaining') === '0' ||
            response.headers.has('retry-after')))
      ) {
        notBefore = rateLimitUntil(response);
        throw Error('rateLimited');
      }
      if (!response.ok) throw Error('networkError');
      let data: unknown;
      try {
        data = await response.json();
      } catch {
        throw Error('invalidResponse');
      }
      const selected = selectRelease(data, this.current, this.platform, this.arch);
      const release = selected.release;
      const hasNotes =
        release &&
        (data as { tag_name: string; assets: { name: string; state: string }[] }[])
          .find((item) => item.tag_name === release.tag)
          ?.assets.some(
            (asset) => asset.name === 'release-notes.json' && asset.state === 'uploaded',
          );
      const [canDownload, withNotes] = await Promise.all([
        selected.canDownload && release && this.discovery.inspect
          ? this.discovery.inspect(release).catch(() => false)
          : false,
        hasNotes && release
          ? loadReleaseNotes(
              release,
              this.discovery.notes?.(release.tag) ??
                `https://github.com/sparkwild/vigilshell/releases/download/${release.tag}/release-notes.json`,
              this.fetcher,
            )
          : release,
      ]);
      success = true;
      this.publish({
        ...selected,
        release: withNotes,
        canDownload,
        checkPhase: selected.phase as UpdateCheckPhase,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : '';
      const checkPhase =
        message === 'rateLimited'
          ? 'rateLimited'
          : message === 'invalidResponse'
            ? 'invalidResponse'
            : 'networkError';
      this.publish({
        ...this.snapshot,
        phase: this.snapshot.release ? this.snapshot.phase : checkPhase,
        checkPhase,
      });
    } finally {
      this.scheduler.pause(true);
      this.scheduler.finish(success, notBefore);
      await this.discovery.store?.save({
        release: this.snapshot.release ?? null,
        canDownload: this.snapshot.canDownload === true,
        schedule: this.scheduler.record,
      });
      if (!this.stopped)
        this.scheduler.pause(
          ['downloading', 'ready', 'preparing', 'installing', 'installError'].includes(
            this.snapshot.phase,
          ),
        );
    }
  }
}
