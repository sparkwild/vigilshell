import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const execute = promisify(execFile);
/** Mount the finished image read-only and verify its sealed application before publishing.
 * @param {string} image @param {string} applicationName
 * @param {(path:string)=>unknown|Promise<unknown>} verify
 */
export async function verifyDmg(image, applicationName, verify) {
  const mount = await mkdtemp(join(tmpdir(), 'desktop-dmg-verify-'));
  let mounted = false;
  try {
    await execute('/usr/bin/hdiutil', [
      'attach',
      '-readonly',
      '-nobrowse',
      '-mountpoint',
      mount,
      image,
    ]);
    mounted = true;
    await verify(join(mount, `${applicationName}.app`));
  } finally {
    if (mounted) await execute('/usr/bin/hdiutil', ['detach', mount]);
    await rm(mount, { recursive: true, force: true });
  }
}
