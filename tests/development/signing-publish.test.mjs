import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, writeFile, readFile, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { publishArtifacts } from '../../scripts/publish-artifacts.mjs';

test('publishing restores previous artifacts on failure and preserves other channels', async () => {
  const root = await mkdtemp(join(tmpdir(), 'signing-publish-'));
  const stage = join(root, 'stage');
  const output = join(root, 'output');
  try {
    await mkdir(stage);
    await mkdir(output);
    await writeFile(join(stage, 'app'), 'new');
    await writeFile(join(stage, 'report'), 'verified');
    await writeFile(join(output, 'app'), 'old');
    await writeFile(join(output, 'dev'), 'unrelated');
    await assert.rejects(
      publishArtifacts(stage, output, async (from, to) => {
        if (from === join(stage, 'report')) throw new Error('simulated disk failure');
        return rename(from, to);
      }),
      /simulated disk failure/,
    );
    assert.equal(await readFile(join(output, 'app'), 'utf8'), 'old');
    assert.equal(await readFile(join(output, 'dev'), 'utf8'), 'unrelated');
    await publishArtifacts(stage, output);
    assert.equal(await readFile(join(output, 'app'), 'utf8'), 'new');
    assert.equal(await readFile(join(output, 'report'), 'utf8'), 'verified');
    assert.equal(await readFile(join(output, 'dev'), 'utf8'), 'unrelated');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
