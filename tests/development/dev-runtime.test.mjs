import assert from 'node:assert/strict';
import { test } from 'node:test';
import { RuntimeCoordinator, vaultReadOutcome } from '../../scripts/dev-runtime.mjs';
const tick = () => new Promise((resolve) => setTimeout(resolve, 25));
function fixture() {
  const starts = [];
  const runtime = new RuntimeCoordinator({
    delay: 5,
    fingerprint: async (v) => v.hash,
    replace: async (v) => starts.push(v.hash),
    report: () => {},
  });
  return { runtime, starts };
}
test('unchanged built output preserves runtime and bursts launch only latest output', async () => {
  const { runtime, starts } = fixture();
  try {
    runtime.request({ hash: 'a' });
    await tick();
    runtime.request({ hash: 'a' });
    await tick();
    assert.deepEqual(starts, ['a']);
    runtime.request({ hash: 'b' });
    runtime.request({ hash: 'c' });
    await tick();
    assert.deepEqual(starts, ['a', 'c']);
  } finally {
    runtime.close();
  }
});
test('authorization and native build both gate replacement, retaining latest request', async () => {
  const { runtime, starts } = fixture();
  try {
    runtime.request({ hash: 'a' });
    await tick();
    runtime.authorization(true);
    runtime.build(true);
    runtime.request({ hash: 'b' });
    runtime.request({ hash: 'c' });
    runtime.build(false);
    await tick();
    assert.deepEqual(starts, ['a']);
    runtime.authorization(false);
    await tick();
    assert.deepEqual(starts, ['a', 'c']);
  } finally {
    runtime.close();
  }
});
test('authorization beginning while fingerprint is read cancels that replacement', async () => {
  let resolve;
  const starts = [];
  const runtime = new RuntimeCoordinator({
    delay: 5,
    fingerprint: () =>
      new Promise((r) => {
        resolve = r;
      }),
    replace: async () => starts.push(1),
  });
  try {
    runtime.request({});
    await tick();
    runtime.authorization(true);
    resolve('a');
    await tick();
    assert.deepEqual(starts, []);
    runtime.authorization(false);
    await tick();
    resolve('a');
    await tick();
    assert.deepEqual(starts, [1]);
  } finally {
    runtime.close();
  }
});
test('only structured vault events release the authorization gate, including early failure', () => {
  const event = (stage, outcome) =>
    JSON.stringify({ fields: { application: 'vigilshell', stage, outcome } });
  assert.equal(vaultReadOutcome(event('vault_key_read', 'started')), true);
  assert.equal(vaultReadOutcome(event('vault_key_read', 'ok')), false);
  assert.equal(vaultReadOutcome(event('vault_key_read', 'failed')), false);
  assert.equal(vaultReadOutcome(event('vault_initialization', 'finished')), false);
  assert.equal(vaultReadOutcome('untrusted message vault_key_read outcome=ok'), null);
});
test('failed Rust transaction cannot launch partially regenerated Main output', async () => {
  const { runtime, starts } = fixture();
  try {
    runtime.request({ hash: 'valid' });
    await tick();
    runtime.build(true);
    runtime.request({ hash: 'partial' });
    runtime.build(false, false);
    await tick();
    runtime.request({ hash: 'late partial' });
    await tick();
    assert.deepEqual(starts, ['valid']);
    runtime.build(true);
    runtime.request({ hash: 'recovered' });
    runtime.build(false, true);
    await tick();
    assert.deepEqual(starts, ['valid', 'recovered']);
  } finally {
    runtime.close();
  }
});
