import assert from 'node:assert/strict';
import {
  chmod,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { normalizeBundlePermissions } from '../../scripts/prepare-package.mjs';

test('macOS staging permits native installation without modifying linked sources or symlink targets', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-permissions-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bundle = join(root, 'App.app');
  await mkdir(bundle);
  const source = join(root, 'source');
  await writeFile(source, 'icon', { mode: 0o444 });
  await link(source, join(bundle, 'icon.icns'));
  await symlink(source, join(bundle, 'linked-icon'));
  await writeFile(join(bundle, 'binary'), 'executable', { mode: 0o555 });
  await chmod(bundle, 0o555);
  await normalizeBundlePermissions(bundle);
  assert.equal((await lstat(bundle)).mode & 0o777, 0o755);
  assert.equal((await lstat(join(bundle, 'icon.icns'))).mode & 0o777, 0o644);
  assert.equal((await lstat(join(bundle, 'binary'))).mode & 0o777, 0o755);
  assert.equal((await lstat(source)).mode & 0o777, 0o444);
  assert((await lstat(join(bundle, 'linked-icon'))).isSymbolicLink());
  assert.equal(await readFile(join(bundle, 'icon.icns'), 'utf8'), 'icon');
});
