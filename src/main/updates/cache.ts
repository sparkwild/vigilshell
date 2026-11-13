import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { join } from 'node:path';
import type { AvailableRelease } from '@contracts';
import type { UpdateInfo } from 'electron-updater';

export interface PendingUpdate {
  schema: 1;
  release: AvailableRelease;
  info: UpdateInfo;
  installing: boolean;
}
export function newer(candidate: string, current: string) {
  if (
    ![candidate, current].every((value) => /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value))
  )
    return false;
  const a = candidate.split('.').map(Number),
    b = current.split('.').map(Number);
  return (
    a.every(Number.isSafeInteger) &&
    b.every(Number.isSafeInteger) &&
    a.some(
      (part, index) => part > b[index]! && a.slice(0, index).every((before, i) => before === b[i]),
    )
  );
}
export function verifiedInfo(
  info: UpdateInfo,
  release: AvailableRelease,
  arch: string,
): UpdateInfo {
  const filename = `VigilShell-${release.version}-macos-${arch}.zip`;
  if (info.version !== release.version || release.tag !== `v${release.version}`)
    throw Error('invalid_update');
  const file = info.files?.find((entry) => entry.url === filename);
  if (
    !file ||
    !Number.isSafeInteger(file.size) ||
    file.size! <= 0 ||
    !/^[A-Za-z0-9+/]{86}==$/.test(file.sha512) ||
    Buffer.from(file.sha512, 'base64').length !== 64
  )
    throw Error('invalid_update');
  return {
    version: info.version,
    path: filename,
    sha512: file.sha512,
    files: [{ url: filename, sha512: file.sha512, size: file.size }],
    releaseDate: info.releaseDate,
    ...(info.minimumSystemVersion ? { minimumSystemVersion: info.minimumSystemVersion } : {}),
  };
}
async function sha512(path: string) {
  const hash = createHash('sha512');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('base64');
}
/** Owns one committed immutable ZIP and its restart receipt; Main serializes cache operations. */
export class UpdateCache {
  archive: string;
  private readonly record: string;
  constructor(
    private directory: string,
    private current: string,
    private arch: string,
  ) {
    this.archive = join(directory, 'archive.zip');
    this.record = join(directory, 'pending.json');
  }
  async write(value: PendingUpdate) {
    await mkdir(this.directory, { recursive: true, mode: 0o700 });
    const temporary = `${this.record}.tmp`;
    await writeFile(temporary, JSON.stringify(value), { mode: 0o600 });
    await rename(temporary, this.record);
  }
  async save(release: AvailableRelease, info: UpdateInfo, file: string) {
    const validated = verifiedInfo(info, release, this.arch);
    await mkdir(this.directory, { recursive: true, mode: 0o700 });
    const archive = this.archiveFor(validated);
    const temporary = `${archive}.tmp`;
    try {
      await copyFile(file, temporary);
      await chmod(temporary, 0o600);
      if (
        (await sha512(temporary)) !== validated.files[0]!.sha512 ||
        (await lstat(temporary)).size !== validated.files[0]!.size
      )
        throw Error('invalid_download');
      await rename(temporary, archive);
      // The old archive remains usable until the new receipt commits successfully.
      await this.write({ schema: 1, release, info: validated, installing: false });
      this.archive = archive;
      await this.removeArchivesExcept(archive).catch((error) =>
        console.error('Update archive cleanup:', error.message),
      );
    } finally {
      await rm(temporary, { force: true });
    }
  }
  async load(): Promise<PendingUpdate | undefined> {
    let value: PendingUpdate;
    try {
      value = JSON.parse(await readFile(this.record, 'utf8'));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        await this.clear();
        return;
      }
      throw error;
    }
    if (
      value.schema !== 1 ||
      typeof value.installing !== 'boolean' ||
      !value.release ||
      typeof value.release.notes !== 'string' ||
      value.release.notes.length > 65536
    )
      throw Error('invalid_cache');
    if (!newer(value.release.version, this.current)) {
      await this.clear();
      return;
    }
    value.info = verifiedInfo(value.info, value.release, this.arch);
    this.archive = this.archiveFor(value.info);
    const file = value.info.files[0]!;
    const stat = await lstat(this.archive);
    if (
      !stat.isFile() ||
      stat.isSymbolicLink() ||
      stat.size !== file.size ||
      (await sha512(this.archive)) !== file.sha512
    )
      throw Error('invalid_cache');
    await this.removeArchivesExcept(this.archive).catch((error) =>
      console.error('Update archive cleanup:', error.message),
    );
    return value;
  }
  async clear() {
    await rm(this.record, { force: true });
    await this.removeArchivesExcept();
    await rm(`${this.record}.tmp`, { force: true });
  }
  private archiveFor(info: UpdateInfo) {
    return join(
      this.directory,
      `archive-${Buffer.from(info.files[0]!.sha512, 'base64').toString('hex')}.zip`,
    );
  }
  private async removeArchivesExcept(keep?: string) {
    const entries = await readdir(this.directory, { withFileTypes: true }).catch((error) => {
      if (error.code === 'ENOENT') return [];
      throw error;
    });
    for (const entry of entries) {
      const path = join(this.directory, entry.name);
      if (
        path !== keep &&
        !entry.isDirectory() &&
        /^archive(?:-[a-f0-9]{128})?\.zip(?:\.tmp)?$/.test(entry.name)
      )
        await rm(path, { force: true });
    }
  }
}
