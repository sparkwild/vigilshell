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
