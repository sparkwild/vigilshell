import test from 'node:test';
import assert from 'node:assert/strict';
import { createProcessGroupStopper } from '../../scripts/process-group.mjs';
test('cleanup is idempotent and never signals a stale group twice', async () => {
  const signals = [];
  const stop = createProcessGroupStopper(42, {
    kill: (...args) => signals.push(args),
    hasMembers: () => true,
  });
  await Promise.all([stop(), stop()]);
  assert.deepEqual(signals, [[-42, 'SIGTERM']]);
});
test('an empty macOS group is already stopped but surviving denied members remain an error', async () => {
  const denied = Object.assign(Error('denied'), { code: 'EPERM' });
  const stop = createProcessGroupStopper(42, {
    kill: () => {
      throw denied;
    },
    hasMembers: () => false,
  });
  await assert.doesNotReject(stop);
  const active = createProcessGroupStopper(42, {
    kill: () => {
      throw denied;
    },
    hasMembers: () => true,
  });
  await assert.rejects(active, (error) => error === denied);
});

test('waits for naturally exiting children before accepting EPERM', async () => {
  let observations = 0;
  const stop = createProcessGroupStopper(42, {
    kill: () => {
      throw Object.assign(Error('exit race'), { code: 'EPERM' });
    },
    hasMembers: () => ++observations < 3,
  });
  await stop();
  assert.equal(observations, 3);
});
