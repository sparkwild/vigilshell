import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { releaseContext } from '../../scripts/release-context.mjs';
import { renderChangelog } from '../../scripts/release-policy.mjs';

const recovery = {
  GITHUB_EVENT_NAME: 'workflow_dispatch',
  GITHUB_REF_TYPE: 'branch',
  GITHUB_REF: 'refs/heads/main',
  GITHUB_REPOSITORY: 'sparkwild/vigilshell',
  GITHUB_WORKFLOW_REF: 'sparkwild/vigilshell/.github/workflows/release-draft.yml@refs/heads/main',
  GITHUB_SHA: 'a'.repeat(40),
  RECOVERY_TAG: 'v0.1.2',
  RECOVERY_SHA: 'b'.repeat(40),
};

test('recovery accepts only an explicit main workflow and full immutable source identity', () => {
  assert.equal(releaseContext(recovery).recovery, true);
  for (const changes of [
    { GITHUB_EVENT_NAME: 'pull_request' },
    { GITHUB_REF: 'refs/heads/feature' },
    { GITHUB_REF_TYPE: 'tag' },
    { GITHUB_WORKFLOW_REF: 'attacker/repo/.github/workflows/release-draft.yml@refs/heads/main' },
    { RECOVERY_SHA: 'main' },
    { RECOVERY_TAG: 'v0.1.2-beta.1' },
  ])
    assert.throws(() => releaseContext({ ...recovery, ...changes }));
});

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'vigil-release-draft-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const source = join(root, 'source');
  const control = join(root, 'control');
  const bin = join(root, 'bin');
  for (const p of [source, control, bin]) mkdirSync(p);
  const git = (cwd, ...args) =>
    execFileSync('git', ['-C', cwd, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  for (const cwd of [source, control]) {
    git(cwd, 'init', '-b', 'main');
    git(cwd, 'config', 'user.name', 'Release test');
    git(cwd, 'config', 'user.email', 'test@example.invalid');
  }
  mkdirSync(join(source, 'docs/releases'), { recursive: true });
  writeFileSync(join(source, 'package.json'), JSON.stringify({ version: '0.1.2' }));
  writeFileSync(
    join(source, 'package-lock.json'),
    JSON.stringify({ version: '0.1.2', packages: { '': { version: '0.1.2' } } }),
  );
  writeFileSync(join(source, 'Cargo.toml'), 'version = "0.1.2"\n');
  writeFileSync(
    join(source, 'Cargo.lock'),
    '[[package]]\nname = "vigilshell-core"\nversion = "0.1.2"\n',
  );
  writeFileSync(
    join(source, 'docs/releases/v0.1.2.md'),
    '## Bug Fixes\n\n- Preserve cached updates.\n',
  );
  writeFileSync(join(source, 'CHANGELOG.md'), renderChangelog(source));
  git(source, 'add', '.');
  git(source, 'commit', '-m', 'Product');
  const sha = git(source, 'rev-parse', 'HEAD');
  git(source, 'tag', '-a', 'v0.1.2', '-m', '0.1.2');
  git(source, 'remote', 'add', 'origin', source);
  git(source, 'fetch', 'origin', 'main');
  for (const file of [
    'release-draft.mjs',
    'release-context.mjs',
    'release-policy.mjs',
    'release-notes.mjs',
    'preview-artifacts.mjs',
    'signing/config.mjs',
  ]) {
    const path = join(control, 'scripts', file);
    mkdirSync(join(path, '..'), { recursive: true });
    copyFileSync(join('scripts', file), path);
  }
  git(control, 'add', '.');
  git(control, 'commit', '-m', 'Control');
  const controlSha = git(control, 'rev-parse', 'HEAD');
  writeFileSync(join(bin, 'gh'), '#!/bin/sh\nprintf "%s" "${TEST_RELEASES:-[[]]}"\n', {
    mode: 0o755,
  });
  const output = join(root, 'output');
  const env = {
    ...process.env,
    ...recovery,
    RECOVERY_SHA: sha,
    GITHUB_SHA: controlSha,
    GITHUB_OUTPUT: output,
    PATH: `${bin}:${process.env.PATH}`,
  };
  const run = (changes = {}, command = 'qualify') =>
    spawnSync(process.execPath, [join(control, 'scripts/release-draft.mjs'), command], {
      cwd: source,
      env: { ...env, ...changes },
      encoding: 'utf8',
    });
  const readSource = () =>
    JSON.parse(readFileSync(output, 'utf8').trim().split('\n').at(-1).slice('source='.length));
  return { run, readSource, git, source, control, sha, controlSha };
}

test('normal tag qualification works with no node_modules or updater dependencies', (t) => {
  const f = fixture(t);
  const result = f.run({
    GITHUB_EVENT_NAME: 'push',
    GITHUB_REF_TYPE: 'tag',
    GITHUB_REF: 'refs/tags/v0.1.2',
    GITHUB_SHA: f.sha,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(f.readSource().sha, f.sha);
  assert.equal(f.readSource().recovery, undefined);
  const verified = f.run(
    {
      GITHUB_EVENT_NAME: 'push',
      GITHUB_REF_TYPE: 'tag',
      GITHUB_REF: 'refs/tags/v0.1.2',
      GITHUB_SHA: f.sha,
      RELEASE_SOURCE: JSON.stringify(f.readSource()),
    },
    'verify-source',
  );
  assert.equal(verified.status, 0, verified.stderr);
});

test('recovery qualifies and verifies clean product separately from control without node_modules', (t) => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stderr);
  const source = f.readSource();
  assert.equal(source.sha, f.sha);
  assert.equal(source.recovery.controlSha, f.controlSha);
  assert.notEqual(source.sha, f.controlSha);
  const verified = f.run({ RELEASE_SOURCE: JSON.stringify(source) }, 'verify-source');
  assert.equal(verified.status, 0, verified.stderr);
  assert.notEqual(
    f.run({ RELEASE_SOURCE: JSON.stringify({ ...source, tree: '0'.repeat(40) }) }, 'verify-source')
      .status,
    0,
  );
});

test('recovery fails closed on mismatched SHA, existing release, dirty product or dirty control', (t) => {
  const f = fixture(t);
  assert.notEqual(f.run({ RECOVERY_SHA: 'f'.repeat(40) }).status, 0);
  assert.notEqual(f.run({ TEST_RELEASES: '[[{"tag_name":"v0.1.2","draft":true}]]' }).status, 0);
  writeFileSync(join(f.source, 'unexpected'), 'dirty');
  assert.notEqual(f.run().status, 0);
  rmSync(join(f.source, 'unexpected'));
  writeFileSync(join(f.control, 'unexpected'), 'dirty');
  assert.notEqual(f.run().status, 0);
});

test('verification refuses a replaced tag object or product not on main', (t) => {
  const f = fixture(t);
  assert.equal(f.run().status, 0);
  const source = f.readSource();
  f.git(f.source, 'tag', '-f', '-a', 'v0.1.2', '-m', 'replacement');
  assert.notEqual(f.run({ RELEASE_SOURCE: JSON.stringify(source) }, 'verify-source').status, 0);
  f.git(f.source, 'checkout', '-b', 'other');
  writeFileSync(join(f.source, 'other'), 'different product');
  f.git(f.source, 'add', '.');
  f.git(f.source, 'commit', '-m', 'Not main');
  const sha = f.git(f.source, 'rev-parse', 'HEAD');
  f.git(f.source, 'tag', '-f', '-a', 'v0.1.2', '-m', 'other');
  assert.notEqual(f.run({ RECOVERY_SHA: sha }).status, 0);
});

test('workflow keeps automatic tags, immutable checkouts and locked draft dependencies', async () => {
  const { load } = await import('js-yaml');
  const workflow = load(readFileSync('.github/workflows/release-draft.yml', 'utf8'));
  assert.deepEqual(workflow.on.push.tags, ['v*']);
  assert.equal(workflow.on.workflow_dispatch.inputs.tag.required, true);
  assert.equal(workflow.on.workflow_dispatch.inputs['expected-sha'].required, true);
  assert.equal(workflow.defaults.run['working-directory'], 'source');
  for (const job of [workflow.jobs.qualify, workflow.jobs.draft]) {
    const checkouts = job.steps.filter((step) => step.uses?.startsWith('actions/checkout@'));
    assert.equal(checkouts.length, 2);
    assert.equal(checkouts[0].with.path, 'control');
    assert.equal(checkouts[0].with.ref, '${{ github.sha }}');
    assert.equal(checkouts[1].with.path, 'source');
    assert.equal(checkouts[0].with['persist-credentials'], false);
    assert.equal(checkouts[1].with['persist-credentials'], false);
  }
  assert.equal(workflow.jobs.qualify.steps[1].with['fetch-depth'], 0);
  assert.equal(
    workflow.jobs.draft.steps[1].with.ref,
    '${{ fromJSON(needs.qualify.outputs.source).sha }}',
  );
  const commands = workflow.jobs.draft.steps.map((step) => step.run ?? '').join('\n');
  assert.match(commands, /npm ci\n/);
  assert.match(commands, /npm --prefix "\$GITHUB_WORKSPACE\/control" ci --ignore-scripts/);
  assert.match(commands, /control\/scripts\/ci-macos-signing.sh" setup-release/);
  assert.match(commands, /npm run test:ci-macos/);
  assert.match(commands, /control\/scripts\/release-draft.mjs" draft/);
  assert.doesNotMatch(commands, /GITHUB_EVENT_NAME=|GITHUB_REF_TYPE=/);
});
