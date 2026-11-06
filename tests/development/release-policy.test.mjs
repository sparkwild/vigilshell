import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRequire } from 'node:module';
import { releaseSource, checkVersions } from '../../scripts/release-policy.mjs';
const require = createRequire(import.meta.url);
const { load } = require('js-yaml');
const repo = 'sparkwild/vigilshell';
test('manual verification cannot become publication; PR and arbitrary branches cannot sign', () => {
  for (const branch of ['main', 'develop'])
    assert.equal(
      releaseSource({
        GITHUB_REPOSITORY: repo,
        GITHUB_EVENT_NAME: 'workflow_dispatch',
        GITHUB_REF: `refs/heads/${branch}`,
      }),
      'verify',
    );
  assert.equal(
    releaseSource({
      GITHUB_REPOSITORY: repo,
      GITHUB_EVENT_NAME: 'push',
      GITHUB_REF: 'refs/tags/v0.1.0-beta.1',
    }),
    'publish',
  );
  for (const [event, ref, owner] of [
    ['pull_request', 'refs/heads/main', repo],
    ['push', 'refs/heads/main', repo],
    ['workflow_dispatch', 'refs/heads/other', repo],
    ['push', 'refs/tags/v0.1.0', repo],
    ['push', 'refs/tags/v0.1.0-beta.0', repo],
    ['workflow_dispatch', 'refs/heads/main', 'other/repo'],
  ])
    assert.throws(() =>
      releaseSource({ GITHUB_REPOSITORY: owner, GITHUB_EVENT_NAME: event, GITHUB_REF: ref }),
    );
});
test('all version sources must match before any signing', async (t) => {
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
      '$10.1.0-beta.2',
    ),
  );
  assert.throws(() => checkVersions(root));
});
test('workflow isolates secret scopes, gates publishing on checks, and builds only ARM64', async () => {
  const w = load(await readFile('.github/workflows/macos-signing.yml', 'utf8'));
  assert.deepEqual(Object.keys(w.on).sort(), ['push', 'workflow_dispatch']);
  assert.equal(w.permissions.contents, 'read');
  for (const scope of ['environment', 'repository']) {
    const job = w.jobs[`package-${scope}`];
    assert(job.if.includes(`== '${scope}'`));
    assert.deepEqual(job.needs, ['policy', 'frontend', 'backend']);
    assert.equal(job.env.EXPECTED_ARCH, 'arm64');
    assert.equal(job.environment, scope === 'environment' ? 'macos-release' : undefined);
  }
  assert(w.jobs.publish.if.includes("github.event_name == 'push'"));
  assert(w.jobs.publish.if.includes("needs.frontend.result == 'success'"));
  assert(w.jobs.publish.if.includes("needs.backend.result == 'success'"));
  const action = load(await readFile('.github/actions/macos-package/action.yml', 'utf8'));
  const steps = action.runs.steps;
  const cleanup = steps.findIndex((s) => s.name === 'Remove disposable signing identity');
  const upload = steps.findIndex((s) => s.name === 'Upload verified release packages');
  assert(cleanup < upload);
  assert(steps[upload].if.includes('success()'));
  const pr = load(await readFile('.github/workflows/macos-ci.yml', 'utf8'));
  assert.deepEqual(
    pr.jobs.native.strategy.matrix.include.map((x) => x.arch),
    ['arm64'],
  );
  assert(!JSON.stringify(pr).includes('secrets.'));
});

test('release assets reject missing, corrupted or mismatched packages and published drafts', async (t) => {
  const { createHash } = await import('node:crypto');
  const { validateAssets, validateDraft } = await import('../../scripts/release-assets.mjs');
  const root = await mkdtemp(join(tmpdir(), 'vigil-assets-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const version = '0.1.0-beta.1',
    sha = 'a'.repeat(40),
    name = `VigilShell-${version}-macos-arm64.dmg`;
  const bytes = Buffer.from('fixture disk image');
  const report = {
    version,
    sourceSha: sha,
    arch: 'arm64',
    profile: 'release',
    identity: 'A'.repeat(40),
  };
  await writeFile(join(root, name), bytes);
  await writeFile(
    join(root, name + '.sha256'),
    `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`,
  );
  await writeFile(join(root, 'signing-report.json'), JSON.stringify(report));
  assert(validateAssets(root, version, sha));
  assert.throws(() => validateAssets(root, version, 'b'.repeat(40)));
  await writeFile(join(root, name), 'corrupted');
  assert.throws(() => validateAssets(root, version, sha));
  await writeFile(join(root, name), bytes);
  await writeFile(join(root, 'signing-report.json'), JSON.stringify({ ...report, arch: 'x64' }));
  assert.throws(() => validateAssets(root, version, sha));
  await rm(join(root, name + '.sha256'));
  assert.throws(() => validateAssets(root, version, sha));
  validateDraft({ draft: true, target_commitish: sha }, sha);
  assert.throws(() => validateDraft({ draft: false, target_commitish: sha }, sha));
  assert.throws(() => validateDraft({ draft: true, target_commitish: 'other' }, sha));
});
