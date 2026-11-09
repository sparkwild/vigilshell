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
  expect((await first).phase).toBe('rateLimited');
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

it('keeps automatic checks disabled for saved enabled preferences while manual checks still work', async () => {
  vi.useFakeTimers();
  const fetcher = vi.fn().mockImplementation(async () => new Response('[]'));
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher);
  service.configure(true);
  await vi.advanceTimersByTimeAsync(30000 + 86400000);
  service.resume();
  service.configure(true);
  expect(fetcher).not.toHaveBeenCalled();
  expect((await service.check()).phase).toBe('empty');
  await vi.advanceTimersByTimeAsync(86400000);
  service.resume();
  expect(fetcher).toHaveBeenCalledTimes(1);
  service.stop();
});
