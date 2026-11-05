import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { homedir } from 'node:os';
import { resolve, join } from 'node:path';
import { promisify } from 'node:util';
import { chromium } from 'playwright';
import { createProcessGroupStopper } from '../../scripts/process-group.mjs';

export const execute = promisify(execFile);
export const root = resolve('.');
export const dataRoot = join(homedir(), 'Library/Application Support/VigilShell-dev');
export const bundle = join(root, 'node_modules/.cache/vigilshell-dev/VigilShell Dev.app');

export async function until(probe, label, timeout = 30000) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    try {
      if (await probe()) return;
    } catch (error) {
      last = error;
    }
    await new Promise((done) => setTimeout(done, 50));
  }
  throw new Error(label, { cause: last });
}

export function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error.code === 'ESRCH') return false;
    throw error;
  }
}

export async function businessFiles() {
  const files = {};
  async function walk(directory, relative = '') {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const name = join(relative, entry.name);
      if (name === 'browser') continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await walk(path, name);
      else if (entry.isFile()) {
        const info = await stat(path);
        files[name] = {
          bytes: info.size,
          mode: info.mode & 0o777,
          hash: createHash('sha256')
            .update(await readFile(path))
            .digest('hex'),
        };
      }
    }
  }
  await walk(dataRoot);
  return files;
}

export async function createEvidenceDirectory(base, kind) {
  const directory = resolve(base);
  await mkdir(directory, { recursive: true });
  return mkdtemp(join(directory, `${kind}-`));
}

/** 初始化失败时，通知本次启动器的进程组退出；正常 Dev 监管器负责其独立子组。 */
export async function stopLauncher(launcher) {
  if (!launcher?.pid) return;
  const { stdout } = await execute('ps', ['-axo', 'pid=,ppid=']);
  const processes = stdout.split('\n').map((line) => line.trim().split(/\s+/).map(Number));
  const owned = new Set([launcher.pid]);
  for (let previous = -1; previous !== owned.size;) {
    previous = owned.size;
    for (const [pid, parent] of processes) if (owned.has(parent)) owned.add(pid);
  }
  await createProcessGroupStopper(launcher.pid)();
  await until(
    () =>
      (launcher.exitCode !== null || launcher.signalCode !== null) &&
      [...owned].every((pid) => !alive(pid)),
    'Dev launcher or its owned children did not exit',
  );
}

