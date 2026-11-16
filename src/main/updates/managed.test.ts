import { expect, it, vi } from 'vitest';
import { ManagedUpdates, type UpdateEngine } from './managed';
import type { PendingUpdate } from './cache';
const release = { version: '0.1.2', tag: 'v0.1.2', notes: 'Notes' };
function fixture() {
  const engine: UpdateEngine = {
    cache: { load: vi.fn(async () => undefined), clear: vi.fn(async () => {}) },
    download: vi.fn(async () => {}),
    inspect: vi.fn(async () => true),
    prepare: vi.fn(async () => {}),
    commit: vi.fn(),
    cancel: vi.fn(() => true),
    stop: vi.fn(),
  };
  const fetcher = vi.fn();
  const install = vi.fn(async (prepare: () => Promise<void>, commit: () => void) => {
    await prepare();
    commit();
    return true;
  });
  const service = new ManagedUpdates('0.1.1', 'darwin', 'arm64', fetcher, engine, install);
  service.snapshot = { phase: 'available', release, canDownload: true };
  return { engine, fetcher, install, service };
}
it('coalesces download and concurrent checks and ignores late progress after cancellation', async () => {
  const f = fixture();
  let finish!: () => void;
  let progress!: Parameters<UpdateEngine['download']>[1];
  vi.mocked(f.engine.download).mockImplementation((_release, onProgress) => {
    progress = onProgress;
    return new Promise((resolve) => {
      finish = resolve;
    });
  });
  const pending = f.service.download();
  expect(f.service.download()).toBe(pending);
  expect(f.service.check()).toBe(pending);
  f.service.cancel();
  progress({ transferred: 10, total: 10, percent: 100 });
  expect(f.service.snapshot.progress).toBeUndefined();
  finish();
  await pending;
  expect(f.service.snapshot.phase).toBe('available');
  expect(f.fetcher).not.toHaveBeenCalled();
});
it('preserves the selected version after download failure and supports explicit retry', async () => {
  const f = fixture();
  vi.mocked(f.engine.download).mockRejectedValueOnce(Error('offline'));
  expect((await f.service.download()).phase).toBe('downloadError');
  expect(f.service.snapshot.release).toEqual(release);
  expect((await f.service.download()).phase).toBe('ready');
});
it('keeps the pending package selected instead of checking a newer release', async () => {
  const f = fixture();
  vi.mocked(f.engine.cache.load).mockResolvedValue({ release, installing: false } as PendingUpdate);
  await f.service.restore();
  f.fetcher.mockResolvedValue(
    new Response(
      JSON.stringify([
        {
          tag_name: 'v0.1.3',
          draft: false,
          prerelease: false,
          body: 'Newer',
          assets: [{ name: 'VigilShell-0.1.3-macos-arm64.dmg', state: 'uploaded' }],
        },
      ]),
    ),
  );
  const check = f.service.check();
  expect((await check).release?.version).toBe(release.version);
  expect(f.fetcher).not.toHaveBeenCalled();
  expect(f.engine.cache.clear).not.toHaveBeenCalled();
});
it('keeps ready when an explicit recheck finds the already downloaded release', async () => {
  const f = fixture();
  vi.mocked(f.engine.cache.load).mockResolvedValue({ release, installing: false } as PendingUpdate);
  await f.service.restore();
  f.fetcher.mockResolvedValue(
    new Response(
      JSON.stringify([
        {
          tag_name: release.tag,
          draft: false,
          prerelease: false,
          body: '',
          assets: [{ name: 'VigilShell-0.1.2-macos-arm64.dmg', state: 'uploaded' }],
        },
      ]),
    ),
  );
  expect((await f.service.check()).phase).toBe('ready');
});
it('restores pending or failed installation receipts without network checks', async () => {
  for (const installing of [false, true]) {
    const f = fixture();
    vi.mocked(f.engine.cache.load).mockResolvedValue({ release, installing } as PendingUpdate);
    await f.service.restore();
    expect(f.service.snapshot.phase).toBe(installing ? 'installError' : 'ready');
    expect(f.fetcher).not.toHaveBeenCalled();
  }
});
it('does not arm or quit when installation is cancelled, and reports preparation failure', async () => {
  const f = fixture();
  f.service.snapshot.phase = 'ready';
  f.install.mockResolvedValueOnce(false);
  expect((await f.service.install()).phase).toBe('ready');
  expect(f.engine.prepare).not.toHaveBeenCalled();
  vi.mocked(f.engine.prepare).mockRejectedValueOnce(Error('signature_invalid'));
  expect((await f.service.install()).phase).toBe('installError');
  expect(f.engine.commit).not.toHaveBeenCalled();
});

it.each(['network', 'rate-limit', 'invalid-json', 'invalid-release'])(
  'keeps a downloaded package installable after %s recheck failure',
  async (failure) => {
    const f = fixture();
    await f.service.download();
    if (failure === 'network') f.fetcher.mockRejectedValue(Error('offline'));
    else if (failure === 'rate-limit')
      f.fetcher.mockResolvedValue(new Response('', { status: 429 }));
    else f.fetcher.mockResolvedValue(new Response(failure === 'invalid-json' ? '{' : '{}'));
    expect((await f.service.check()).phase).toBe('ready');
    expect(f.service.snapshot.release).toEqual(release);
    expect(f.engine.cache.clear).not.toHaveBeenCalled();
    expect((await f.service.install()).phase).toBe('installing');
    expect(f.engine.prepare).toHaveBeenCalledOnce();
    expect(f.engine.commit).toHaveBeenCalledOnce();
  },
);
it('keeps discovery failures visible when no verified package is pending', async () => {
  const f = fixture();
  f.fetcher.mockRejectedValue(Error('offline'));
  expect((await f.service.check()).checkPhase).toBe('networkError');
  expect(f.service.snapshot.phase).toBe('available');
  await f.service.install();
  expect(f.engine.prepare).not.toHaveBeenCalled();
});
