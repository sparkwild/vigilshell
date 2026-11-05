import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function checkRustToolchain() {
  const source = readFileSync(resolve(root, 'rust-toolchain.toml'), 'utf8');
  const version = source.match(/^channel\s*=\s*"([\d.]+)"/m)?.[1];
  if (!version) throw new Error('rust-toolchain.toml must name an exact Rust version');
  for (const tool of ['rustc', 'cargo']) {
    const actual = execFileSync(tool, ['--version'], {
      cwd: root,
      encoding: 'utf8',
    }).trim();
    if (actual.split(' ')[1] !== version)
      throw new Error(
        `${tool} must be ${version}; found ${actual}. Select the project toolchain without changing the global default.`,
      );
  }
  return version;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(`Rust toolchain ${checkRustToolchain()} verified`);
}
