import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import type { AvailableRelease } from '@contracts';
import { newer } from './cache';
import { initialSchedule, type ScheduleRecord, updateDay } from './scheduler';
export interface DiscoveryRecord {
  schema: 1;
  source: string;
  current: string;
  schedule: ScheduleRecord;
  release: AvailableRelease | null;
  canDownload: boolean;
}
export class DiscoveryStore {
  constructor(
    private path: string,
    private source: string,
    private current: string,
  ) {}
  async load(): Promise<DiscoveryRecord | undefined> {
    try {
      const value = JSON.parse(await readFile(this.path, 'utf8')) as DiscoveryRecord;
      if (
        value.schema !== 1 ||
        value.source !== this.source ||
        typeof value.canDownload !== 'boolean'
      )
        throw Error('invalid_discovery');
      if (
        !value.schedule ||
        !['lastSuccess', 'nextCheck', 'failures', 'notBefore'].every(
          (key) =>
            Number.isSafeInteger(value.schedule[key as keyof ScheduleRecord]) &&
            value.schedule[key as keyof ScheduleRecord] >= 0,
        ) ||
        value.schedule.failures > 3
      )
        throw Error('invalid_schedule');
      const release = value.release;
      if (
        release &&
        (typeof release.version !== 'string' ||
          release.tag !== `v${release.version}` ||
          typeof release.notes !== 'string' ||
          release.notes.length > 65536)
      )
        throw Error('invalid_release');
      if (release && !newer(release.version, this.current)) value.release = null;
      if (value.current !== this.current || value.schedule.lastSuccess > Date.now())
        value.schedule = initialSchedule();
      // Clock changes must not suppress discovery indefinitely.
      value.schedule.nextCheck = Math.min(value.schedule.nextCheck, Date.now() + updateDay);
      return value;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT')
        console.error('Update discovery cache:', (error as Error).message);
      return;
    }
  }
  async save(value: Pick<DiscoveryRecord, 'release' | 'canDownload' | 'schedule'>) {
    try {
      await mkdir(dirname(this.path), { recursive: true, mode: 0o700 });
      const temporary = `${this.path}.tmp`;
      await writeFile(
        temporary,
        JSON.stringify({ ...value, schema: 1, source: this.source, current: this.current }),
        { mode: 0o600 },
      );
      await rename(temporary, this.path);
    } catch (error) {
      console.error('Update discovery cache:', (error as Error).message);
    }
  }
}
