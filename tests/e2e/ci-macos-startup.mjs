import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { _electron as electron } from 'playwright';

// This owns fresh Dev data only on disposable hosted runners, never a developer's Mac.
assert(
  process.env.GITHUB_ACTIONS === 'true' &&
    process.env.RUNNER_ENVIRONMENT === 'github-hosted' &&
    process.platform === 'darwin',
  'Startup smoke requires a disposable GitHub-hosted macOS runner',
);
assert(process.env.RUNNER_TEMP);
assert.equal(process.arch, process.env.EXPECTED_ARCH);
const data = join(homedir(), 'Library/Application Support/VigilShell-dev');
const productionData = join(homedir(), 'Library/Application Support/VigilShell');
assert(!existsSync(data), 'Refusing to use existing Dev data');
assert(!existsSync(productionData), 'Refusing to run alongside production data');
const evidence = join(process.env.RUNNER_TEMP, 'vigilshell-ci-evidence');
await mkdir(evidence, { recursive: true });
const bundle = resolve(
  'release/dev',
  process.arch === 'arm64' ? 'mac-arm64' : 'mac',
  'VigilShell Dev.app',
);
const executable = execFileSync(
  'plutil',
  ['-extract', 'CFBundleExecutable', 'raw', '-o', '-', join(bundle, 'Contents/Info.plist')],
  { encoding: 'utf8' },
).trim();
let application;
let backendPid;
let passed = false;
const errors = [];
const logs = [];
function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error.code === 'ESRCH') return false;
    throw error;
  }
}
const environment = { ...process.env };
delete environment.ELECTRON_RUN_AS_NODE;
try {
  application = await electron.launch({
    executablePath: join(bundle, 'Contents/MacOS', executable),
    timeout: 60000,
    env: environment,
  });
  application.process().stdout?.on('data', (value) => logs.push(String(value)));
  application.process().stderr?.on('data', (value) => logs.push(String(value)));
  application
    .context()
    .on('page', (page) => page.on('pageerror', (error) => errors.push(error.message)));
  const page = await application.firstWindow();
  page.on('pageerror', (error) => errors.push(error.message));
  await page.getByTestId('workbench').waitFor({ timeout: 60000 });
  await page.waitForFunction(
    async () => (await window.vigilshell.getSnapshot()).runtime.status === 'ready',
    null,
    { timeout: 60000 },
  );
  const identity = await application.evaluate(({ app, BrowserWindow }) => ({
    name: app.name,
    packaged: app.isPackaged,
    data: app.getPath('userData'),
    visible: BrowserWindow.getAllWindows().some((window) => window.isVisible()),
  }));
  assert.deepEqual(identity, { name: 'VigilShell Dev', packaged: true, data, visible: true });
  const snapshot = await page.evaluate(() => window.vigilshell.getSnapshot());
  assert(snapshot.preferences, 'Preferences IPC must respond');
  assert.equal(await page.evaluate(() => typeof window.require), 'undefined');
  const children = execFileSync('ps', ['-axo', 'pid=,ppid=,command='], { encoding: 'utf8' })
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/))
    .filter(
      (match) =>
        match &&
        Number(match[2]) === application.process().pid &&
        match[3].includes('vigilshell-backend --data-dir'),
    );
  assert.equal(children.length, 1, 'Expected one real Rust backend');
  assert(children[0][3].includes(data));
  backendPid = Number(children[0][1]);
  await page.screenshot({ path: join(evidence, 'workbench.png') });
  await writeFile(
    join(evidence, 'identity.json'),
    JSON.stringify({ ...identity, arch: process.arch, runtime: snapshot.runtime }, null, 2),
  );
  assert.deepEqual(errors, []);
  await application.close();
  assert.equal(application.process().exitCode, 0, 'Application must quit normally');
  for (let i = 0; i < 100 && alive(backendPid); i++) await delay(100);
  assert(!alive(backendPid), 'Backend must exit with the application');
  assert(!existsSync(productionData), 'Production data must remain untouched');
  passed = true;
} finally {
  try {
    if (application?.process().exitCode === null) await application.close();
  } finally {
    await writeFile(join(evidence, 'application.log'), logs.join(''));
    await writeFile(join(evidence, 'result.json'), JSON.stringify({ passed, errors }, null, 2));
    await rm(data, { recursive: true, force: true });
  }
}
console.log('Native Dev startup, Renderer IPC, backend identity and normal shutdown verified');
