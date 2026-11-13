import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { load } from 'js-yaml';
import preparePackage from '../../scripts/prepare-package.mjs';
import checksumArtifact from '../../scripts/checksum-artifact.mjs';
import { verifyDmg } from '../../scripts/verify-dmg.mjs';

test('DMG preparation preserves notices outside the app and resolves the hidden directory source', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-dmg-notices-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const appOutDir = join(root, 'output');
  const app = join(appOutDir, 'VigilShell.app');
  const electron = join(root, 'node_modules/electron/dist');
  const notice = 'LICENSES.chromium.html';
  const bytes = Buffer.from('<html>Complete Chromium notices ©</html>');
  await mkdir(app, { recursive: true });
  await mkdir(electron, { recursive: true });
  await writeFile(join(electron, notice), bytes);
  await writeFile(join(appOutDir, notice), bytes);
  const config = load(await readFile('electron-builder.yml', 'utf8'));
  await preparePackage({
    electronPlatformName: 'darwin',
    appOutDir,
    packager: { projectDir: root, appInfo: { productFilename: 'VigilShell' }, config },
  });
  const hidden = config.dmg.contents.find((entry) => entry.name === '.licenses');
  assert.equal(hidden.type, 'dir');
  assert.deepEqual(await readFile(join(hidden.path, notice)), bytes);
  assert.deepEqual(await readdir(app), []);
  assert.deepEqual(await readFile(join(appOutDir, notice)), bytes);
  assert.equal(config.dmg.sign, false);
});

test('DMG checksum hook leaves artifact bytes unchanged and verification rejects a changed image', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-dmg-checksum-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const file = join(root, 'VigilShell.dmg');
  const bytes = Buffer.from('finished disk image');
  await writeFile(file, bytes);
  await checksumArtifact({ file });
  assert.deepEqual(await readFile(file), bytes);
  assert.equal(
    await readFile(`${file}.sha256`, 'utf8'),
    `${createHash('sha256').update(bytes).digest('hex')}  VigilShell.dmg\n`,
  );
  await writeFile(file, 'changed disk image');
  await assert.rejects(
    verifyDmg(file, 'VigilShell', () => assert.fail('must reject before mounting')),
    /DMG checksum differs/,
  );
  await checksumArtifact({ file: join(root, 'update.zip') });
  await checksumArtifact({});
  assert.deepEqual((await readdir(root)).sort(), ['VigilShell.dmg', 'VigilShell.dmg.sha256']);
});
