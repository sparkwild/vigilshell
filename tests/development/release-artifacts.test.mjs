import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, cpSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import {
  selectPreviewArtifacts,
  assertSuccessfulPreviewJob,
} from '../../scripts/preview-reuse.mjs';
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
      runAttempt: arch === 'arm64' ? '1' : '2',
      preview: true,
      signing: 'disposable-self-signed',
      nativeUpdate: true,
    });
    cpSync(join(f.root, arch), join(input, arch), { recursive: true });
  }
  json(join(input, 'selection.json'), {
    sha: source.sha,
    runId: '10',
    aggregationAttempt: '2',
    artifacts: Object.fromEntries(
      ['arm64', 'x64'].map((arch, index) => [
        arch,
        {
          id: index + 1,
          name: `preview-macos-${arch}-${source.sha}-10-${arch === 'arm64' ? '1' : '2'}`,
          attempt: arch === 'arm64' ? '1' : '2',
        },
      ]),
    ),
  });
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
  const manifest = JSON.parse(readFileSync(join(f.root, 'preview/build.json')));
  assert.equal(manifest.architectures.arm64.runAttempt, '1');
  assert.equal(manifest.architectures.x64.runAttempt, '2');
  assert.equal(manifest.aggregation.artifacts.arm64.attempt, '1');
});

function previewFixture(t) {
  const f = fixture(t);
  for (const arch of ['arm64', 'x64'])
    f.change(arch, { preview: true, signing: 'disposable-self-signed', nativeUpdate: true });
  f.change('arm64', { runAttempt: '1' });
  return {
    ...f,
    merge: (attempts = { arm64: '1', x64: '2' }) =>
      aggregateReleaseBuilds(
        f.root,
        join(f.root, 'reused'),
        source,
        undefined,
        '10',
        '2',
        true,
        attempts,
      ),
  };
}
test('preview reuses exact earlier attempt and preserves each original build manifest', (t) => {
  const f = previewFixture(t);
  const original = readFileSync(join(f.root, 'arm64/build.json'), 'utf8');
  const result = f.merge();
  assert.equal(result.builds.arm64.runAttempt, '1');
  assert.equal(result.builds.x64.runAttempt, '2');
  assert.equal(readFileSync(join(f.root, 'arm64/build.json'), 'utf8'), original);
  assert.deepEqual(result.builds.arm64, JSON.parse(original));
});
for (const change of [
  { runId: '11' },
  { sha: 'e'.repeat(40) },
  { tree: 'e'.repeat(40) },
  { nativeStartup: false },
  { nativeUpdate: false },
  { signingCleanup: false },
  { runAttempt: '2' },
])
  test(`preview reuse rejects changed provenance ${JSON.stringify(change)}`, (t) => {
    const f = previewFixture(t);
    f.change('arm64', change);
    assert.throws(() => f.merge());
  });
test('cross-attempt preview still checks digests and cannot weaken release rules', (t) => {
  const f = previewFixture(t);
  for (const attempts of [{ arm64: '1' }, { arm64: '1', x64: '3' }, { arm64: '0', x64: '2' }])
    assert.throws(() => f.merge(attempts));
  writeFileSync(join(f.root, 'arm64/VigilShell-0.1.5-macos-arm64.zip.blockmap'), 'changed');
  assert.throws(() => f.merge(), /Changed arm64/);
  const release = fixture(t);
  assert.throws(
    () =>
      aggregateReleaseBuilds(
        release.root,
        join(release.root, 'bad'),
        source,
        identity,
        '10',
        '2',
        false,
        { arm64: '2', x64: '2' },
      ),
    /preview-only/,
  );
});
function artifact(arch, attempt, values = {}) {
  return {
    id: arch === 'arm64' ? 1 : 2,
    expired: false,
    name: `preview-macos-${arch}-${source.sha}-10-${attempt}`,
    workflow_run: { id: 10 },
    ...values,
  };
}
test('selection uses latest verified same-run same-SHA artifact per architecture', () => {
  const chosen = selectPreviewArtifacts(
    [
      artifact('arm64', '1'),
      artifact('x64', '1'),
      artifact('x64', '2'),
      artifact('arm64', '3'),
      artifact('arm64', '1', { name: 'diagnostics-arm64-10-2' }),
      artifact('arm64', '1', { name: `preview-macos-arm64-${'e'.repeat(40)}-10-2` }),
    ],
    source.sha,
    '10',
    '2',
  );
  assert.equal(chosen.arm64.attempt, '1');
  assert.equal(chosen.x64.attempt, '2');
});
test('selection refuses missing, expired, duplicate and foreign-run artifacts', () => {
  const arm = artifact('arm64', '1');
  const intel = artifact('x64', '2');
  for (const artifacts of [
    [arm],
    [arm, { ...intel, expired: true }],
    [arm, intel, intel],
    [arm, { ...intel, workflow_run: { id: 11 } }],
  ])
    assert.throws(() => selectPreviewArtifacts(artifacts, source.sha, '10', '2'));
});
test('reuse requires a successful architecture job in its exact run and attempt', () => {
  const job = {
    name: 'package-arm64',
    run_id: 10,
    run_attempt: 1,
    status: 'completed',
    conclusion: 'success',
  };
  assertSuccessfulPreviewJob([job], 'arm64', '1', '10');
  for (const change of [
    { conclusion: 'cancelled' },
    { conclusion: 'failure' },
    { status: 'in_progress' },
    { name: 'package-x64' },
    { run_id: 11 },
    { run_attempt: 2 },
  ])
    assert.throws(() => assertSuccessfulPreviewJob([{ ...job, ...change }], 'arm64', '1', '10'));
  assert.throws(() => assertSuccessfulPreviewJob([job, job], 'arm64', '1', '10'));
});

