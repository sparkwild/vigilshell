import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  createReleaseNotesDocument,
  readReleaseNotes,
  renderNoteSections,
} from './release-notes.mjs';

const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function isStableReleaseTag(tag) {
  return tag.startsWith('v') && stableVersion.test(tag.slice(1));
}

export function checkVersions(root = '.') {
  const read = (name) => readFileSync(resolve(root, name), 'utf8');
  const pkg = JSON.parse(read('package.json'));
  assert(stableVersion.test(pkg.version), 'Expected a stable X.Y.Z version');
  const lock = JSON.parse(read('package-lock.json'));
  assert.equal(lock.version, pkg.version);
  assert.equal(lock.packages[''].version, pkg.version);
  assert.equal(read('Cargo.toml').match(/^version = "([^"]+)"/m)?.[1], pkg.version);
  for (const block of read('Cargo.lock').split('[[package]]')) {
    if (/\nname = "vigilshell-[^"]+"/.test(block))
      assert.equal(block.match(/\nversion = "([^"]+)"/)?.[1], pkg.version);
  }
  return pkg.version;
}

export function checkReleaseTag(tag, root = '.') {
  const version = checkVersions(root);
  assert.equal(tag, `v${version}`, 'Release tag must match the stable project version');
  return version;
}

export function renderChangelog(root = '.') {
  const tags = readdirSync(resolve(root, 'docs/releases'), { withFileTypes: true })
    .filter((file) => file.isDirectory())
    .map((file) => file.name)
    .filter(isStableReleaseTag)
    .sort((a, b) => b.localeCompare(a, 'en', { numeric: true }));
  const entries = tags.flatMap((tag) => [
    `## ${tag}`,
    ...renderNoteSections(readReleaseNotes(tag, root), '###'),
  ]);
  return (
    [
      '# Changelog',
      'See [GitHub Releases](https://github.com/sparkwild/vigilshell/releases) for published installers.',
      ...entries,
    ].join('\n\n') + '\n'
  );
}

export function checkReleaseNotes(tag, root = '.') {
  createReleaseNotesDocument(tag, root);
  const changelog = resolve(root, 'CHANGELOG.md');
  assert(
    existsSync(changelog) &&
      readFileSync(changelog, 'utf8').replaceAll('\r\n', '\n') === renderChangelog(root),
    'CHANGELOG.md is missing or out of date. Run npm run changelog:generate.',
  );
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const version = checkVersions();
  const tag = `v${version}`;
  if (process.argv.includes('--write-changelog')) {
    readReleaseNotes(tag);
    writeFileSync('CHANGELOG.md', renderChangelog());
  }
  if (process.argv.includes('--notes')) checkReleaseNotes(tag);
  console.log({ version });
}
