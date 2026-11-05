import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

for (const [name, github, runner] of [
  ['local developer machine', '', ''],
  ['self-hosted runner', 'true', 'self-hosted'],
]) {
  test(`CI startup refuses a ${name} before accessing application data`, () => {
    const result = spawnSync(process.execPath, ['tests/e2e/ci-macos-startup.mjs'], {
      encoding: 'utf8',
      env: { ...process.env, GITHUB_ACTIONS: github, RUNNER_ENVIRONMENT: runner },
      timeout: 10000,
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Startup smoke requires a disposable GitHub-hosted macOS runner/);
  });
}
