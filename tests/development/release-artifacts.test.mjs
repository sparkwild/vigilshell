import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, cpSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { aggregateReleaseBuilds } from '../../scripts/release-artifacts.mjs';
import { inspectUpdateArtifacts } from '../../scripts/verify-update-artifacts.mjs';

const source = {
  tag: 'v0.1.5',
  version: '0.1.5',
  sha: 'a'.repeat(40),
  tree: 'b'.repeat(40),
  tagObject: 'c'.repeat(40),
};
const identity = 'D'.repeat(40);
const hash = (value, algorithm = 'sha256', encoding = 'hex') =>
  createHash(algorithm).update(value).digest(encoding);
const json = (path, value) => writeFileSync(path, JSON.stringify(value));
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'vigilshell-dual-release-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const arch of ['arm64', 'x64']) {
    const directory = join(root, arch);
    mkdirSync(directory);
    const dmg = `VigilShell-${source.version}-macos-${arch}.dmg`;
    const zip = `VigilShell-${source.version}-macos-${arch}.zip`;
    const bytes = Buffer.from(`native ${arch} package`);
    const files = {
      [dmg]: bytes,
      [`${dmg}.sha256`]: `${hash(bytes)}  ${dmg}\n`,
      [zip]: bytes,
      [`${zip}.sha256`]: `${hash(bytes)}  ${zip}\n`,
      [`${zip}.blockmap`]: 'blockmap',
      'latest-mac.yml': JSON.stringify({
        version: source.version,
        files: [{ url: zip, size: bytes.length, sha512: hash(bytes, 'sha512', 'base64') }],
      }),
      'signing-report.json': JSON.stringify({
        version: source.version,
        sourceSha: source.sha,
        profile: 'release',
        arch,
        identity,
      }),
    };
    for (const [file, value] of Object.entries(files)) writeFileSync(join(directory, file), value);
    json(join(directory, 'build.json'), {
      ...source,
      arch,
      platform: 'darwin',
      identity,
      preview: false,
      signing: 'fixed-self-signed',
      nativeStartup: true,
      signingCleanup: true,
      runId: '10',
      runAttempt: '2',
      files: Object.fromEntries(Object.entries(files).map(([file, value]) => [file, hash(value)])),
    });
  }
  return {
    root,
    run: () => aggregateReleaseBuilds(root, join(root, 'assembled'), source, identity, '10', '2'),
    change: (arch, values) => {
      const path = join(root, arch, 'build.json');
      json(path, { ...JSON.parse(readFileSync(path)), ...values });
    },
  };
}

test('assembles both installers and one metadata document usable by each architecture', (t) => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.files.length, 12);
  assert.equal(new Set(result.files).size, result.files.length);
  for (const arch of ['arm64', 'x64']) {
    const update = inspectUpdateArtifacts(join(f.root, 'assembled'), source.version, arch, [
      'arm64',
      'x64',
    ]);
    assert.equal(update.metadata.files.length, 2);
    assert.equal(update.file.url, `VigilShell-${source.version}-macos-${arch}.zip`);
  }
  const reports = JSON.parse(readFileSync(result.reportPath));
  assert.deepEqual(Object.keys(reports.architectures), ['arm64', 'x64']);
});

for (const values of [
  { sha: 'e'.repeat(40) },
  { tree: 'e'.repeat(40) },
  { version: '0.1.6' },
  { arch: 'arm64' },
  { identity: 'E'.repeat(40) },
  { runId: '11' },
  { runAttempt: '1' },
  { nativeStartup: false },
  { signingCleanup: false },
  { preview: true },
])
  test(`rejects mixed or unverified builds: ${JSON.stringify(values)}`, (t) => {
    const f = fixture(t);
    f.change('x64', values);
    assert.throws(f.run);
  });

test('rejects a missing Intel build and mutated uploaded assets', (t) => {
  const missing = fixture(t);
  rmSync(join(missing.root, 'x64'), { recursive: true });
  assert.throws(missing.run);
  const corrupt = fixture(t);
  writeFileSync(join(corrupt.root, 'x64', 'VigilShell-0.1.5-macos-x64.zip.blockmap'), 'corrupt');
  assert.throws(corrupt.run, /Changed x64/);
});

test('preview aggregation requires native upgrade acceptance for both architectures', (t) => {
  const f = fixture(t);
  for (const arch of ['arm64', 'x64'])
    f.change(arch, { preview: true, signing: 'disposable-self-signed', nativeUpdate: true });
  const assemble = () =>
    aggregateReleaseBuilds(f.root, join(f.root, 'preview'), source, undefined, '10', '2', true);
  assert.equal(assemble().files.length, 12);
  f.change('x64', { nativeUpdate: false });
  assert.throws(assemble);
});

test('preview CLI completes aggregation with independently downloaded native builds', (t) => {
  const f = fixture(t);
  const input = join(f.root, '.cache/preview-builds');
  mkdirSync(input, { recursive: true });
  for (const arch of ['arm64', 'x64']) {
    f.change(arch, {
      tag: undefined,
      tagObject: undefined,
      preview: true,
      signing: 'disposable-self-signed',
      nativeUpdate: true,
    });
    cpSync(join(f.root, arch), join(input, arch), { recursive: true });
  }
  const bin = join(f.root, 'bin');
  mkdirSync(bin);
  writeFileSync(
    join(bin, 'git'),
    `#!/bin/sh\ncase "$2" in HEAD) echo ${source.sha};; 'HEAD^{tree}') echo ${source.tree};; *) exit 1;; esac\n`,
    { mode: 0o755 },
  );
  const result = spawnSync(process.execPath, [resolve('scripts/preview-artifacts.mjs'), 'merge'], {
    cwd: f.root,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      GITHUB_RUN_ID: '10',
      GITHUB_RUN_ATTEMPT: '2',
    },
    encoding: 'utf8',
    timeout: 10000,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(readFileSync(join(f.root, 'preview/latest-mac.yml'))).files.length, 2);
});
