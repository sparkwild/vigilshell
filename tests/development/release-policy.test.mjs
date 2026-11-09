import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkReleaseTag, checkVersions } from '../../scripts/release-policy.mjs';

test('release tag must match the stable project version exactly', () => {
  const version = checkVersions();
  assert.equal(checkReleaseTag(`v${version}`), version);
  for (const tag of [version, `v${version}-beta.1`, `v${version}\n`, `v${version}.1`])
    assert.throws(() => checkReleaseTag(tag), /Release tag must match/);
});

test('all project version sources must match', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigil-version-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const file of ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock'])
    await writeFile(join(root, file), await readFile(file));
  assert.equal(checkVersions(root), JSON.parse(await readFile('package.json', 'utf8')).version);
  const path = join(root, 'Cargo.lock');
  await writeFile(
    path,
    (await readFile(path, 'utf8')).replace(
      /(name = "vigilshell-[^"]+"\nversion = ")[^"]+/,
      '$10.1.1',
    ),
  );
  assert.throws(() => checkVersions(root));
});

test('prepare a stable version without touching dependency versions; reject invalid input', async (t) => {
  const { prepareVersion } = await import('../../scripts/prepare-version.mjs');
  const root = await mkdtemp(join(tmpdir(), 'vigil-prepare-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const file of ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock'])
    await writeFile(join(root, file), await readFile(file));
  const before = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
  prepareVersion('0.1.0', root);
  assert.equal(checkVersions(root), '0.1.0');
  const after = JSON.parse(await readFile(join(root, 'package-lock.json'), 'utf8'));
  for (const [name, pkg] of Object.entries(before.packages))
    if (name) assert.deepEqual(after.packages[name], pkg);
  for (const invalid of ['v0.1.0', '01.2.3', '1.0', '1.0.0-beta.1', '1.0.0\n'])
    assert.throws(() => prepareVersion(invalid, root));
  assert.equal(checkVersions(root), '0.1.0');
});

test('version checks reject prerelease source versions even when every file agrees', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigil-version-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const current = checkVersions();
  for (const file of ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock'])
    await writeFile(
      join(root, file),
      (await readFile(file, 'utf8')).replaceAll(current, '0.1.0-beta.1'),
    );
  assert.throws(() => checkVersions(root), /Expected a stable X.Y.Z version/);
});
