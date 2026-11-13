#!/usr/bin/env node
import { execFile, spawn } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { notifyRuntime } from './dev-runtime.mjs';

if (process.env.VIGILSHELL_DEV_CONTROL && process.env.VIGILSHELL_DEV_SUPERVISED !== '1') {
  await notifyRuntime({
    kind: 'launch',
    args: process.argv.slice(2),
    rendererUrl: process.env.ELECTRON_RENDERER_URL,
  });
  await new Promise((resolve) => {
    const timer = setInterval(() => {}, 60000);
    const stop = () => {
      clearInterval(timer);
      resolve();
    };
    for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.once(signal, stop);
  });
  process.exit(0);
}

const execute = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const lock = join(root, '.cache/vigilshell-dev/electron.lock');
const nativeRoot = join(root, '.cache/vigilshell-dev/native');
let generation;
let hostExitConfirmed = false;
const executable = process.env.VIGILSHELL_DEV_EXECUTABLE;
if (!executable) throw new Error('Native development executable is not configured');
let cancelled = false;
let ownsLock = false;
/** @type {import('node:child_process').ChildProcess | undefined} */
let child;
/** @type {Promise<void> | undefined} */
let stopping;
/** @type {Promise<void>} */
let exited = Promise.resolve();

/** @param {number} pid */
function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ESRCH') return false;
    throw error;
  }
}

/** electron-vite 会立即拉起替代进程；独占锁让新实例等待旧 Main/Rust 退出。 */
async function acquire() {
  await mkdir(dirname(lock), { recursive: true });
  const deadline = Date.now() + 10_000;
  while (!cancelled) {
    try {
      await mkdir(lock);
      ownsLock = true;
      await writeFile(join(lock, 'owner'), String(process.pid));
      return;
    } catch (error) {
      if (!(error instanceof Error) || !('code' in error) || error.code !== 'EEXIST') throw error;
      const owner = Number(await readFile(join(lock, 'owner'), 'utf8').catch(() => '0'));
      if (owner > 0 && !alive(owner)) await rm(lock, { recursive: true, force: true });
      if (Date.now() >= deadline)
        throw new Error('Another VigilShell Dev session is still running', {
          cause: error,
        });
      await delay(40);
    }
  }
}

async function stop() {
  cancelled = true;
  if (stopping) return stopping;
  const current = child;
  if (!current?.pid) return;
  const mainPid = current.pid;
  stopping = (async () => {
    const { stdout } = await execute('ps', ['-axo', 'pid=,ppid=,command=']);
    const runtimes = stdout.split('\n').flatMap((line) => {
      const match = line.trim().match(/^(\d+)\s+(\d+)\s+(.+)$/);
      return match &&
        Number(match[2]) === mainPid &&
        match[3]?.includes('vigilshell-backend --data-dir')
        ? [Number(match[1])]
        : [];
    });
    current.kill('SIGTERM');
    const timer = setTimeout(() => current.kill('SIGKILL'), 3500);
    await exited;
    clearTimeout(timer);
    const deadline = Date.now() + 3500;
    while (runtimes.some(alive) && Date.now() < deadline) await delay(40);
    for (const pid of runtimes) if (alive(pid)) process.kill(pid, 'SIGKILL');
    const reapDeadline = Date.now() + 1500;
    while (runtimes.some(alive) && Date.now() < reapDeadline) await delay(40);
    if (runtimes.some(alive)) throw new Error('Previous VigilShell Backend did not exit');
    hostExitConfirmed = true;
  })();
  return stopping;
}

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    void stop().catch((error) => {
      console.error(error);
      process.exitCode = 1;
    });
  });
}

try {
  await acquire();
  if (!cancelled) {
    const activeRuntime = JSON.parse(await readFile(join(nativeRoot, 'active.json'), 'utf8'));
    generation = activeRuntime.generation;
    if (typeof generation !== 'string' || !/^[a-f0-9-]{36}$/.test(generation))
      throw new Error('Invalid signed development runtime');
    const active = spawn(executable, process.argv.slice(2), {
      cwd: join(root, '.'),
      stdio: 'inherit',
      env: { ...process.env, VIGILSHELL_DEV_GENERATION: generation },
    });
    child = active;
    exited = new Promise((resolve, reject) => {
      active.once('error', reject);
      active.once('close', (code) => {
        process.exitCode = cancelled ? 0 : (code ?? 0);
        resolve();
      });
    });
    await exited;
    await stopping;
  }
} finally {
  try {
    // 只有监管器确认旧 Backend 已退出后才能回收其映射的签名文件。
    if (ownsLock && generation && hostExitConfirmed) {
      const current = JSON.parse(await readFile(join(nativeRoot, 'active.json'), 'utf8'));
      if (current.generation !== generation)
        await rm(join(nativeRoot, generation), { recursive: true, force: true });
    }
  } finally {
    if (ownsLock) await rm(lock, { recursive: true, force: true });
  }
}
