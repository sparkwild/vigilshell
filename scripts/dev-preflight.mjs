import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { loadSigningConfig, preflight, projectRoot } from './macos-signing.mjs';

// 在任何共享页面或外壳产物被覆盖前检查；Rust watcher 走独立的代次构建。
const owner = Number(
  await readFile(join(projectRoot, '.cache/vigilshell-dev/electron.lock/owner'), 'utf8').catch(
    () => '0',
  ),
);
if (owner > 0) {
  try {
    process.kill(owner, 0);
    throw new Error('Quit the running VigilShell Dev before rebuilding the application');
  } catch (error) {
    if (error.code !== 'ESRCH') throw error;
  }
}
if (process.platform === 'darwin') {
  const processes = execFileSync('/bin/ps', ['-axo', 'comm='], { encoding: 'utf8' });
  if (
    processes
      .split('\n')
      .some((path) =>
        /\/VigilShell Dev\.app\/Contents\/MacOS\/(Electron|VigilShell Dev)$/.test(path.trim()),
      )
  )
    throw Error('Quit the running VigilShell Dev before rebuilding or switching variants');
  if (!process.argv.includes('--process-only')) preflight(loadSigningConfig('dev'));
}
