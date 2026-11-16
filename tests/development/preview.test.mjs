import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { validatePreview } from '../../scripts/package-policy.mjs';

for (const arch of ['arm64', 'x64'])
  test(`preview rejects corrupt bytes, wrong source, version and architecture (${arch})`, () => {
    const source = { sha: 'a'.repeat(40), version: '0.1.0' };
    const report = {
      sourceSha: source.sha,
      version: source.version,
      arch,
      profile: 'release',
      identity: 'A'.repeat(40),
    };
    const dmg = Buffer.from('fixture');
    const checksum = `${createHash('sha256').update(dmg).digest('hex')}  VigilShell-0.1.0-macos-${arch}.dmg\n`;
    assert(validatePreview(dmg, checksum, report, source, arch));
    assert.throws(() => validatePreview(Buffer.from('corrupt'), checksum, report, source, arch));
    for (const change of [
      { sourceSha: 'b'.repeat(40) },
      { version: '0.2.0' },
      { arch: arch === 'arm64' ? 'x64' : 'arm64' },
      { profile: 'dev' },
      { identity: '-' },
    ])
      assert.throws(() => validatePreview(dmg, checksum, { ...report, ...change }, source, arch));
  });
