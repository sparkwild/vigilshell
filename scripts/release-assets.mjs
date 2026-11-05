import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export function validateAssets(root, version, sha) {
  function files(directory) {
    return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? files(path) : [path];
    });
  }
  const assets = files(root);
  const name = `VigilShell-${version}-macos-arm64.dmg`;
  const dmg = join(root, name);
  const checksum = `${dmg}.sha256`;
  const reports = assets.filter((path) => path.endsWith('/signing-report.json'));
  assert.equal(reports.length, 1);
  assert.equal(assets.length, 3, 'Unexpected release assets');
  const digest = createHash('sha256').update(readFileSync(dmg)).digest('hex');
  assert.equal(readFileSync(checksum, 'utf8').trim(), `${digest}  ${name}`);
  const report = JSON.parse(readFileSync(reports[0], 'utf8'));
  assert.equal(report.version, version);
  assert.equal(report.sourceSha, sha);
  assert.equal(report.arch, 'arm64');
  assert.equal(report.profile, 'release');
  assert(/^[A-F0-9]{40}$/.test(report.identity));

  return { dmg, checksum, report: reports[0] };
}

export function validateDraft(release, sha) {
  assert(release.draft, 'Refusing to overwrite a published release');
  assert.equal(release.target_commitish, sha, 'Draft belongs to another source SHA');
}
