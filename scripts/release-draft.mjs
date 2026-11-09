import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  createReadStream,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, join } from 'node:path';
import { checkReleaseTag } from './release-policy.mjs';
import { validatePreview } from './preview-artifacts.mjs';
import { normalizeFingerprint } from './signing/config.mjs';

const env = process.env;
const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));
const gh = (args, input) => execFileSync('gh', args, { encoding: 'utf8', input }).trim();
const api = (args, body) => JSON.parse(gh(['api', ...args], body && JSON.stringify(body)));
const releases = `repos/${env.GITHUB_REPOSITORY}/releases`;

function verifyTag(source) {
  assert.equal(env.GITHUB_REF, `refs/tags/${source.tag}`);
  assert.equal(
    git('ls-remote', '--refs', 'origin', env.GITHUB_REF),
    `${source.tagObject}\t${env.GITHUB_REF}`,
    'Release tag is missing or has moved',
  );
}

function requireUnusedRelease(tag) {
  const existing = api(['--paginate', '--slurp', `${releases}?per_page=100`]).flat();
  assert(!existing.some((release) => release.tag_name === tag), 'A release already uses this tag');
}

function qualify() {
  assert(env.GITHUB_REF.startsWith('refs/tags/'));
  const tag = env.GITHUB_REF.slice('refs/tags/'.length);
  const version = checkReleaseTag(tag);
  const sha = git('rev-parse', 'HEAD');
  assert.equal(git('rev-parse', `${env.GITHUB_REF}^{commit}`), sha);
  assert.equal(git('rev-parse', `${env.GITHUB_SHA}^{commit}`), sha);
  const source = {
    tag,
    version,
    sha,
    tagObject: git('rev-parse', env.GITHUB_REF),
    tree: git('rev-parse', 'HEAD^{tree}'),
  };
  verifyTag(source);
  git('merge-base', '--is-ancestor', sha, 'refs/remotes/origin/main');
  const runs = api([
    `repos/${env.GITHUB_REPOSITORY}/actions/workflows/ci.yml/runs?branch=main&event=push&head_sha=${sha}&status=success&per_page=1`,
  ]).workflow_runs;
  const checked = runs.find((run) => run.head_sha === sha && run.conclusion === 'success');
  assert(checked, 'The tagged commit must have a successful main push CI run');
  requireUnusedRelease(tag);
  writeFileSync(
    env.GITHUB_OUTPUT,
    `source=${JSON.stringify({ ...source, ciRunId: checked.id })}\n`,
    {
      flag: 'a',
    },
  );
}

function verifyDraft(release, source) {
  assert.equal(release.tag_name, source.tag);
  assert.equal(release.draft, true);
  assert.equal(release.prerelease, false);
  assert.equal(release.published_at, null);
}

async function draft() {
  const source = JSON.parse(env.RELEASE_SOURCE);
  assert.equal(checkReleaseTag(source.tag), source.version);
  assert.equal(git('rev-parse', 'HEAD'), source.sha);
  assert.equal(git('rev-parse', 'HEAD^{tree}'), source.tree);
  assert.equal(git('status', '--porcelain'), '', 'Release build changed the checkout');
  verifyTag(source);
  requireUnusedRelease(source.tag);

  const evidence = join(env.RUNNER_TEMP, 'vigilshell-ci-evidence');
  assert.equal(readJson(join(evidence, 'result.json')).passed, true);
  assert.equal(readJson(join(evidence, 'signing-cleanup.json')).passed, true);
  const name = `VigilShell-${source.version}-macos-arm64.dmg`;
  const dmg = join('release', name);
  const checksum = `${dmg}.sha256`;
  const reportPath = join('release', 'mac-arm64', 'signing-report.json');
  const report = readJson(reportPath);
  assert.equal(report.identity, normalizeFingerprint(env.MACOS_SIGNING_IDENTITY));
  const sha256 = validatePreview(readFileSync(dmg), readFileSync(checksum, 'utf8'), report, source);
  const runUrl = `https://github.com/${env.GITHUB_REPOSITORY}/actions/runs/${env.GITHUB_RUN_ID}`;
  const temp = mkdtempSync(join(env.RUNNER_TEMP, 'vigilshell-release-'));
  try {
    const manifest = join(temp, 'build.json');
    writeFileSync(
      manifest,
      JSON.stringify(
        {
          ...source,
          platform: 'darwin',
          arch: 'arm64',
          preview: false,
          signing: 'fixed-self-signed',
          sha256,
          node: process.version,
          rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
          runId: env.GITHUB_RUN_ID,
          runAttempt: env.GITHUB_RUN_ATTEMPT,
          nativeStartup: true,
          signingCleanup: true,
        },
        null,
        2,
      ) + '\n',
    );
    const files = [dmg, checksum, reportPath, manifest];
    const release = api(['--method', 'POST', releases, '--input', '-'], {
      tag_name: source.tag,
      target_commitish: source.sha,
      name: `VigilShell ${source.version}`,
      body: `Asset upload and verification pending. Review the build before publishing.\n\n[Build run](${runUrl})`,
      draft: true,
      prerelease: false,
      make_latest: 'false',
    });
    verifyDraft(release, source);
    const endpoint = `${releases}/${release.id}`;
    console.log(`Created pending Release draft ${release.html_url}`);
    for (const file of files) {
      const response = await fetch(
        `https://uploads.github.com/${endpoint}/assets?name=${encodeURIComponent(basename(file))}`,
        {
          method: 'POST',
          headers: {
            Authorization: `Bearer ${env.GH_TOKEN}`,
            'Content-Type': 'application/octet-stream',
            'Content-Length': String(statSync(file).size),
          },
          body: createReadStream(file),
          duplex: 'half',
          redirect: 'error',
          signal: AbortSignal.timeout(300000),
        },
      );
      assert(response.ok, `Upload of ${basename(file)} failed with HTTP ${response.status}`);
      assert.equal((await response.json()).state, 'uploaded');
    }
    verifyTag(source);
    const complete = api([endpoint]);
    verifyDraft(complete, source);
    assert.equal(complete.assets.length, files.length);
    for (const file of files) {
      const asset = complete.assets.find((entry) => entry.name === basename(file));
      assert.equal(asset?.state, 'uploaded');
      assert.equal(
        asset.digest,
        `sha256:${createHash('sha256').update(readFileSync(file)).digest('hex')}`,
      );
    }
    const verified = api(['--method', 'PATCH', endpoint, '--input', '-'], {
      body: `macOS Apple Silicon installer signed with the fixed VigilShell certificate. The installer uses self-signing without Apple notarization.\n\nSource: \`${source.sha}\`. Runner startup, shutdown, and artifact checks passed.\n\n[Build run](${runUrl})`,
    });
    verifyDraft(verified, source);
    console.log(`Verified Release draft ${verified.html_url}`);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

assert.equal(env.GITHUB_EVENT_NAME, 'push');
assert.equal(env.GITHUB_REF_TYPE, 'tag');
switch (process.argv[2]) {
  case 'qualify':
    qualify();
    break;
  case 'draft':
    await draft();
    break;
  default:
    throw Error('Use qualify or draft');
}