test('preview download paginates lists, accepts legacy job names, and fetches immutable IDs', (t) => {
  const f = fixture(t);
  const bin = join(f.root, 'bin');
  mkdirSync(bin);
  const zip = join(f.root, 'fixture.zip');
  const made = spawnSync('python3', [
    '-c',
    'import sys,zipfile; z=zipfile.ZipFile(sys.argv[1],"w"); z.writestr("build.json","{}"); z.close()',
    zip,
  ]);
  assert.equal(made.status, 0);
  const calls = join(f.root, 'calls.jsonl');
  writeFileSync(join(bin, 'git'), `#!/bin/sh\necho ${source.sha}\n`, { mode: 0o755 });
  writeFileSync(
    join(bin, 'gh'),
    `#!${process.execPath}
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + '\\n');
const endpoint = args.at(-1);
if (args[0] !== 'api') process.exit(9);
if (endpoint.endsWith('/artifacts?per_page=100')) {
  if (!args.includes('--paginate') || !args.includes('--slurp')) process.exit(8);
  process.stdout.write(JSON.stringify([{artifacts:[${JSON.stringify(artifact('arm64', '1'))}]},{artifacts:[${JSON.stringify(artifact('x64', '2'))}]}]));
} else if (endpoint.includes('/attempts/')) {
  if (!args.includes('--paginate') || !args.includes('--slurp')) process.exit(8);
  const arm = endpoint.includes('/attempts/1/');
  process.stdout.write(JSON.stringify([{jobs:[]},{jobs:[{name:arm?'package (arm64, macos-15, aarch64-apple-darwin)':'package-x64',run_id:10,run_attempt:arm?1:2,status:'completed',conclusion:'success'}]}]));
} else if (/^repos\\/owner\\/repo\\/actions\\/artifacts\\/[12]\\/zip$/.test(endpoint)) {
  if (process.env.FIXTURE_DELETED_ID === "1") process.exit(22);
  process.stdout.write(fs.readFileSync(${JSON.stringify(zip)}));
} else process.exit(7);
`,
    { mode: 0o755 },
  );
  const result = spawnSync(process.execPath, [resolve('scripts/preview-reuse.mjs')], {
    cwd: f.root,
    encoding: 'utf8',
    timeout: 10000,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      GITHUB_REPOSITORY: 'owner/repo',
      GITHUB_RUN_ID: '10',
      GITHUB_RUN_ATTEMPT: '2',
    },
  });
  assert.equal(result.status, 0, result.stderr);
  const selection = JSON.parse(readFileSync(join(f.root, '.cache/preview-builds/selection.json')));
  assert.equal(selection.artifacts.arm64.id, 1);
  assert.equal(selection.artifacts.x64.id, 2);
  const requests = readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(
    requests.filter((args) => args.at(-1).endsWith('/zip')),
    [
      ['api', 'repos/owner/repo/actions/artifacts/1/zip'],
      ['api', 'repos/owner/repo/actions/artifacts/2/zip'],
    ],
  );
  for (const arch of ['arm64', 'x64'])
    assert.equal(
      readFileSync(join(f.root, `.cache/preview-builds/${arch}/build.json`), 'utf8'),
      '{}',
    );
  const retry = join(f.root, 'deleted-id');
  mkdirSync(retry);
  const deleted = spawnSync(process.execPath, [resolve('scripts/preview-reuse.mjs')], {
    cwd: retry,
    encoding: 'utf8',
    timeout: 10000,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      GITHUB_REPOSITORY: 'owner/repo',
      GITHUB_RUN_ID: '10',
      GITHUB_RUN_ATTEMPT: '2',
      FIXTURE_DELETED_ID: '1',
    },
  });
  assert.notEqual(deleted.status, 0, 'Deleted artifact ID must fail without name fallback');
  assert.throws(() => readFileSync(join(retry, '.cache/preview-builds/selection.json')));
});
