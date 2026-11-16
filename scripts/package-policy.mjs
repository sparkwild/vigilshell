import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

/** Each Electron, Rust and native helper must be built on the requested native host.
 * @param {string} arch @param {string} host @param {string} platform
 */
export function packageArchitecture(arch, host = process.arch, platform = process.platform) {
  if (platform !== 'darwin' || !['arm64', 'x64'].includes(arch) || arch !== host)
    throw Error('Build each DMG on its matching native macOS architecture (arm64 or x64)');
  return {
    arch,
    directory: arch === 'arm64' ? 'mac-arm64' : 'mac',
    rust: arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin',
  };
}

export function validatePreview(dmg, checksum, report, source, arch = 'arm64') {
  assert(['arm64', 'x64'].includes(arch), 'Unsupported package architecture');
  assert.equal(report.version, source.version);
  assert.equal(report.sourceSha, source.sha);
  assert.equal(report.arch, arch);
  assert.equal(report.profile, 'release');
  assert(/^[A-F0-9]{40}$/.test(report.identity));
  const name = `VigilShell-${source.version}-macos-${arch}.dmg`;
  const digest = createHash('sha256').update(dmg).digest('hex');
  assert.equal(checksum.trim(), `${digest}  ${name}`);
  return digest;
}