export async function devSession({
  lifecycle = false,
  cdpUrl = process.env.VIGILSHELL_DEV_CDP_URL,
  startupTimeoutMs = 120000,
} = {}) {
  assert.equal(process.platform, 'darwin');
  assert(!process.env.VIGILSHELL_DATA_DIR, 'Dev acceptance uses the existing Dev data root');
  const before = await businessFiles();
  const attached = cdpUrl;
  assert(!lifecycle || !attached, 'Lifecycle acceptance must own its normal Dev launcher');
  const evidence = await createEvidenceDirectory(
    process.env.VIGILSHELL_EVIDENCE_DIR || '.cache/dev-acceptance',
    attached ? 'attached' : lifecycle ? 'lifecycle' : 'smoke',
  );
  let endpoint = attached,
    launcher,
    browser,
    page,
    output = '';
  if (attached) {
    const url = new URL(attached);
    assert(
      url.protocol === 'http:' &&
        ['127.0.0.1', 'localhost'].includes(url.hostname) &&
        !url.username &&
        !url.password,
    );
  }
  async function connect() {
    await until(
      async () => (await fetch(`${endpoint}/json/version`)).ok,
      'Dev CDP did not become ready',
      startupTimeoutMs,
    );
    browser = await chromium.connectOverCDP(endpoint);
    await until(() => browser.contexts()[0]?.pages().length, 'Dev window missing');
    page = browser.contexts()[0].pages()[0];
    await page.getByTestId('workbench').waitFor({ timeout: startupTimeoutMs });
    await until(
      async () =>
        (await page.evaluate(() => window.vigilshell.getSnapshot())).runtime.status === 'ready',
      'Backend not ready',
      startupTimeoutMs,
    );
    return page;
  }
  async function start() {
    assert(!attached);
    await execute(process.execPath, ['scripts/dev-preflight.mjs', '--process-only'], { cwd: root });
    const socket = createServer();
    await new Promise((done) => socket.listen(0, '127.0.0.1', done));
    const port = socket.address().port;
    await new Promise((done) => socket.close(done));
    endpoint = `http://127.0.0.1:${port}`;
    assert(process.env.npm_execpath, 'Run through npm');
    const environment = { ...process.env };
    for (const key of [
      'ELECTRON_RUN_AS_NODE',
      'ELECTRON_EXEC_PATH',
      'VIGILSHELL_DEV_APP',
      'ELECTRON_RENDERER_URL',
    ])
      delete environment[key];
    launcher = spawn(
      process.execPath,
      [process.env.npm_execpath, 'run', 'dev:app', '--', '--remoteDebuggingPort', String(port)],
      { cwd: root, env: environment, detached: true, stdio: ['ignore', 'pipe', 'pipe'] },
    );
    launcher.stdout.on('data', (chunk) => {
      output += chunk;
    });
    launcher.stderr.on('data', (chunk) => {
      output += chunk;
    });
    launcher.on('error', (error) => {
      output += String(error);
    });
    return connect();
  }
  async function identity() {
    const port = new URL(endpoint).port;
    const { stdout } = await execute('lsof', ['-nP', `-iTCP:${port}`, '-sTCP:LISTEN', '-t']);
    const main = Number(stdout.trim().split('\n')[0]);
    assert(main > 0);
    const processes = (await execute('ps', ['-axo', 'pid=,ppid=,command='])).stdout
      .split('\n')
      .flatMap((line) => {
        const m = line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/);
        return m && Number(m[2]) === main && m[3].includes('vigilshell-backend --data-dir')
          ? [{ pid: Number(m[1]), command: m[3] }]
          : [];
      });
    assert.equal(processes.length, 1);
    assert(processes[0].command.includes(dataRoot));
    assert(processes[0].command.includes('.cache/vigilshell-dev/native/'));
    const id = (
      await execute('plutil', [
        '-extract',
        'CFBundleIdentifier',
        'raw',
        '-o',
        '-',
        join(bundle, 'Contents/Info.plist'),
      ])
    ).stdout.trim();
    assert.equal(id, 'com.sparkwild.vigilshell.electron.dev');
    await execute('codesign', ['--verify', '--deep', '--strict', bundle]);
    return { main, backend: processes[0].pid };
  }
  let windowProbe;
  async function visibleWindows(pid) {
    if (!windowProbe) {
      windowProbe = join(evidence, 'visible-windows');
      await execute('swiftc', ['tests/helpers/dev-windows.swift', '-o', windowProbe], {
        cwd: root,
      });
    }
    return Number((await execute(windowProbe, [String(pid)])).stdout.trim());
  }
  async function move(x, y) {
    const { main } = await identity();
    await visibleWindows(main);
    const bounds = JSON.parse((await execute(windowProbe, [String(main), 'bounds'])).stdout);
    const height = await page.evaluate(() => innerHeight);
    await execute(windowProbe, [
      String(main),
      'move',
      String(bounds.X + x),
      String(bounds.Y + bounds.Height - height + y),
    ]);
  }
  async function hover(locator) {
    const box = await locator.boundingBox();
    assert(box, 'Hover target must be visible');
    await move(box.x + box.width / 2, box.y + box.height / 2);
  }
  async function quit() {
    assert(!attached, 'An attached Dev remains owned by the user');
    const owned = await identity();
    await execute('osascript', ['-e', `tell application ${JSON.stringify(bundle)} to quit`]);
    await until(() => !alive(owned.main) && !alive(owned.backend), 'Dev did not quit normally');
    await until(
      () => launcher.exitCode !== null || launcher.signalCode !== null,
      'Dev launcher did not exit',
    );
    assert.equal(launcher.exitCode, 0);
    await browser?.close();
    browser = null;
  }
  async function cleanup(normalQuit) {
    const errors = [];
    try {
      if (!attached && launcher) {
        if (normalQuit && launcher.exitCode === null && launcher.signalCode === null) {
          try {
            await quit();
          } catch (error) {
            errors.push(error);
            await stopLauncher(launcher);
          }
        } else if (!normalQuit) await stopLauncher(launcher);
      }
    } catch (error) {
      errors.push(error);
    } finally {
      try {
        await browser?.close();
        browser = null;
      } catch (error) {
        errors.push(error);
      }
      try {
        await writeFile(join(evidence, 'development.log'), output);
        const after = await businessFiles();
        await writeFile(
          join(evidence, 'data-preservation.json'),
          JSON.stringify({ before, after }, null, 2),
        );
        assert.deepEqual(after, before, 'Dev business files changed');
      } catch (error) {
        errors.push(error);
      }
    }
    if (errors.length) throw new AggregateError(errors, 'Dev acceptance cleanup failed');
  }
  let closing;
  function close() {
    return (closing ??= cleanup(true));
  }
  try {
    if (attached) await connect();
    else await start();
  } catch (error) {
    const errors = [error];
    try {
      await writeFile(
        join(evidence, 'startup-failure.json'),
        JSON.stringify({ message: error.message, attached: !!attached }, null, 2),
      );
    } catch (evidenceError) {
      errors.push(evidenceError);
    }
    try {
      await cleanup(false);
    } catch (cleanupError) {
      errors.push(cleanupError);
    }
    if (errors.length > 1)
      throw new AggregateError(errors, `Dev initialization and cleanup failed: ${evidence}`, {
        cause: error,
      });
    error.message += ` (evidence: ${evidence})`;
    throw error;
  }
  return {
    get endpoint() {
      return endpoint;
    },
    get page() {
      return page;
    },
    get output() {
      return output;
    },
    evidence,
    identity,
    visibleWindows,
    move,
    hover,
    start,
    connect,
    quit,
    close,
  };
}
