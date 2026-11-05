import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { checkRustToolchain, root } from './check-rust-toolchain.mjs';
checkRustToolchain();
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--no-deps', '--locked', '--format-version', '1'], {
    cwd: root,
    encoding: 'utf8',
  }),
);
for (const pkg of metadata.packages) {
  for (const dependency of pkg.dependencies) {
    if (!dependency.path) continue;
    const path = relative(root, dependency.path);
    if (path === '..' || path.startsWith('../') || path.startsWith('..\\') || isAbsolute(path)) {
      throw new Error(`Local dependency outside the repository: ${dependency.name}`);
    }
  }
}
const version = '0.20.2';
const toolRoot = resolve(root, '.cache/rust-tools', `cargo-deny-${version}`);
let executable = resolve(
  toolRoot,
  'bin',
  process.platform === 'win32' ? 'cargo-deny.exe' : 'cargo-deny',
);
/** @param {string} file */
function matches(file) {
  try {
    return (
      execFileSync(file, ['--version'], {
        encoding: 'utf8',
        stdio: 'pipe',
      }).trim() === `cargo-deny ${version}`
    );
  } catch {
    return false;
  }
}
if (matches('cargo-deny')) executable = 'cargo-deny';
else if (!existsSync(executable) || !matches(executable)) {
  execFileSync(
    'cargo',
    ['install', 'cargo-deny', '--version', version, '--locked', '--root', toolRoot],
    { cwd: root, stdio: 'inherit' },
  );
  if (!matches(executable)) throw new Error('cargo-deny version verification failed');
}
// 公告更新失败与检查不通过都保留非零退出码；不伪装成离线检查通过。
execFileSync(executable, ['--locked', 'check'], { cwd: root, stdio: 'inherit' });
