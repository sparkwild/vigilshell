import assert from 'node:assert/strict';
import { waitUntil } from './ci-macos-lifecycle.mjs';

/** Re-scan after relaunch and bound cleanup to this disposable runner's test bundle. */
export async function stopHostedBundle(
  bundle,
  snapshot,
  {
    environment = process.env,
    signal = (pid, name) => process.kill(pid, name),
    graceMs = 3000,
    timeoutMs = 15000,
    intervalMs = 100,
  } = {},
) {
  assert.equal(environment.GITHUB_ACTIONS, 'true');
  assert.equal(environment.RUNNER_ENVIRONMENT, 'github-hosted');
  assert.equal(environment.RUNNER_OS, 'macOS');
  const started = Date.now();
  await waitUntil(
    () => {
      const owned = snapshot().filter((entry) => entry.executable.startsWith(bundle + '/'));
      if (!owned.length) return true;
      for (const entry of owned) {
        try {
          signal(entry.pid, Date.now() - started < graceMs ? 'SIGTERM' : 'SIGKILL');
        } catch (error) {
          if (error.code !== 'ESRCH') throw error;
        }
      }
      return false;
    },
    'hosted update process cleanup',
    { timeoutMs, intervalMs },
  );
}
