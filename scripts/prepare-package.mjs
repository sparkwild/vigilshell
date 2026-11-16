import { constants } from 'node:fs';
import { chmod, copyFile, lstat, mkdir, readdir, rename, rm } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { packageBuildContext } from './build-context.mjs';

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
  const build = packageBuildContext(
    process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release',
    process.arch,
    context.packager.projectDir,
  );
  // Keep DMG-only resources outside appOutDir so the ZIP and App layouts stay unchanged.
  const notices = join(build.directory, 'dmg-licenses');
  await mkdir(notices, { recursive: true });
  await copyFile(
    join(context.packager.projectDir, 'node_modules/electron/dist/LICENSES.chromium.html'),
    join(notices, 'LICENSES.chromium.html'),
  );
  // DMG content paths do not expand the build-root macros used by extraResources.
  context.packager.config.dmg.contents.find((entry) => entry.name === '.licenses').path = notices;
}
