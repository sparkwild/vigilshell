import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { validatePreview } from './package-policy.mjs';
import { inspectUpdateArtifacts } from './verify-update-artifacts.mjs';
import { normalizeFingerprint } from './signing/config.mjs';

export const releaseArchitectures = ['arm64', 'x64'];
const json = (path) => JSON.parse(readFileSync(path, 'utf8'));
const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
const writeJson = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n');

/** Collect only after native verification and signing-key cleanup have both succeeded. */
export function collectReleaseBuild(source, environment = process.env, preview = false) {
  const arch = environment.EXPECTED_ARCH;
  assert(releaseArchitectures.includes(arch));
  assert.equal(arch, process.arch);
  const evidence = join(environment.RUNNER_TEMP, 'vigilshell-ci-evidence');
  assert.equal(json(join(evidence, 'result.json')).passed, true);
  assert.equal(json(join(evidence, 'signing-cleanup.json')).passed, true);
  if (preview) {
    const updateResult = json(join(evidence, 'update-result.json'));
    assert.equal(updateResult.passed, true);
    assert.equal(updateResult.arch, arch);
    assert.equal(updateResult.sourceSha, source.sha);
    assert.equal(updateResult.sourceDirty, false);
  }
  const reportPath = join('release', arch === 'arm64' ? 'mac-arm64' : 'mac', 'signing-report.json');
  const report = json(reportPath);
  const identity = normalizeFingerprint(
    preview ? environment.VIGILSHELL_RELEASE_SIGNING_IDENTITY : environment.MACOS_SIGNING_IDENTITY,
  );
  assert.equal(report.identity, identity);
  const dmg = `VigilShell-${source.version}-macos-${arch}.dmg`;
  validatePreview(
    readFileSync(join('release', dmg)),
    readFileSync(join('release', `${dmg}.sha256`), 'utf8'),
    report,
    source,
    arch,
  );
  const update = inspectUpdateArtifacts('release', source.version, arch);
  assert.equal(
    readFileSync(`${update.archive}.sha256`, 'utf8').trim(),
    `${update.sha256}  ${basename(update.archive)}`,
  );
  const directory = preview ? 'preview' : 'release-build';
  mkdirSync(directory, { recursive: true });
  const files = [
    dmg,
    `${dmg}.sha256`,
    basename(update.archive),
    `${basename(update.archive)}.sha256`,
    `${basename(update.archive)}.blockmap`,
    'latest-mac.yml',
  ];
  for (const file of files) copyFileSync(join('release', file), join(directory, file));
  copyFileSync(reportPath, join(directory, 'signing-report.json'));
  files.push('signing-report.json');
  writeJson(join(directory, 'build.json'), {
    ...source,
    platform: 'darwin',
    arch,
    preview,
    signing: preview ? 'disposable-self-signed' : 'fixed-self-signed',
    identity,
    node: process.version,
    rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
    runId: environment.GITHUB_RUN_ID,
    runAttempt: environment.GITHUB_RUN_ATTEMPT,
    nativeStartup: true,
    signingCleanup: true,
    ...(preview ? { nativeUpdate: true } : {}),
    files: Object.fromEntries(files.map((file) => [file, digest(join(directory, file))])),
  });
}

/** Assemble one release only when both native builds belong to the same source and run. */
export function aggregateReleaseBuilds(
  input,
  output,
  source,
  identity,
  runId,
  runAttempt,
  preview = false,
  previewAttempts,
) {
  if (previewAttempts !== undefined) {
    assert.equal(preview, true, 'Cross-attempt reuse is preview-only');
    assert.deepEqual(Object.keys(previewAttempts).sort(), [...releaseArchitectures].sort());
    assert.match(runAttempt, /^[1-9][0-9]*$/);
    for (const attempt of Object.values(previewAttempts)) {
      assert.match(attempt, /^[1-9][0-9]*$/);
      assert(BigInt(attempt) <= BigInt(runAttempt), 'Future preview attempt');
    }
  }
  mkdirSync(output, { recursive: true });
  const builds = {};
  const reports = {};
  const files = [];
  const entries = [];
  const metadata = [];
  for (const arch of releaseArchitectures) {
    const directory = join(input, arch);
    const build = json(join(directory, 'build.json'));
    for (const key of ['tag', 'version', 'sha', 'tree', 'tagObject'])
      assert.equal(build[key], source[key], `Mismatched ${arch} ${key}`);
    assert.equal(build.arch, arch);
    assert.equal(build.platform, 'darwin');
    assert.equal(build.preview, preview);
    assert.equal(build.signing, preview ? 'disposable-self-signed' : 'fixed-self-signed');
    assert.equal(build.identity, normalizeFingerprint(preview ? build.identity : identity));
    assert.equal(build.runId, runId);
    assert.equal(build.runAttempt, previewAttempts?.[arch] ?? runAttempt);
    assert.equal(build.nativeStartup, true);
    assert.equal(build.signingCleanup, true);
    if (preview) assert.equal(build.nativeUpdate, true);
    const report = json(join(directory, 'signing-report.json'));
    assert.equal(report.identity, build.identity);
    const dmg = `VigilShell-${source.version}-macos-${arch}.dmg`;
    validatePreview(
      readFileSync(join(directory, dmg)),
      readFileSync(join(directory, `${dmg}.sha256`), 'utf8'),
      report,
      source,
      arch,
    );
    const update = inspectUpdateArtifacts(directory, source.version, arch);
    const zip = basename(update.archive);
    assert.equal(
      readFileSync(`${update.archive}.sha256`, 'utf8').trim(),
      `${update.sha256}  ${zip}`,
    );
    const expected = [
      dmg,
      `${dmg}.sha256`,
      zip,
      `${zip}.sha256`,
      `${zip}.blockmap`,
      'latest-mac.yml',
      'signing-report.json',
    ];
    assert.deepEqual(Object.keys(build.files).sort(), [...expected].sort());
    for (const file of expected) {
      assert.equal(digest(join(directory, file)), build.files[file], `Changed ${arch} ${file}`);
      if (['latest-mac.yml', 'signing-report.json'].includes(file)) continue;
      copyFileSync(join(directory, file), join(output, file));
      files.push(join(output, file));
    }
    builds[arch] = build;
    reports[arch] = report;
    entries.push(update.file);
    metadata.push(update.metadata);
  }
  assert.equal(
    metadata[0].minimumSystemVersion,
    metadata[1].minimumSystemVersion,
    'Architecture OS requirements differ',
  );
  const merged = {
    version: source.version,
    files: entries,
    path: entries[0].url,
    sha512: entries[0].sha512,
    ...(metadata[0].releaseDate ? { releaseDate: metadata[0].releaseDate } : {}),
    ...(metadata[0].minimumSystemVersion
      ? { minimumSystemVersion: metadata[0].minimumSystemVersion }
      : {}),
  };
  const metadataPath = join(output, 'latest-mac.yml');
  writeJson(metadataPath, merged);
  for (const arch of releaseArchitectures)
    inspectUpdateArtifacts(output, source.version, arch, releaseArchitectures);
  const reportPath = join(output, 'signing-report.json');
  writeJson(reportPath, { ...source, architectures: reports });
  files.push(metadataPath, reportPath);
  return { files, builds, metadataSha256: digest(metadataPath), reportPath };
}
