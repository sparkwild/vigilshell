import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { inspectUpdateArtifacts } from '../../scripts/verify-update-artifacts.mjs';
test('release preflight rejects mutated archives, wrong architecture and DMG updater entries', async (t) => {
  const directory = await mkdtemp(join(tmpdir(), 'vigilshell-artifacts-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const bytes = Buffer.from('finished archive');
  const name = 'VigilShell-0.1.2-macos-arm64.zip';
  const file = {
    url: name,
    size: bytes.length,
    sha512: createHash('sha512').update(bytes).digest('base64'),
  };
  const metadata = { version: '0.1.2', files: [file] };
  await writeFile(join(directory, name), bytes);
  await writeFile(join(directory, 'latest-mac.yml'), JSON.stringify(metadata));
  assert.equal(
    inspectUpdateArtifacts(directory, '0.1.2', 'arm64').sha256,
    createHash('sha256').update(bytes).digest('hex'),
  );
  assert.throws(() => inspectUpdateArtifacts(directory, '0.1.2', 'x64'));
  await writeFile(join(directory, name), 'mutated content');
  assert.throws(() => inspectUpdateArtifacts(directory, '0.1.2', 'arm64'));
  await writeFile(join(directory, name), bytes);
  await writeFile(
    join(directory, 'latest-mac.yml'),
    JSON.stringify({ ...metadata, files: [...metadata.files, { ...file, url: 'installer.dmg' }] }),
  );
  assert.throws(() => inspectUpdateArtifacts(directory, '0.1.2', 'arm64'));
});
