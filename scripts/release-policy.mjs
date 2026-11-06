import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function releaseSource(env) {
  assert.equal(env.GITHUB_REPOSITORY, 'sparkwild/vigilshell', 'Unexpected repository');
  if (env.GITHUB_EVENT_NAME === 'workflow_dispatch') {
    assert(
      ['refs/heads/main', 'refs/heads/develop'].includes(env.GITHUB_REF),
      'Verification requires main or develop',
    );
    return 'verify';
  }
  assert.equal(env.GITHUB_EVENT_NAME, 'push', 'Fixed signing rejects this event');
  assert(/^refs\/tags\/v\d+\.\d+\.\d+-beta\.[1-9]\d*$/.test(env.GITHUB_REF), 'Expected a Beta tag');
  return 'publish';
}

export function checkVersions(root = '.') {
  const read = (name) => readFileSync(resolve(root, name), 'utf8');
  const pkg = JSON.parse(read('package.json'));
  assert(
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-beta\.[1-9]\d*$/.test(pkg.version),
    'Expected a Beta version',
  );
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

export function verifySource(env = process.env) {
  const mode = releaseSource(env);
  const version = checkVersions();
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
  const sha = git('rev-parse', 'HEAD');
  assert.equal(sha, env.GITHUB_SHA, 'Checkout does not match workflow SHA');
  if (mode === 'publish') {
    const tag = `v${version}`;
    assert.equal(env.GITHUB_REF, `refs/tags/${tag}`, 'Tag and version differ');
    const api = (path) =>
      JSON.parse(
        execFileSync('gh', ['api', `repos/sparkwild/vigilshell/${path}`], { encoding: 'utf8' }),
      );
    assert(
      ['ahead', 'identical'].includes(api(`compare/${sha}...main`).status),
      'Source is not in remote main history',
    );
    let object = api(`git/ref/tags/${tag}`).object;
    while (object.type === 'tag') object = api(`git/tags/${object.sha}`).object;
    assert.equal(object.type, 'commit');
    assert.equal(object.sha, sha, 'Remote tag moved');
  }
  return { mode, version, sha };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(process.argv.includes('--source') ? verifySource() : { version: checkVersions() });
}
