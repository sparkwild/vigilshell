// @vitest-environment node
import { afterEach, expect, it, vi } from 'vitest';
import { IdentityVerifier, type NativeIdentityResult } from './identity';
const purpose = { challengeId: 'challenge', action: 'approveConnection', sessionId: 'session' };
function fixture() {
  vi.useFakeTimers();
  let reply!: (value: NativeIdentityResult) => void;
  const bridge = {
    verify: vi.fn((_reason: string, callback: typeof reply) => {
      reply = callback;
    }),
    cancel: vi.fn<() => NativeIdentityResult | undefined>(() => undefined),
  };
  const log = vi.fn();
  const visible = vi.fn(() => true);
  const verifier = new IdentityVerifier({
    bridge: () => bridge,
    visible,
    supported: () => true,
    log,
    timeoutMs: 100,
  });
  return {
    bridge,
    log,
    visible,
    verifier,
    reply: (outcome: NativeIdentityResult['outcome'], nativeCode = 0) =>
      reply({ outcome, nativeCode }),
  };
}
afterEach(() => vi.useRealTimers());
it('preserves success already completed natively when its JS callback races the deadline', async () => {
  const f = fixture();
  const result = f.verifier.verify('test', purpose);
  f.bridge.cancel.mockReturnValue({ outcome: 'approved', nativeCode: 0 });
  await vi.advanceTimersByTimeAsync(100);
  await expect(result).resolves.toBe(true);
  f.reply('approved');
  expect(f.log.mock.calls.filter(([v]) => v.phase === 'settled')).toHaveLength(1);
  expect(f.log.mock.calls.at(-1)?.[0]).toMatchObject({
    phase: 'late_native_reply',
    nativeOutcome: 'approved',
  });
});
it('times out a pending check and ignores success delivered after actual cancellation', async () => {
  const f = fixture();
  const result = f.verifier.verify('test', purpose);
  const failure = expect(result).rejects.toThrow('authorization_timed_out');
  await vi.advanceTimersByTimeAsync(100);
  await failure;
  f.reply('approved');
  expect(f.log.mock.calls.find(([v]) => v.phase === 'settled')?.[0]).toMatchObject({
    code: 'authorization_timed_out',
  });
});
it.each([
  ['userCancelled', 'authorization_cancelled'],
  ['systemCancelled', 'authorization_interrupted'],
  ['applicationCancelled', 'authorization_interrupted'],
  ['authenticationFailed', 'authorization_failed'],
  ['unavailable', 'system_authentication_unavailable'],
  ['invalidContext', 'authorization_interrupted'],
] as const)('keeps the native outcome %s distinct', async (outcome, code) => {
  const f = fixture();
  const result = f.verifier.verify('test', purpose);
  f.reply(outcome, -1);
  await expect(result).rejects.toThrow(code);
  expect(f.log.mock.calls.at(-1)?.[0]).toMatchObject({
    ...purpose,
    nativeOutcome: outcome,
    nativeCode: -1,
  });
});
it('reports hidden windows and concurrent verification without invoking or cancelling another check', async () => {
  const f = fixture();
  f.visible.mockReturnValue(false);
  await expect(f.verifier.verify('test', purpose)).rejects.toThrow('authorization_window_hidden');
  expect(f.bridge.verify).not.toHaveBeenCalled();
  f.visible.mockReturnValue(true);
  const first = f.verifier.verify('test', purpose);
  await expect(f.verifier.verify('test', purpose)).rejects.toThrow('authorization_busy');
  expect(f.bridge.cancel).not.toHaveBeenCalled();
  f.reply('approved');
  await expect(first).resolves.toBe(true);
});
it('application shutdown never recovers a previous success into a new authorization', async () => {
  const f = fixture();
  const result = f.verifier.verify('test', purpose);
  f.bridge.cancel.mockReturnValue({ outcome: 'approved', nativeCode: 0 });
  f.verifier.cancel();
  await expect(result).rejects.toThrow('authorization_interrupted');
  f.reply('approved');
});

it('does not let a diagnostic failure prevent a subsequent successful attempt', async () => {
  const f = fixture();
  const first = f.verifier.verify('test', purpose);
  f.verifier.cancel();
  await expect(first).rejects.toThrow('authorization_interrupted');
  f.log.mockImplementation(() => {
    throw Error('sink_closed');
  });
  const second = f.verifier.verify('test', { ...purpose, challengeId: 'next' });
  f.reply('approved');
  await expect(second).resolves.toBe(true);
});
