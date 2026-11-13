import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdir, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  checkReleaseNotes,
  checkReleaseTag,
  checkVersions,
  renderChangelog,
} from '../../scripts/release-policy.mjs';
import {
  createReleaseNotesDocument,
  readReleaseNotes,
  renderReleaseNotes,
} from '../../scripts/release-notes.mjs';

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
  const version = JSON.parse(await readFile('package.json', 'utf8')).version;
  assert.equal(checkVersions(root), version);
  const mismatchedVersion = version === '0.1.1' ? '0.1.0' : '0.1.1';
  const path = join(root, 'Cargo.lock');
  await writeFile(
    path,
    (await readFile(path, 'utf8')).replace(
      /(name = "vigilshell-[^"]+"\nversion = ")[^"]+/,
      (_, prefix) => prefix + mismatchedVersion,
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

test('release notes require content and reserve generated sections for the workflow', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigil-release-notes-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  assert.throws(() => readReleaseNotes('v0.2.0', root), /Missing release notes/);

  const dir = join(root, 'docs', 'releases', 'v0.2.0');
  await mkdir(dir, { recursive: true });
  const path = join(dir, 'en.md');
  for (const [markdown, error] of [
    ['## Features\n', /empty Features/],
    ['', /requires at least one release notes section/],
    [
      '## Features\n\n- A feature\n\n## Downloads\n\n- An asset\n',
      /unsupported section: Downloads/,
    ],
  ]) {
    await writeFile(path, markdown);
    assert.throws(() => readReleaseNotes('v0.2.0', root), error);
  }
  await writeFile(path, '## Features\r\n\r\n- A feature\r\n');
  assert.deepEqual(readReleaseNotes('v0.2.0', root), { Features: '- A feature' });
  await writeFile(path, '## Bug Fixes\n\n- A fix\n');
  assert.deepEqual(readReleaseNotes('v0.2.0', root), { 'Bug Fixes': '- A fix' });
});

test('rendered notes put upgrade actions before versioned downloads', () => {
  const repository = 'sparkwild/vigilshell';
  const repoUrl = `https://github.com/${repository}`;
  const source = { tag: 'v0.2.0', sha: 'a'.repeat(40) };
  const installer = {
    os: 'macOS',
    format: 'DMG',
    architecture: 'Apple Silicon',
    color: '0969DA',
    logo: 'apple',
    name: 'VigilShell-0.2.0-macos-arm64.dmg',
  };
  const assets = {
    installers: [installer],
    manifest: 'build.json',
    signingReport: 'signing-report.json',
  };
  const options = { repository, source, assets, runUrl: `${repoUrl}/actions/runs/1` };
  const notes = renderReleaseNotes(
    {
      'Upgrade Notes': '- Back up settings before upgrading.',
      Features: '- A feature',
      'Bug Fixes': '- A fix',
    },
    { ...options, previousTag: 'v0.1.0' },
  );
  assert.deepEqual(notes.match(/^## .+$/gm), [
    '## Features',
    '## Bug Fixes',
    '## Upgrade Notes',
    '## Downloads',
    '## Installation & Upgrade',
    '## Build & Verification',
  ]);
  for (const name of [installer.name, assets.manifest, assets.signingReport])
    assert(notes.includes(`${repoUrl}/releases/download/${source.tag}/${name}`));
  assert.match(notes, /\| OS \| Download \|\n\| :--- \| :--- \|\n\| macOS \| <a href="/);
  assert.match(notes, /color=0969DA&amp;logo=apple/);
  assert.match(notes, /alt="DMG for Apple Silicon" align="absmiddle"><\/a> \|$/m);
  assert.doesNotMatch(notes, /SHA-256|\.dmg\.sha256/);
  assert(notes.includes(`[Changelog](${repoUrl}/blob/main/CHANGELOG.md)`));
  assert(notes.includes(`[Compare source changes](${repoUrl}/compare/v0.1.0...v0.2.0)`));

  const first = renderReleaseNotes(
    { Features: '- A feature' },
    { ...options, showChangelog: false },
  );
  assert.doesNotMatch(
    first,
    /## Bug Fixes|## Upgrade Notes|Changelog|Compare source changes|\| (Windows|Linux) \|/,
  );

  const multi = renderReleaseNotes(
    { Features: '- A feature' },
    {
      ...options,
      assets: {
        ...assets,
        installers: [
          installer,
          {
            ...installer,
            architecture: 'Intel x64',
            color: '00A9E0',
            name: 'VigilShell-0.2.0-macos-x64.dmg',
          },
          {
            os: 'Linux',
            format: 'AppImage',
            architecture: 'x64',
            color: 'F84E29',
            logo: 'linux',
            name: 'VigilShell-0.2.0-linux-x64.AppImage',
          },
        ],
      },
    },
  );
  assert.equal(multi.match(/^\| macOS \|/gm)?.length, 1);
  assert.match(multi, /^\| macOS \| .+<br><a .+alt="DMG for Intel x64"/m);
  assert.match(multi, /^\| Linux \| <a .+alt="AppImage for x64"/m);
  assert.doesNotMatch(multi, /^\| Windows \|/m);
  assert.match(
    renderReleaseNotes({ Features: '- Feature' }, { ...options, hasChinese: true }),
    /blob\/v0.2.0\/docs\/releases\/v0.2.0\/zh-CN.md/,
  );
});

test('changelog preflight rejects history out of sync with the version notes', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigil-changelog-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const dir = join(root, 'docs', 'releases');
  await mkdir(dir, { recursive: true });
  await mkdir(join(dir, 'v0.2.2'), { recursive: true });
  await mkdir(join(dir, 'v0.2.10'), { recursive: true });
  await writeFile(join(dir, 'v0.2.2', 'en.md'), '## Features\n\n- Earlier change\n');
  const latest = join(dir, 'v0.2.10', 'en.md');
  await writeFile(latest, '## Features\n\n- Recent change\n\n## Bug Fixes\n\n- Fixed behavior\n');
  await writeFile(
    join(dir, 'v0.2.10', 'zh-CN.md'),
    '## 新增功能\n\n- 中文功能\n\n## 问题修复\n\n- 中文修复\n',
  );

  assert.throws(() => checkReleaseNotes('v0.2.10', root), /CHANGELOG.md is missing or out of date/);
  const log = renderChangelog(root);
  assert.deepEqual(log.match(/^## v.+$/gm), ['## v0.2.10', '## v0.2.2']);
  assert.match(log, /### Bug Fixes\n\n- Fixed behavior/);
  assert.doesNotMatch(log, /Translated notes/);
  await writeFile(join(root, 'CHANGELOG.md'), log);
  assert.doesNotThrow(() => checkReleaseNotes('v0.2.10', root));
  await writeFile(latest, '## Features\n\n- Updated change\n\n## Bug Fixes\n\n- Fixed behavior\n');
  assert.throws(() => checkReleaseNotes('v0.2.10', root), /CHANGELOG.md is missing or out of date/);
});

test('generates version-bound bilingual notes and requires complete translations only when provided', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'vigil-bilingual-notes-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const directory = join(root, 'docs/releases/v0.2.0');
  await mkdir(directory, { recursive: true });
  await writeFile(
    join(directory, 'en.md'),
    '## Features\n\n- **New** feature\n\n## Upgrade Notes\n\n- Back up settings\n',
  );
  const english = createReleaseNotesDocument('v0.2.0', root);
  assert.deepEqual(english, {
    schema: 1,
    tag: 'v0.2.0',
    version: '0.2.0',
    languages: { en: { features: ['New feature'], upgradeNotes: ['Back up settings'] } },
  });
  await writeFile(join(directory, 'zh-CN.md'), '## 新增功能\n\n- 新功能\n');
  assert.throws(() => createReleaseNotesDocument('v0.2.0', root), /cover all English sections/);
  await writeFile(
    join(directory, 'zh-CN.md'),
    '## 新增功能\n\n- 新功能\n\n## 升级注意事项\n\n- 请备份设置\n',
  );
  const document = createReleaseNotesDocument('v0.2.0', root);
  assert.deepEqual(document.languages['zh-CN'], {
    features: ['新功能'],
    upgradeNotes: ['请备份设置'],
  });
  const { default: Ajv } = await import('ajv');
  const ajv = new Ajv({ strict: false, validateFormats: false });
  ajv.addSchema(
    JSON.parse(await readFile('src/contracts/generated/runtime.schema.json', 'utf8')),
    'contract',
  );
  assert(ajv.getSchema('contract#/definitions/ReleaseNotesDocument')(document));
  assert.throws(() => readReleaseNotes('../v0.2.0', root), /Invalid release tag/);
  await writeFile(join(directory, 'zh-CN.md'), '## 下载\n\n- 不应包含下载信息\n');
  assert.throws(() => createReleaseNotesDocument('v0.2.0', root), /unsupported section/);
  await rm(join(directory, 'zh-CN.md'));
  await writeFile(
    join(directory, 'en.md'),
    '## Features\n\n' + ('- ' + 'x'.repeat(645) + '\n').repeat(100),
  );
  assert.throws(() => createReleaseNotesDocument('v0.2.0', root), /exceed 64 KiB/);
});
