export const updateDay = 24 * 60 * 60 * 1000;
export interface ScheduleRecord {
  lastSuccess: number;
  nextCheck: number;
  failures: number;
  notBefore: number;
}
export const initialSchedule = (): ScheduleRecord => ({
  lastSuccess: 0,
  nextCheck: 0,
  failures: 0,
  notBefore: 0,
});
/** Owns one background timer. Manual requests bypass the interval, but not server backoff. */
export class UpdateScheduler {
  record = initialSchedule();
  private timer?: ReturnType<typeof setTimeout>;
  private enabled = false;
  private paused = false;
  private stopped = false;
  private earliest = 0;
  constructor(private check: () => Promise<unknown>) {}
  configure(enabled: boolean) {
    if (this.enabled === enabled || this.stopped) return;
    this.enabled = enabled;
    if (enabled) this.earliest = Date.now() + 30000;
    this.schedule();
  }
  pause(value: boolean) {
    this.paused = value;
    this.schedule();
  }
  restore(record: ScheduleRecord) {
    this.record = record;
    this.schedule();
  }
  finish(success: boolean, notBefore = 0) {
    const now = Date.now();
    const failures = success ? 0 : Math.min(3, this.record.failures + 1);
    this.record = {
      lastSuccess: success ? now : this.record.lastSuccess,
      nextCheck: Math.max(
        now + (success ? updateDay : [0, 15 * 60000, 60 * 60000, 4 * 60 * 60000][failures]!),
        notBefore,
      ),
      failures,
      notBefore: success ? 0 : notBefore,
    };
    this.schedule();
  }
  resume() {
    this.schedule();
  }
  private schedule() {
    clearTimeout(this.timer);
    if (!this.enabled || this.paused || this.stopped) return;
    const due = Math.max(this.earliest, this.record.nextCheck, this.record.notBefore);
    this.timer = setTimeout(
      () => {
        void this.check();
      },
      Math.min(2147483647, Math.max(0, due - Date.now())),
    );
    this.timer.unref?.();
  }
  stop() {
    this.stopped = true;
    clearTimeout(this.timer);
  }
}
export function rateLimitUntil(response: Response, now = Date.now()): number {
  const retry = response.headers.get('retry-after');
  const reset = response.headers.get('x-ratelimit-reset');
  const candidates = [
    retry ? (/^\d+$/.test(retry) ? now + Number(retry) * 1000 : Date.parse(retry)) : 0,
    reset && /^\d+$/.test(reset) ? Number(reset) * 1000 : 0,
  ].filter((value) => Number.isSafeInteger(value) && value > now);
  return Math.max(now + 60000, ...candidates);
}
