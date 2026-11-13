import { expect, it, vi } from 'vitest';
import { NativeUpdateUncertain, UpdateInstallation } from './installation';
function fixture() {
  const calls: string[] = [];
  const ports = {
    confirm: vi.fn(async () => true),
    lock: vi.fn((value: boolean) => {
      calls.push(`lock:${value}`);
    }),
    flush: vi.fn(async () => {
      calls.push('flush');
    }),
    shutdown: vi.fn(async () => {
      calls.push('shutdown');
    }),
    recover: vi.fn(),
  };
  const prepare = vi.fn(async () => {
    calls.push('prepare');
  });
  const commit = vi.fn(() => {
    calls.push('commit');
  });
  return { calls, ports, prepare, commit, installer: new UpdateInstallation(ports) };
}
it('coalesces installs and saves before native preparation and teardown', async () => {
  const f = fixture();
  const first = f.installer.install(f.prepare, f.commit);
  expect(f.installer.install(f.prepare, f.commit)).toBe(first);
  expect(await first).toBe(true);
  expect(f.calls).toEqual(['lock:true', 'flush', 'prepare', 'shutdown', 'commit']);
  expect(f.ports.confirm).toHaveBeenCalledOnce();
});
it('cancelling does not save, arm Squirrel or end sessions', async () => {
  const f = fixture();
  f.ports.confirm.mockResolvedValue(false);
  expect(await f.installer.install(f.prepare, f.commit)).toBe(false);
  expect(f.calls).toEqual([]);
});
it.each(['save', 'native'])('keeps services running and unlocks after %s failure', async (kind) => {
  const f = fixture();
  (kind === 'save' ? f.ports.flush : f.prepare).mockRejectedValue(Error('failed'));
  await expect(f.installer.install(f.prepare, f.commit)).rejects.toThrow('failed');
  expect(f.ports.shutdown).not.toHaveBeenCalled();
  expect(f.commit).not.toHaveBeenCalled();
  expect(f.ports.lock).toHaveBeenLastCalledWith(false);
});
it('recovers by restarting after uncertain native preparation instead of returning to ordinary work', async () => {
  const f = fixture();
  f.prepare.mockRejectedValue(new NativeUpdateUncertain('timeout'));
  await expect(f.installer.install(f.prepare, f.commit)).rejects.toThrow('timeout');
  expect(f.ports.shutdown).toHaveBeenCalledOnce();
  expect(f.ports.recover).toHaveBeenCalledOnce();
  expect(f.ports.lock).not.toHaveBeenCalledWith(false);
});
