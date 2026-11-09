import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { validatePreview } from '../../scripts/preview-artifacts.mjs';

test('preview rejects corrupt bytes, wrong source, version and architecture', () => {
  const source = { sha: 'a'.repeat(40), version: '0.1.0' };
  const report = {
    sourceSha: source.sha,
    version: source.version,
    arch: 'arm64',
    profile: 'release',
    identity: 'A'.repeat(40),
  };
  const dmg = Buffer.from('fixture');
  const checksum = `${createHash('sha256').update(dmg).digest('hex')}  VigilShell-0.1.0-macos-arm64.dmg\n`;
  assert(validatePreview(dmg, checksum, report, source));
  assert.throws(() => validatePreview(Buffer.from('corrupt'), checksum, report, source));
  for (const change of [
    { sourceSha: 'b'.repeat(40) },
    { version: '0.2.0' },
    { arch: 'x64' },
    { profile: 'dev' },
    { identity: '-' },
  ])
    assert.throws(() => validatePreview(dmg, checksum, { ...report, ...change }, source));
});
