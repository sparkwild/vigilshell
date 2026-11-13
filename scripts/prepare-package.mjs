import { constants } from 'node:fs';
import { chmod, copyFile, lstat, readdir, rename, rm } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';

/** Squirrel removes quarantine attributes after extraction, which requires owner-write permission.
 * Normalize only the unsigned staging bundle, retaining executable bits and symlink targets.
 */
export async function normalizeBundlePermissions(path) {
  const info = await lstat(path);
  if (info.isSymbolicLink()) return;
  const mode = (info.mode & 0o777) | 0o200;
  if (!(info.mode & 0o200)) {
    if (info.isFile() && info.nlink > 1) {
      const copy = `${path}.${randomUUID()}.copy`;
      try {
        await copyFile(path, copy, constants.COPYFILE_EXCL);
        await chmod(copy, mode);
        await rename(copy, path);
      } finally {
        await rm(copy, { force: true });
      }
    } else await chmod(path, mode);
  }
  if (info.isDirectory())
    for (const name of await readdir(path)) await normalizeBundlePermissions(join(path, name));
}

export default async function preparePackage(context) {
  if (context.electronPlatformName !== 'darwin') return;
  await normalizeBundlePermissions(
    join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`),
  );
}
