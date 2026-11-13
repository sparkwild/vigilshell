import { expect, it, vi } from 'vitest';
import { loadReleaseNotes } from './release-notes';
import { UpdateService } from './service';
const release = { version: '0.2.0', tag: 'v0.2.0', notes: '## Features\n- English fallback' };
const document = {
  schema: 1,
  version: release.version,
  tag: release.tag,
  languages: { en: { features: ['New feature'] }, 'zh-CN': { features: ['新功能'] } },
};

it('loads bounded optional copy and preserves the selected update on every copy failure', async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json(document));
  expect(
    (await loadReleaseNotes(release, 'http://127.0.0.1/notes', fetcher)).localizedNotes,
  ).toEqual(document.languages);
  expect(fetcher.mock.calls[0]![1]).toMatchObject({ credentials: 'omit' });
  for (const response of [
    new Response('', { status: 404 }),
    new Response('{'),
    Response.json({ ...document, tag: 'v0.3.0' }),
    new Response('', { headers: { 'content-length': '65537' } }),
    new Response('x'.repeat(65537)),
  ]) {
    expect(
      await loadReleaseNotes(
        release,
        'http://127.0.0.1/notes',
        vi.fn().mockResolvedValue(response),
      ),
    ).toEqual(release);
  }
  expect(
    await loadReleaseNotes(
      release,
      'http://127.0.0.1/notes',
      vi.fn().mockRejectedValue(new DOMException('timeout', 'TimeoutError')),
    ),
  ).toEqual(release);
});

it('fetches notes only for the selected release and keeps download eligibility when notes fail', async () => {
  const item = (version: string) => ({
    tag_name: `v${version}`,
    draft: false,
    prerelease: false,
    body: release.notes,
    assets: [
      { name: `VigilShell-${version}-macos-arm64.dmg`, state: 'uploaded' },
      { name: `VigilShell-${version}-macos-arm64.zip`, state: 'uploaded' },
      { name: 'latest-mac.yml', state: 'uploaded' },
      { name: 'release-notes.json', state: 'uploaded' },
    ],
  });
  const fetcher = vi
    .fn()
    .mockImplementation(async (url: string) =>
      url.endsWith('/releases') ? Response.json([item('0.1.5'), item('0.2.0')]) : new Response('{'),
    );
  const inspect = vi.fn().mockResolvedValue(true);
  const service = new UpdateService('0.1.0', 'darwin', 'arm64', fetcher, {
    releases: 'http://127.0.0.1/releases',
    notes: (tag) => `http://127.0.0.1/${tag}/release-notes.json`,
    inspect,
  });
  try {
    const snapshot = await service.check();
    expect(snapshot).toMatchObject({
      phase: 'available',
      checkPhase: 'available',
      canDownload: true,
      release,
    });
    expect(fetcher.mock.calls.map((call) => call[0])).toEqual([
      'http://127.0.0.1/releases',
      'http://127.0.0.1/v0.2.0/release-notes.json',
    ]);
    expect(inspect).toHaveBeenCalledOnce();
  } finally {
    service.stop();
  }
});
