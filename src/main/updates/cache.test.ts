import { afterEach, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { UpdateCache, verifiedInfo } from './cache';
const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});
const release = { version: '0.1.2', tag: 'v0.1.2', notes: 'Notes' };
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-update-cache-'));
  roots.push(root);
  const source = join(root, 'download.zip');
  const bytes = Buffer.from('verified archive fixture');
  await writeFile(source, bytes);
  const hash = createHash('sha512').update(bytes).digest('base64');
  const info = {
    version: release.version,
    path: '',
    sha512: hash,
    releaseDate: '',
    files: [{ url: 'VigilShell-0.1.2-macos-arm64.zip', sha512: hash, size: bytes.length }],
  };
  const directory = join(root, 'cache');
  const cache = new UpdateCache(directory, '0.1.1', 'arm64');
  await cache.save(release, info, source);
  return { directory, cache, info };
}
it('restores a verified pending update offline and consumes a successful installation receipt', async () => {
  const { directory, cache } = await fixture();
  const saved = await cache.load();
  expect(saved?.release).toEqual(release);
  await cache.write({ ...saved!, installing: true });
  expect((await new UpdateCache(directory, '0.1.1', 'arm64').load())?.installing).toBe(true);
  expect(await new UpdateCache(directory, '0.1.2', 'arm64').load()).toBeUndefined();
  await expect(readFile(cache.archive)).rejects.toHaveProperty('code', 'ENOENT');
});
it('rejects a corrupted ZIP and an architecture mismatch instead of restoring ready', async () => {
  const { directory, cache } = await fixture();
  await expect(new UpdateCache(directory, '0.1.1', 'x64').load()).rejects.toThrow();
  await writeFile(cache.archive, 'tampered archive');
  await expect(cache.load()).rejects.toThrow('invalid_cache');
});
it('rejects metadata that changes the selected version or redirects downloads', async () => {
  const { info } = await fixture();
  expect(() => verifiedInfo({ ...info, version: '0.2.0' }, release, 'arm64')).toThrow();
  expect(() =>
    verifiedInfo(
      { ...info, files: [{ ...info.files[0]!, url: 'https://other.invalid/update.zip' }] },
      release,
      'arm64',
    ),
  ).toThrow();
});
it('preserves an older ready update when committing a replacement receipt fails', async () => {
  const { cache, directory } = await fixture();
  const next = { version: '0.1.3', tag: 'v0.1.3', notes: 'Newer' };
  const source = join(directory, 'next.zip');
  const bytes = Buffer.from('replacement archive');
  await writeFile(source, bytes);
  const hash = createHash('sha512').update(bytes).digest('base64');
  const info = {
    version: next.version,
    path: '',
    sha512: hash,
    releaseDate: '',
    files: [{ url: 'VigilShell-0.1.3-macos-arm64.zip', sha512: hash, size: bytes.length }],
  };
  vi.spyOn(cache, 'write').mockRejectedValueOnce(Error('disk_full'));
  await expect(cache.save(next, info, source)).rejects.toThrow('disk_full');
  expect((await cache.load())?.release.version).toBe('0.1.2');
});

it('restores locale documents offline and never discards a verified ZIP for malformed copy', async () => {
  const { cache, directory } = await fixture();
  const saved = (await cache.load())!;
  const localizedNotes = {
    en: { features: ['Feature'], upgradeNotes: ['Back up'] },
    'zh-CN': { features: ['功能'], upgradeNotes: ['备份'] },
    fr: { features: ['Fonction'], upgradeNotes: ['Sauvegarder'] },
  };
  await cache.write({ ...saved, release: { ...saved.release, localizedNotes } });
  expect((await cache.load())?.release.localizedNotes).toEqual(localizedNotes);
  const path = join(directory, 'pending.json');
  const broken = JSON.parse(await readFile(path, 'utf8'));
  broken.release.notes = null;
  broken.release.localizedNotes = { en: { features: [42] } };
  await writeFile(path, JSON.stringify(broken));
  expect((await cache.load())?.release).toEqual({ ...release, notes: '' });
  expect((await readFile(cache.archive)).length).toBeGreaterThan(0);
});
