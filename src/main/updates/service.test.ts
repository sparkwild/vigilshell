import { afterEach, describe, expect, it, vi } from 'vitest';
import { selectRelease, UpdateService } from './service';
const featureOverride = vi.hoisted(() => ({ automaticChecks: undefined as boolean | undefined }));
vi.mock('../../shared/application-features', async (importOriginal) => {
  const features = await importOriginal<typeof import('../../shared/application-features')>();
  return {
    ...features,
    get automaticUpdateChecksEnabled() {
      return featureOverride.automaticChecks ?? features.automaticUpdateChecksEnabled;
    },
  };
});
function release(v: string, extra = {}) {
  return {
    tag_name: `v${v}`,
    draft: false,
    prerelease: false,
    body: 'Changes',
    assets: [{ name: `VigilShell-${v}-macos-arm64.dmg`, state: 'uploaded' }],
    ...extra,
  };
}
describe('release selection', () => {
  it('orders stable versions numerically across patch, minor and major versions', () => {
    for (const [older, newer] of [
      ['0.1.2', '0.1.10'],
      ['0.2.9', '0.10.0'],
      ['0.99.9', '1.0.0'],
    ] as const) {
      expect(
        selectRelease([release(newer), release(older)], older, 'darwin', 'arm64').release?.version,
      ).toBe(newer);
    }
  });
  it('rejects prerelease tags even when marked stable and stable tags marked prerelease', () => {
    for (const item of [
      release('0.2.0-beta.1'),
      release('0.2.0-rc.1'),
      release('0.2.0', { prerelease: true }),
      release('0.2.0', { tag_name: '0.2.0' }),
    ]) {
      expect(selectRelease([item], '0.1.0', 'darwin', 'arm64').phase).toBe('empty');
    }
  });
  it('excludes drafts, incompatible assets and prereleases for stable users', () => {
    expect(selectRelease([release('0.2.0-beta.1')], '0.1.0', 'darwin', 'arm64').phase).toBe(
      'empty',
    );
    expect(
      selectRelease([release('0.2.0', { draft: true })], '0.1.0', 'darwin', 'arm64').phase,
    ).toBe('empty');
    expect(selectRelease([release('0.2.0')], '0.1.0', 'darwin', 'x64').phase).toBe('empty');
  });
  it('distinguishes current, empty and malformed responses', () => {
    expect(selectRelease([release('0.1.0')], '0.1.0', 'darwin', 'arm64').phase).toBe('current');
    expect(selectRelease([], '0.1.0', 'darwin', 'arm64').phase).toBe('empty');
    expect(() => selectRelease({}, '0.1.0', 'darwin', 'arm64')).toThrow();
  });
});
afterEach(() => {
  featureOverride.automaticChecks = undefined;
  vi.useRealTimers();
});
it('shares in-flight checks and reports failures without claiming current', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response('[]', { status: 429 }));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  const first = service.check();
  expect(service.check()).toBe(first);
  expect((await first).checkPhase).toBe('rateLimited');
  expect(fetcher).toHaveBeenCalledTimes(1);
  service.stop();
});
it('delays startup, cancels scheduling and coalesces resume', async () => {
  featureOverride.automaticChecks = true;
  vi.useFakeTimers();
  const fetcher = vi.fn().mockImplementation(async () => new Response('[]'));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  service.configure(true);
  await vi.advanceTimersByTimeAsync(29999);
  expect(fetcher).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(1);
  service.configure(false);
  await vi.advanceTimersByTimeAsync(86400000);
  service.resume();
  expect(fetcher).toHaveBeenCalledTimes(1);
  service.stop();
});

it('respects saved disabled preferences while manual checks remain available', async () => {
  vi.useFakeTimers();
  const fetcher = vi.fn().mockImplementation(async () => new Response('[]'));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  service.configure(false);
  await vi.advanceTimersByTimeAsync(86430000);
  expect(fetcher).not.toHaveBeenCalled();
  expect((await service.check()).phase).toBe('empty');
  service.resume();
  await vi.advanceTimersByTimeAsync(86400000);
  expect(fetcher).toHaveBeenCalledTimes(1);
  service.stop();
});
it('preserves discovered releases when background checks fail', async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(new Response(JSON.stringify([release('0.2.0')])))
    .mockRejectedValue(Error('offline'));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  await service.check();
  const result = await service.check('background');
  expect(result.phase).toBe('available');
  expect(result.release?.version).toBe('0.2.0');
  expect(result.checkPhase).toBe('networkError');
  service.stop();
});
it('enables internal download only after matching ZIP and metadata inspection', async () => {
  const item = release('0.2.0');
  const inspect = vi.fn(async () => true);
  const fetcher = vi.fn().mockImplementation(async () => new Response(JSON.stringify([item])));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher, { inspect });
  expect((await service.check()).canDownload).toBe(false);
  expect(inspect).not.toHaveBeenCalled();
  item.assets.push(
    { name: 'VigilShell-0.2.0-macos-arm64.zip', state: 'uploaded' },
    { name: 'latest-mac.yml', state: 'uploaded' },
  );
  expect((await service.check()).canDownload).toBe(true);
  inspect.mockRejectedValueOnce(Error('invalid_metadata'));
  expect((await service.check()).canDownload).toBe(false);
  service.stop();
});
it('distinguishes forbidden responses from rate limits and respects manual cooldown', async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(new Response('', { status: 403 }))
    .mockResolvedValue(new Response('', { status: 403, headers: { 'retry-after': '120' } }));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  expect((await service.check()).checkPhase).toBe('networkError');
  expect((await service.check()).checkPhase).toBe('rateLimited');
  await service.check();
  expect(fetcher).toHaveBeenCalledTimes(2);
  service.stop();
});
