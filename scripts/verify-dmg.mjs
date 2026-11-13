import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
const execute = promisify(execFile);
/** Mount the finished image read-only and verify its sealed application before publishing.
 * @param {string} image @param {string} applicationName
 * @param {(path:string)=>unknown|Promise<unknown>} verify
 */
export async function verifyDmg(image, applicationName, verify) {
  const digest = createHash('sha256')
    .update(await readFile(image))
    .digest('hex');
  if ((await readFile(`${image}.sha256`, 'utf8')).trim() !== `${digest}  ${basename(image)}`)
    throw Error('DMG checksum differs from the final image');
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
    const visible = (await readdir(mount)).filter((name) => !name.startsWith('.')).sort();
    if (
      JSON.stringify(visible) !== JSON.stringify(['Applications', `${applicationName}.app`].sort())
    )
      throw Error('DMG must show only the application and Applications');
    const notice = 'LICENSES.chromium.html';
    if (JSON.stringify(await readdir(join(mount, '.licenses'))) !== JSON.stringify([notice]))
      throw Error('DMG hidden license directory must contain only Chromium notices');
    const [packaged, original] = await Promise.all([
      readFile(join(mount, '.licenses', notice)),
      readFile(new URL(`../node_modules/electron/dist/${notice}`, import.meta.url)),
    ]);
    if (!original.length || !packaged.equals(original))
      throw Error('DMG Chromium notices differ from the Electron distribution');
    await verify(join(mount, `${applicationName}.app`));
  } finally {
    if (mounted) await execute('/usr/bin/hdiutil', ['detach', mount]);
    await rm(mount, { recursive: true, force: true });
  }
}
