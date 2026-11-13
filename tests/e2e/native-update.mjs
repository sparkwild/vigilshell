import { verifyDiscoveryUi } from './update-discovery-ui.mjs';
import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createReadStream, existsSync, readFileSync } from 'node:fs';
import {
  chmod,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  rename,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { createServer } from 'node:http';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { _electron as electron } from 'playwright';
import { loadSigningConfig, preflight, verifyBundle } from '../../scripts/macos-signing.mjs';
import { packageBuildEnvironment } from '../../scripts/build-context.mjs';
import { closeApplication, waitUntil } from './ci-macos-lifecycle.mjs';

// Native mode probes Squirrel directly; --application exercises product IPC and shutdown.
assert.equal(process.platform, 'darwin');
assert.equal(process.arch, 'arm64');
const applicationFlow = process.argv.includes('--application');
const config = loadSigningConfig('dev', {});
preflight(config);
execFileSync(process.execPath, ['scripts/dev-preflight.mjs', '--process-only']);
async function clearTestPending() {
  const cache = join(homedir(), 'Library/Application Support/VigilShell-dev/updates');
  const record = join(cache, 'pending.json');
  if (existsSync(record)) {
    const pending = JSON.parse(readFileSync(record, 'utf8'));
    assert.equal(
      pending.release?.notes,
      'VigilShell native update acceptance.',
      'Refusing to replace a non-test pending update',
    );
    for (const name of await readdir(cache))
      if (name === 'pending.json' || /^archive(?:-[a-f0-9]{128})?\.zip(?:\.tmp)?$/.test(name))
        await unlink(join(cache, name));
  }
}
if (applicationFlow) await clearTestPending();
const sourceVersion = JSON.parse(readFileSync('package.json')).version;
assert(/^\d+\.\d+\.\d+$/.test(sourceVersion));
const parts = sourceVersion.split('.').map(Number);
const targetVersion = `${parts[0]}.${parts[1]}.${parts[2] + 1}`;
const appName = 'VigilShell Dev.app';
const parent = resolve('.cache/native-update');
await mkdir(parent, { recursive: true });
const evidence = await mkdtemp(join(parent, 'run-'));
const output = join(evidence, 'target');
const buildConfig = join(evidence, 'builder.json');
await writeFile(
  buildConfig,
  JSON.stringify({
    extends: resolve('electron-builder.yml'),
    appId: 'com.sparkwild.vigilshell.electron.dev',
    productName: 'VigilShell Dev',
    extraMetadata: { version: targetVersion },
    directories: { output },
    mac: {
      identity: config.identity,
      extraDistFiles: ['resources/licenses/electron-LICENSE.txt'],
    },
  }),
);
execFileSync(
  resolve('node_modules/.bin/electron-builder'),
  ['--config', buildConfig, '--mac', 'zip', '--arm64', '--publish', 'never'],
  {
    stdio: 'inherit',
    env: {
      ...packageBuildEnvironment('dev'),
      CSC_IDENTITY_AUTO_DISCOVERY: 'false',
      CSC_KEYCHAIN: config.keychain,
    },
  },
);
const base = resolve('release/dev/mac-arm64', appName);
verifyBundle(base, config, process.arch);
verifyBundle(join(output, 'mac-arm64', appName), config, process.arch);
const zip = join(
  output,
  (await readdir(output)).find((name) => name.endsWith('.zip')),
);
const bytes = readFileSync(zip);
const archiveName = applicationFlow ? basename(zip) : 'update.zip';
const info = {
  version: targetVersion,
  files: [
    {
      url: archiveName,
      sha512: createHash('sha512').update(bytes).digest('base64'),
      size: bytes.length,
    },
  ],
  releaseDate: new Date().toISOString(),
};
let servedArchive = zip;
let servedInfo = info;
let invalidArchive;
if (applicationFlow) {
  const invalidApp = join(evidence, 'invalid', appName);
  await mkdir(join(evidence, 'invalid'));
  await cp(join(output, 'mac-arm64', appName), invalidApp, {
    recursive: true,
    verbatimSymlinks: true,
    preserveTimestamps: true,
  });
  execFileSync('/usr/bin/codesign', ['--remove-signature', invalidApp]);
  assert.throws(() =>
    execFileSync('/usr/bin/codesign', ['--verify', '--strict', invalidApp], { stdio: 'pipe' }),
  );
  invalidArchive = join(evidence, 'invalid-update.zip');
  execFileSync('/usr/bin/ditto', ['-c', '-k', '--keepParent', invalidApp, invalidArchive]);
}
const requests = [];
let offline = false;
let offlineRequestCount = 0;
const prefix = `/${randomUUID()}`;
const server = createServer((request, response) => {
  const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
  requests.push(pathname);
  if (offline) {
    response.writeHead(503);
    response.end();
    return;
  }
  if (pathname === `${prefix}/releases`) {
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(
      JSON.stringify([
        {
          tag_name: `v${targetVersion}`,
          draft: false,
          prerelease: false,
          body: 'VigilShell native update acceptance.',
          assets: [
            { name: `VigilShell-${targetVersion}-macos-arm64.dmg`, state: 'uploaded' },
            { name: 'latest-mac.yml', state: 'uploaded' },
            { name: archiveName, state: 'uploaded' },
          ],
        },
      ]),
    );
  } else if (
    pathname === `${prefix}/latest-mac.yml` ||
    pathname === `${prefix}/v${targetVersion}/latest-mac.yml`
  ) {
    response.writeHead(200, { 'Content-Type': 'application/yaml' });
    response.end(JSON.stringify(servedInfo));
  } else if (
    pathname === `${prefix}/${archiveName}` ||
    pathname === `${prefix}/v${targetVersion}/${archiveName}`
  ) {
    response.writeHead(200, {
      'Content-Type': 'application/zip',
      'Content-Length': servedInfo.files[0].size,
    });
    const stream = createReadStream(servedArchive);
    stream.on('error', () => response.destroy());
    response.on('close', () => stream.destroy());
    stream.pipe(response);
  } else {
    response.writeHead(404);
    response.end();
  }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const feed = `http://127.0.0.1:${server.address().port}${prefix}`;
const updaterConfig = join(evidence, 'dev-update.yml');
await writeFile(
  updaterConfig,
  JSON.stringify({
    provider: 'generic',
    url: feed,
    updaterCacheDirName: 'vigilshell-dev-native-update',
  }),
);
const live = resolve('node_modules/.cache/vigilshell-dev', appName);
const original = join(evidence, 'original', appName);
await mkdir(join(evidence, 'original'));
assert(existsSync(live), 'Existing VigilShell Dev shell is required');
await rename(live, original);
await cp(base, live, { recursive: true, verbatimSymlinks: true, preserveTimestamps: true });
const discoveryPath = join(
  homedir(),
  'Library/Application Support/VigilShell-dev/updates/discovery.json',
);
const discoveryBackup = existsSync(discoveryPath) ? readFileSync(discoveryPath, 'utf8') : null;
await writeFile(
  join(evidence, 'restore.json'),
  JSON.stringify({ live, original, discoveryBackup }),
);
const executable = join(live, 'Contents/MacOS/VigilShell Dev');
const environment = { ...process.env };
for (const key of [
  'ELECTRON_RUN_AS_NODE',
  'VIGILSHELL_DEV_APP',
  'VIGILSHELL_DATA_DIR',
  'ELECTRON_RENDERER_URL',
  'ELECTRON_EXEC_PATH',
])
  delete environment[key];
if (applicationFlow) environment.VIGILSHELL_DEV_UPDATE_FEED = feed;
let application;
let child;
let upgraded = false;
const events = [];
const result = {
  passed: false,
  applicationFlow,
  archiveSha256: createHash('sha256').update(bytes).digest('hex'),
  sourceSha: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  sourceDirty: execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim() !== '',
  sourceVersion,
  targetVersion,
  identity: config.identity,
  evidence,
  ordinaryQuitPreservedVersion: false,
  needsDevShellRestoration: true,
};
const plistVersion = () =>
  execFileSync(
    'plutil',
    ['-extract', 'CFBundleShortVersionString', 'raw', '-o', '-', join(live, 'Contents/Info.plist')],
    { encoding: 'utf8' },
  ).trim();
const processes = () =>
  execFileSync('ps', ['-axo', 'pid=,ppid=,comm='], { encoding: 'utf8' })
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/))
    .filter(Boolean);
try {
  if (applicationFlow) {
    const invalidBytes = readFileSync(invalidArchive);
    servedArchive = invalidArchive;
    servedInfo = {
      ...info,
      files: [
        {
          ...info.files[0],
          size: invalidBytes.length,
          sha512: createHash('sha512').update(invalidBytes).digest('base64'),
        },
      ],
    };
    application = await electron.launch({
      executablePath: executable,
      env: environment,
      timeout: 60000,
    });
    child = application.process();
    const page = await application.firstWindow();
    await page.getByTestId('workbench').waitFor({ timeout: 60000 });
    await page.waitForFunction(
      async () => (await window.vigilshell.getSnapshot()).runtime.status === 'ready',
      null,
      { timeout: 60000 },
    );
    await application.evaluate(({ autoUpdater }) => {
      global.__nativeSignatureError = '';
      autoUpdater.on('error', (error) => {
        global.__nativeSignatureError = error.message;
      });
    });
    await verifyDiscoveryUi(application, page, evidence, result);
    assert.equal(
      (await page.evaluate(() => window.vigilshell.application.download())).phase,
      'ready',
    );
    assert.equal(
      (await page.evaluate(() => window.vigilshell.application.install())).phase,
      'installError',
    );
    const signatureError = await application.evaluate(() => global.__nativeSignatureError);
    assert.match(signatureError, /sign|签名/i);
    result.invalidSignatureError = signatureError;
    assert.equal(
      (await page.evaluate(() => window.vigilshell.getSnapshot())).runtime.status,
      'ready',
    );
    assert.equal(plistVersion(), sourceVersion);
    result.invalidSignatureRejected = true;
    await closeApplication(application, child);
    application = undefined;
    await clearTestPending();
    servedArchive = zip;
    servedInfo = info;
    console.log('Native signature rejection preserved the running Dev backend and original bundle');
  }
  for (const installing of [false, true]) {
    application = await electron.launch({
      executablePath: executable,
      env: environment,
      timeout: 60000,
    });
    child = application.process();
    const page = await application.firstWindow();
    await page.getByTestId('workbench').waitFor({ timeout: 60000 });
    await page.waitForFunction(
      async () => (await window.vigilshell.getSnapshot()).runtime.status === 'ready',
      null,
      { timeout: 60000 },
    );
    const identity = await application.evaluate(({ app }) => ({
      name: app.name,
      version: app.getVersion(),
      data: app.getPath('userData'),
      packaged: app.isPackaged,
    }));
    assert.deepEqual(identity, {
      name: 'VigilShell Dev',
      version: sourceVersion,
      data: join(homedir(), 'Library/Application Support/VigilShell-dev'),
      packaged: true,
    });
    console.log(
      'Dev application ready; starting',
      installing ? 'native install' : 'ordinary quit',
      'probe',
    );
    if (applicationFlow) {
      if (!installing) {
        const found = await page.evaluate(() => window.vigilshell.application.check());
        assert.equal(found.release.version, targetVersion);
        const downloaded = await page.evaluate(() => window.vigilshell.application.download());
        assert.equal(downloaded.phase, 'ready');
        const directory = dirname(live);
        const mode = (await lstat(directory)).mode & 0o777;
        try {
          await chmod(directory, mode & ~0o222);
          const refused = await page.evaluate(() => window.vigilshell.application.install());
          assert.equal(refused.phase, 'installError');
          assert.equal(refused.issue, 'readOnlyLocation');
          assert.equal(
            (await page.evaluate(() => window.vigilshell.getSnapshot())).runtime.status,
            'ready',
          );
          result.readOnlyInstallPreservedRuntime = true;
        } finally {
          await chmod(directory, mode);
        }
      } else {
        await page.waitForFunction(
          async () => (await window.vigilshell.application.state()).phase === 'ready',
          null,
          { timeout: 30000 },
        );
        result.restoredReadyOffline = true;
        assert.equal(requests.length, offlineRequestCount);
      }
    } else
      await application.evaluate(
        async ({ app, autoUpdater }, options) => {
          const load = process
            .getBuiltinModule('module')
            .createRequire(`${app.getAppPath()}/package.json`);
          const { MacUpdater } = load('electron-updater');
          const updater = new MacUpdater();
          updater.updateConfigPath = options.config;
          updater.autoDownload = false;
          updater.autoInstallOnAppQuit = false;
          updater.disableDifferentialDownload = true;
          global.__nativeUpdateProbe = { updater, events: [], nativeReady: false };
          updater.on('error', (error) =>
            global.__nativeUpdateProbe.events.push({ event: 'error', message: error.message }),
          );
          autoUpdater.on('update-downloaded', () => {
            global.__nativeUpdateProbe.nativeReady = true;
          });
          autoUpdater.on('error', (error) =>
            global.__nativeUpdateProbe.events.push({
              event: 'native-error',
              message: error.message,
            }),
          );
          const found = await updater.checkForUpdates();
          if (found.updateInfo.version !== options.target) throw Error('Unexpected target');
          await updater.downloadUpdate();
          global.__nativeUpdateProbe.events.push({ event: 'downloaded' });
        },
        {
          config: updaterConfig,
          target: targetVersion,
        },
      );
    if (!installing) {
      await closeApplication(application, child);
      application = undefined;
      assert.equal(plistVersion(), sourceVersion);
      result.ordinaryQuitPreservedVersion = true;
      if (applicationFlow) {
        offlineRequestCount = requests.length;
        offline = true;
      }
      continue;
    }
    if (applicationFlow) {
      await page
        .evaluate(() => window.vigilshell.application.install())
        .catch((error) => {
          if (!/closed|destroyed|Target/.test(error.message)) throw error;
        });
    } else {
      await application.evaluate(({ autoUpdater }) => autoUpdater.checkForUpdates());
      await waitUntil(
        async () => {
          const state = await application.evaluate(() => ({
            ready: global.__nativeUpdateProbe.nativeReady,
            events: global.__nativeUpdateProbe.events,
          }));
          events.splice(0, events.length, ...state.events);
          assert(!events.some((event) => event.event.includes('error')), JSON.stringify(events));
          return state.ready;
        },
        'Squirrel signature validation and preparation',
        { timeoutMs: 90000 },
      );
      console.log('Squirrel completed native signature validation and preparation');
      const backends = processes().filter(
        (match) => Number(match[2]) === child.pid && match[3].endsWith('/vigilshell-backend'),
      );
      for (const match of backends) process.kill(Number(match[1]), 'SIGTERM');
      await waitUntil(
        () => !processes().some((match) => backends.some((old) => old[1] === match[1])),
        'idle probe backend shutdown',
      );
      await application
        .evaluate(({ app, autoUpdater, BrowserWindow }) => {
          autoUpdater.once('before-quit-for-update', () => {
            app.removeAllListeners('before-quit');
            for (const window of BrowserWindow.getAllWindows()) window.removeAllListeners('close');
          });
          global.__nativeUpdateProbe.updater.quitAndInstall();
        })
        .catch((error) => {
          if (!/closed|destroyed|Target/.test(error.message)) throw error;
        });
    }
    await waitUntil(
      () => child.exitCode !== null || child.signalCode !== null,
      'old Dev process exit',
      { timeoutMs: 30000 },
    );
    application = undefined;
    console.log('Old Dev process exited; awaiting installed version and automatic relaunch');
    await waitUntil(() => plistVersion() === targetVersion, 'installed target version', {
      timeoutMs: 60000,
    });
    await waitUntil(
      () => processes().some((match) => match[3] === executable),
      'automatic native relaunch',
      { timeoutMs: 60000 },
    );
    verifyBundle(live, config, process.arch);
    if (applicationFlow) assert.equal(requests.length, offlineRequestCount);
    upgraded = true;
    result.passed = true;
    result.nativeSignaturePreparation = true;
    result.automaticRelaunch = true;
    result.installedVersion = plistVersion();
    result.needsDevShellRestoration = true;
    console.log(
      'Native update and automatic relaunch verified; quit the upgraded Dev before restoring its original shell.',
    );
  }
} catch (error) {
  result.error = error.stack;
  throw error;
} finally {
  if (application && child.exitCode === null && child.signalCode === null)
    await closeApplication(application, child).catch((error) => {
      result.cleanupError = error.stack;
    });
  server.close();
  if (!upgraded && !processes().some((match) => match[3] === executable)) {
    const { rm } = await import('node:fs/promises');
    await rm(live, { recursive: true });
    await rename(original, live);
    result.needsDevShellRestoration = false;
    result.devShellRestored = true;
    if (discoveryBackup !== null) await writeFile(discoveryPath, discoveryBackup, { mode: 0o600 });
    else if (existsSync(discoveryPath)) await unlink(discoveryPath);
  }
  await writeFile(
    join(evidence, 'result.json'),
    JSON.stringify({ ...result, events, requests }, null, 2),
  );
  console.log('Native update evidence:', join(evidence, 'result.json'));
}
