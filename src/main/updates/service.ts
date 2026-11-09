import { EventEmitter } from 'node:events';
import type { AvailableRelease, UpdateSnapshot } from '@contracts';
import { automaticUpdateChecksEnabled } from '../../shared/application-features';
const day = 24 * 60 * 60 * 1000;
// Release versions follow the project's stable X.Y.Z grammar.
function version(value: string): number[] | null {
  const m = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value);
  if (!m) return null;
  const parts = [Number(m[1]), Number(m[2]), Number(m[3])];
  return parts.every(Number.isSafeInteger) ? parts : null;
}
function compare(a: number[], b: number[]) {
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i]! > b[i]! ? 1 : -1;
  }
  return 0;
}
export function selectRelease(
  data: unknown,
  current: string,
  platform: string,
  arch: string,
): UpdateSnapshot {
  if (!Array.isArray(data) || !version(current)) throw Error('invalidResponse');
  const installed = version(current)!;
  const releases: AvailableRelease[] = [];
  for (const item of data) {
    if (
      !item ||
      typeof item !== 'object' ||
      typeof item.tag_name !== 'string' ||
      typeof item.draft !== 'boolean' ||
      typeof item.prerelease !== 'boolean' ||
      !Array.isArray(item.assets)
    )
      throw Error('invalidResponse');
    const candidate = version(item.tag_name);
    if (item.draft || item.prerelease || !item.tag_name.startsWith('v') || !candidate) continue;
    const v = item.tag_name.replace(/^v/, '');
    if (
      platform !== 'darwin' ||
      !item.assets.some(
        (a: { name?: string; state?: string }) =>
          a.name === `VigilShell-${v}-macos-${arch}.dmg` && a.state === 'uploaded',
      )
    )
      continue;
    if (item.body != null && typeof item.body !== 'string') throw Error('invalidResponse');
    releases.push({ version: v, tag: item.tag_name, notes: (item.body ?? '').slice(0, 65536) });
  }
  releases.sort((a, b) => compare(version(b.version)!, version(a.version)!));
  const newest = releases[0];
  return {
    phase: !newest
      ? 'empty'
      : compare(version(newest.version)!, installed) > 0
        ? 'available'
        : 'current',
    release: newest && compare(version(newest.version)!, installed) > 0 ? newest : null,
  };
}
export class UpdateService extends EventEmitter<{ changed: [UpdateSnapshot] }> {
  snapshot: UpdateSnapshot = { phase: 'idle', release: null };
  private flight?: Promise<UpdateSnapshot>;
  private timer?: ReturnType<typeof setTimeout>;
  private enabled = false;
  private due = 0;
  private stopped = false;
  constructor(
    private current: string,
    private platform: string,
    private arch: string,
    private fetcher = fetch,
  ) {
    super();
  }
  configure(enabled: boolean) {
    enabled = automaticUpdateChecksEnabled && enabled;
    if (this.enabled === enabled || this.stopped) return;
    this.enabled = enabled;
    clearTimeout(this.timer);
    if (enabled) {
      this.due = Date.now() + 30000;
      this.schedule();
    }
  }
  private schedule() {
    clearTimeout(this.timer);
    if (this.enabled && !this.stopped)
      this.timer = setTimeout(
        () => {
          void this.check();
        },
        Math.max(0, this.due - Date.now()),
      );
  }
  resume() {
    if (this.enabled && Date.now() >= this.due) void this.check();
  }
  stop() {
    this.stopped = true;
    clearTimeout(this.timer);
  }
  private publish(snapshot: UpdateSnapshot) {
    if (this.stopped) return;
    this.snapshot = snapshot;
    this.emit('changed', snapshot);
  }
  check(): Promise<UpdateSnapshot> {
    if (this.flight) return this.flight;
    this.publish({ ...this.snapshot, phase: 'checking' });
    this.flight = this.run().finally(() => {
      this.flight = undefined;
      this.due = Date.now() + day;
      this.schedule();
    });
    return this.flight;
  }
  private async run() {
    try {
      const response = await this.fetcher(
        'https://api.github.com/repos/sparkwild/vigilshell/releases?per_page=100',
        {
          headers: { Accept: 'application/vnd.github+json' },
          signal: AbortSignal.timeout(15000),
          credentials: 'omit',
        },
      );
      if (response.status === 429 || response.status === 403) throw Error('rateLimited');
      if (!response.ok) throw Error('networkError');
      let data: unknown;
      try {
        data = await response.json();
      } catch {
        throw Error('invalidResponse');
      }
      this.publish(selectRelease(data, this.current, this.platform, this.arch));
    } catch (error) {
      const message = error instanceof Error ? error.message : '';
      this.publish({
        ...this.snapshot,
        phase:
          message === 'rateLimited'
            ? 'rateLimited'
            : message === 'invalidResponse'
              ? 'invalidResponse'
              : 'networkError',
      });
    }
    return this.snapshot;
  }
}
