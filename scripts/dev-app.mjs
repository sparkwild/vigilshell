import { createProcessGroupStopper } from './process-group.mjs';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepareDevApp } from './prepare-dev-app.mjs';
import { stageBackend, publishCandidate } from './dev-backend.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(join(root, 'package.json'));
const developmentApp = await prepareDevApp();
await stageBackend();
await publishCandidate();
const child = spawn(
  process.execPath,
  [
    join(dirname(require.resolve('electron-vite/package.json')), 'bin/electron-vite.js'),
    'dev',
    '--watch',
    ...process.argv.slice(2),
  ],
  {
    cwd: join(root, '.'),
    detached: true,
    stdio: 'inherit',
    env: {
      ...process.env,
      VIGILSHELL_DEV_APP: '1',
      VIGILSHELL_DEV_EXECUTABLE: developmentApp.executable,
      ELECTRON_EXEC_PATH: join(root, 'scripts/dev-app-electron.mjs'),
    },
  },
);
const stopGroup = createProcessGroupStopper(child.pid);
let cleanupErrorReported = false;
function stop() {
  void stopGroup().catch((error) => {
    if (!cleanupErrorReported) console.error(error);
    cleanupErrorReported = true;
    process.exitCode = 1;
  });
}
child.on('error', (error) => {
  console.error(error);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 0;
  stop();
});
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, stop);
// beforeExit allows the bounded cleanup check to finish; exit cannot await it.
process.once('beforeExit', stop);
