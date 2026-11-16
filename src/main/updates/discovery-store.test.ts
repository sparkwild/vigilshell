import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import { DiscoveryStore } from './discovery-store';
import { initialSchedule } from './scheduler';
const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((path) => rm(path, { recursive: true, force: true })));
  vi.restoreAllMocks();
});
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'vs-discovery-'));
  roots.push(directory);
  const path = join(directory, 'discovery.json');
  return { path, store: new DiscoveryStore(path, 'dev|arm64', '0.1.2') };
}
it('restores discoveries offline, isolates sources and clears the installed version', async () => {
  const { path, store } = await fixture();
  const release = { version: '0.1.3', tag: 'v0.1.3', notes: 'Notes' };
  await store.save({
    release,
    canDownload: true,
    schedule: { ...initialSchedule(), lastSuccess: Date.now(), nextCheck: Date.now() + 86400000 },
  });
  expect((await store.load())?.release).toEqual(release);
  const log = vi.spyOn(console, 'error').mockImplementation(() => {});
  expect(await new DiscoveryStore(path, 'production|arm64', '0.1.2').load()).toBeUndefined();
  expect((await new DiscoveryStore(path, 'dev|arm64', '0.1.3').load())?.release).toBeNull();
  expect((await new DiscoveryStore(path, 'dev|arm64', '0.1.3').load())?.schedule).toEqual(
    initialSchedule(),
  );
  expect(log).toHaveBeenCalled();
});
it('treats corrupt records and write failures as recoverable discovery failures', async () => {
  const { path, store } = await fixture();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  await writeFile(path, '{');
  expect(await store.load()).toBeUndefined();
  const blocked = new DiscoveryStore(join(path, 'discovery.json'), 'dev', '0.1.2');
  await expect(
    blocked.save({ release: null, canDownload: false, schedule: initialSchedule() }),
  ).resolves.toBeUndefined();
});

it('retains a discovered version offline when optional translated copy is invalid', async () => {
  const { path, store } = await fixture();
  const localizedNotes = {
    en: { features: ['Feature'], fixes: ['Fix'] },
    'zh-CN': { features: ['功能'], fixes: ['修复'] },
    fr: { features: ['Fonction'], fixes: ['Correction'] },
  };
  await store.save({
    release: { version: '0.1.3', tag: 'v0.1.3', notes: '', localizedNotes },
    canDownload: true,
    schedule: initialSchedule(),
  });
  expect((await store.load())?.release?.localizedNotes).toEqual(localizedNotes);
  const record = {
    schema: 1,
    source: 'dev|arm64',
    current: '0.1.2',
    release: { version: '0.1.3', tag: 'v0.1.3', notes: null, localizedNotes: {} },
    canDownload: true,
    schedule: initialSchedule(),
  };
  await writeFile(path, JSON.stringify(record));
  expect(await store.load()).toMatchObject({
    canDownload: true,
    release: { version: '0.1.3', notes: '' },
  });
});
