import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { renderChangelog } from '../../scripts/release-policy.mjs';

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
  writeFileSync(join(bin, 'gh'), '#!/bin/sh\nprintf "%s" "${TEST_RELEASES:-[[]]}"\n', {
    mode: 0o755,
  });
  const output = join(root, 'output');
  const env = {
    ...process.env,
    GITHUB_EVENT_NAME: 'push',
    GITHUB_REF_TYPE: 'tag',
    GITHUB_REF: 'refs/tags/v0.1.2',
    GITHUB_REPOSITORY: 'sparkwild/vigilshell',
    GITHUB_SHA: sha,
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
  return { run, readSource, git, source, control, sha };
}

test('normal tag qualification works without node_modules or updater dependencies', (t) => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stderr);
  assert.equal(f.readSource().sha, f.sha);
});

test('qualification rejects dispatch, non-tag events, wrong SHA and existing releases', (t) => {
  const f = fixture(t);
  for (const changes of [
    { GITHUB_EVENT_NAME: 'workflow_dispatch' },
    { GITHUB_REF_TYPE: 'branch', GITHUB_REF: 'refs/heads/main' },
    { GITHUB_SHA: 'f'.repeat(40) },
    { TEST_RELEASES: '[[{"tag_name":"v0.1.2","draft":true}]]' },
  ])
    assert.notEqual(f.run(changes).status, 0);
});

test('qualification rejects a product outside main ancestry', (t) => {
  const f = fixture(t);
  f.git(f.source, 'checkout', '-b', 'other');
  writeFileSync(join(f.source, 'other'), 'different product');
  f.git(f.source, 'add', '.');
  f.git(f.source, 'commit', '-m', 'Not main');
  const sha = f.git(f.source, 'rev-parse', 'HEAD');
  f.git(f.source, 'tag', '-f', '-a', 'v0.1.2', '-m', 'other');
  assert.notEqual(f.run({ GITHUB_SHA: sha }).status, 0);
});

test('workflow retains tag-only automation, release environment and native verification', async () => {
  const { load } = await import('js-yaml');
  const workflow = load(readFileSync('.github/workflows/release-draft.yml', 'utf8'));
  assert.deepEqual(workflow.on, { push: { tags: ['v*'] } });
  assert.equal(workflow.jobs.draft.environment, 'macos-release');
  const commands = workflow.jobs.draft.steps.map((step) => step.run ?? '').join('\n');
  assert.match(commands, /npm ci/);
  assert.match(commands, /ci-macos-signing.sh setup-release/);
  assert.match(commands, /npm run test:ci-macos/);
  assert.match(commands, /release-draft.mjs draft/);
  assert.doesNotMatch(commands, /GITHUB_EVENT_NAME=|GITHUB_REF_TYPE=/);
});
