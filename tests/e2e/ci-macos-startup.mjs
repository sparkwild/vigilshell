import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { _electron as electron } from 'playwright';
import { closeApplication, waitUntil, withCleanup } from './ci-macos-lifecycle.mjs';
import { releaseStartupContext } from './ci-macos-context.mjs';

const { data, developmentData, bundle, signingReport, evidence } = releaseStartupContext({
  environment: process.env,
  platform: process.platform,
  arch: process.arch,
  home: homedir(),
  workspace: process.cwd(),
  exists: existsSync,
});
const report = JSON.parse(readFileSync(signingReport, 'utf8'));
assert.equal(report.profile, 'release');
assert.equal(report.arch, process.arch);
assert.equal(report.identity, process.env.VIGILSHELL_RELEASE_SIGNING_IDENTITY);
await mkdir(evidence, { recursive: true });
const executable = execFileSync(
  'plutil',
  ['-extract', 'CFBundleExecutable', 'raw', '-o', '-', join(bundle, 'Contents/Info.plist')],
  { encoding: 'utf8' },
).trim();
let application;
let applicationProcess;
let backendPid;
let passed = false;
const errors = [];
const logs = [];
const failures = [];
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
for (const name of [
  'ELECTRON_RUN_AS_NODE',
  'ELECTRON_RENDERER_URL',
  'VIGILSHELL_DATA_DIR',
  'VIGILSHELL_DEV_APP',
  'VIGILSHELL_DEV_PACKAGE_BUILD',
  'VIGILSHELL_DEVELOPMENT_BUILD',
])
  delete environment[name];
try {
  await withCleanup(
    async () => {
      application = await electron.launch({
        executablePath: join(bundle, 'Contents/MacOS', executable),
        timeout: 60000,
        env: environment,
      });
      applicationProcess = application.process();
      applicationProcess.stdout?.on('data', (value) => logs.push(String(value)));
      applicationProcess.stderr?.on('data', (value) => logs.push(String(value)));
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
      await waitUntil(
        () =>
          application.evaluate(({ BrowserWindow }) =>
            BrowserWindow.getAllWindows().some((window) => window.isVisible()),
          ),
        'visible native window',
        { timeoutMs: 15000 },
      );
      const identity = await application.evaluate(({ app, BrowserWindow }) => ({
        name: app.name,
        packaged: app.isPackaged,
        data: app.getPath('userData'),
        visible: BrowserWindow.getAllWindows().some((window) => window.isVisible()),
      }));
      assert.deepEqual(identity, { name: 'VigilShell', packaged: true, data, visible: true });
      const snapshot = await page.evaluate(() => window.vigilshell.getSnapshot());
      assert(snapshot.preferences, 'Preferences IPC must respond');
      assert.equal(await page.evaluate(() => typeof window.require), 'undefined');
      const children = execFileSync('ps', ['-axo', 'pid=,ppid=,command='], { encoding: 'utf8' })
        .split('\n')
        .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/))
        .filter(
          (match) =>
            match &&
            Number(match[2]) === applicationProcess.pid &&
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
      await closeApplication(application, applicationProcess);
      await waitUntil(() => !alive(backendPid), 'backend process exit');
      assert(!existsSync(developmentData), 'Release application must not create Dev data');
    },
    async () => {
      if (applicationProcess?.exitCode === null && applicationProcess.signalCode === null)
        await closeApplication(application, applicationProcess);
      if (backendPid) await waitUntil(() => !alive(backendPid), 'backend cleanup');
      await rm(data, { recursive: true, force: true });
    },
  );
  passed = true;
} catch (error) {
  failures.push(
    ...(error instanceof AggregateError ? error.errors : [error]).map(
      (item) => item.stack ?? String(item),
    ),
  );
  throw error;
} finally {
  await writeFile(join(evidence, 'application.log'), logs.join(''));
  await writeFile(
    join(evidence, 'result.json'),
    JSON.stringify({ passed, errors, failures }, null, 2),
  );
}
console.log('Native release startup, Renderer IPC, backend identity and normal shutdown verified');
