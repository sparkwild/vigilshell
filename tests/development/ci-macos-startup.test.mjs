import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { closeApplication, waitUntil, withCleanup } from '../e2e/ci-macos-lifecycle.mjs';

test('native visibility waits for the asynchronous window event', async () => {
  let observations = 0;
  await waitUntil(() => ++observations === 3, 'visible window', { intervalMs: 1 });
  assert.equal(observations, 3);
});

test('a window that never becomes visible fails instead of passing', async () => {
  await assert.rejects(
    waitUntil(() => false, 'visible window', { timeoutMs: 5, intervalMs: 1 }),
    /Timed out waiting for visible window/,
  );
});

test('an unresponsive native visibility query also reaches its deadline', async () => {
  await assert.rejects(
    waitUntil(() => new Promise(() => {}), 'native query', { timeoutMs: 5 }),
    /Timed out waiting for native query/,
  );
});

test('shutdown uses the retained process after Playwright disposes its dispatcher', async () => {
  const child = { exitCode: null, signalCode: null };
  await closeApplication(
    {
      close: async () => {
        child.exitCode = 0;
      },
      process: () => {
        throw new Error('dispatcher disposed');
      },
    },
    child,
  );
});

test('abnormal application exit still fails verification', async () => {
  await assert.rejects(
    closeApplication({ close: async () => {} }, { exitCode: 1, signalCode: null }),
    /Application must quit normally/,
  );
});

test('cleanup preserves the original failure and reports its own failure too', async () => {
  const original = new Error('window was not visible');
  const cleanup = new Error('close failed');
  await assert.rejects(
    withCleanup(
      async () => {
        throw original;
      },
      async () => {
        throw cleanup;
      },
    ),
    (error) => {
      assert.deepEqual(error.errors, [original, cleanup]);
      return true;
    },
  );
});

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
