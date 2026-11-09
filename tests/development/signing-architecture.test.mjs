import test from 'node:test';
import assert from 'node:assert/strict';
import { packageArchitecture } from '../../scripts/package-policy.mjs';
test('both native release architectures map to matching Rust and Electron outputs', () => {
  assert.deepEqual(packageArchitecture('arm64', 'arm64', 'darwin'), {
    arch: 'arm64',
    directory: 'mac-arm64',
    rust: 'aarch64-apple-darwin',
  });
  assert.deepEqual(packageArchitecture('x64', 'x64', 'darwin'), {
    arch: 'x64',
    directory: 'mac',
    rust: 'x86_64-apple-darwin',
  });
  for (const [arch, host, platform] of [
    ['x64', 'arm64', 'darwin'],
    ['arm64', 'x64', 'darwin'],
    ['universal', 'arm64', 'darwin'],
    ['arm64', 'arm64', 'linux'],
  ])
    assert.throws(() => packageArchitecture(arch, host, platform), /matching native/);
});
