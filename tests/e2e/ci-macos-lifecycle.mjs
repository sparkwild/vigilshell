import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';

async function withinTimeout(operation, description, timeoutMs) {
  let timer;
  try {
    return await Promise.race([
      Promise.resolve().then(operation),
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Timed out waiting for ${description}`)),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

export async function waitUntil(check, description, { timeoutMs = 10000, intervalMs = 100 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (!(await withinTimeout(check, description, Math.max(0, deadline - Date.now())))) {
    assert(Date.now() < deadline, `Timed out waiting for ${description}`);
    await delay(intervalMs);
  }
}

// Playwright disposes its Electron dispatcher on close; retain the original ChildProcess.
export async function closeApplication(application, child) {
  await withinTimeout(() => application.close(), 'application close', 10000);
  await waitUntil(
    () => child.exitCode !== null || child.signalCode !== null,
    'application process exit',
  );
  assert.equal(child.signalCode, null, 'Application must not be terminated by a signal');
  assert.equal(child.exitCode, 0, 'Application must quit normally');
}

export async function withCleanup(action, cleanup) {
  const failures = [];
  try {
    await action();
  } catch (error) {
    failures.push(error);
  }
  try {
    await cleanup();
  } catch (error) {
    failures.push(error);
  }
  if (failures.length) throw new AggregateError(failures, 'Native startup verification failed');
}
