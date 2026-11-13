import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkVersions } from './release-policy.mjs';

export function validatePreview(dmg, checksum, report, source) {
  assert.equal(report.version, source.version);
  assert.equal(report.sourceSha, source.sha);
  assert.equal(report.arch, 'arm64');
  assert.equal(report.profile, 'release');
  assert(/^[A-F0-9]{40}$/.test(report.identity));
  const name = `VigilShell-${source.version}-macos-arm64.dmg`;
  const digest = createHash('sha256').update(dmg).digest('hex');
  assert.equal(checksum.trim(), `${digest}  ${name}`);
  return digest;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
  if (process.argv[2] === 'capture') {
    assert.equal(git('status', '--porcelain'), '', 'Preview requires a clean checkout');
    const sha = git('rev-parse', 'HEAD');
    mkdirSync('.cache', { recursive: true });
    writeFileSync(
      '.cache/preview-source.json',
      JSON.stringify({ sha, tree: git('rev-parse', 'HEAD^{tree}'), version: checkVersions() }),
    );
    if (process.env.GITHUB_OUTPUT)
      writeFileSync(process.env.GITHUB_OUTPUT, `sha=${sha}\n`, { flag: 'a' });
  } else {
    assert.equal(process.argv[2], 'collect');
    const source = JSON.parse(readFileSync('.cache/preview-source.json', 'utf8'));
    assert.equal(git('rev-parse', 'HEAD'), source.sha);
    assert.equal(git('status', '--porcelain'), '', 'Build changed tracked source');
    const name = `VigilShell-${source.version}-macos-arm64.dmg`;
    const reportPath = 'release/mac-arm64/signing-report.json';
    const sha256 = validatePreview(
      readFileSync(`release/${name}`),
      readFileSync(`release/${name}.sha256`, 'utf8'),
      JSON.parse(readFileSync(reportPath, 'utf8')),
      source,
    );
    mkdirSync('preview', { recursive: true });
    const archive = `release/VigilShell-${source.version}-macos-arm64.zip`;
    for (const path of [
      `release/${name}`,
      `release/${name}.sha256`,
      archive,
      `${archive}.sha256`,
      `${archive}.blockmap`,
      'release/latest-mac.yml',
      reportPath,
    ])
      copyFileSync(path, `preview/${basename(path)}`);
    writeFileSync(
      'preview/build.json',
      JSON.stringify(
        {
          ...source,
          platform: 'darwin',
          arch: 'arm64',
          preview: true,
          signing: 'disposable-self-signed',
          sha256,
          node: process.version,
          rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
          runId: process.env.GITHUB_RUN_ID,
          runAttempt: process.env.GITHUB_RUN_ATTEMPT,
        },
        null,
        2,
      ) + '\n',
    );
  }
}
