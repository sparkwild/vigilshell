import { execFileSync } from 'node:child_process';
import { checkRustToolchain, root } from './check-rust-toolchain.mjs';
checkRustToolchain();
for (const args of [
  ['fmt', '--all', '--', '--check'],
  ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
  ['test', '--workspace', '--locked'],
])
  execFileSync('cargo', args, { cwd: root, stdio: 'inherit' });
