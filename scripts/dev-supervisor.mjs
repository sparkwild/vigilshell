import { setTimeout as delay } from 'node:timers/promises';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdtemp, readFile, rm, chmod } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { RuntimeCoordinator, runtimeFingerprint, vaultReadOutcome } from './dev-runtime.mjs';

export async function createDevSupervisor(root, executable) {
  const directory = await mkdtemp(join(tmpdir(), 'vs-dev-'));
  await chmod(directory, 0o700);
  const path = join(directory, 'control');
  let child;
  let exited;
  let replacing = false;
  let finish;
  const finished = new Promise((resolve) => {
    finish = resolve;
  });
  const stop = async () => {
    if (!child) return;
    child.kill('SIGTERM');
    await exited;
    child = undefined;
  };
  const coordinator = new RuntimeCoordinator({
    fingerprint: async (request) => {
      const active = JSON.parse(
        await readFile(join(root, '.cache/vigilshell-dev/native/active.json'), 'utf8'),
      );
      request.generation = active.generation;
      return runtimeFingerprint(root, active.generation);
    },
    replace: async (request) => {
      replacing = true;
      await stop();
      replacing = false;
      while (coordinator.building && !coordinator.closed) await delay(50);
      if (coordinator.closed) return;
      request = coordinator.pending ?? request;
      coordinator.pending = null;
      const applied = await coordinator.fingerprint(request);
      if (coordinator.closed) return;
      // Do not replace a newly launched backend before its first vault read settles.
      coordinator.authorization(true);
      let buffer = '';
      child = spawn(
        process.execPath,
        [join(root, 'scripts/dev-app-electron.mjs'), ...request.args],
        {
          cwd: root,
          stdio: ['ignore', 'inherit', 'pipe'],
          env: {
            ...process.env,
            VIGILSHELL_DEV_SUPERVISED: '1',
            VIGILSHELL_DEV_APP: '1',
            VIGILSHELL_DEV_EXECUTABLE: executable,
            ELECTRON_RENDERER_URL: request.rendererUrl,
          },
        },
      );
      exited = new Promise((resolve) =>
        child.once('close', (code) => {
          coordinator.authorization(false);
          if (!replacing && !coordinator.closed) finish(code === 0 ? 0 : 1);
          resolve();
        }),
      );
      child.on('error', console.error);
      child.stderr.on('data', (chunk) => {
        process.stderr.write(chunk);
        buffer += chunk.toString();
        let end;
        while ((end = buffer.indexOf('\n')) >= 0) {
          const line = buffer.slice(0, end);
          buffer = buffer.slice(end + 1);
          const pending = vaultReadOutcome(line);
          if (pending !== null) coordinator.authorization(pending);
        }
        if (buffer.length > 65536) buffer = buffer.slice(-65536);
      });
      console.log('[dev:app] Started runtime generation', request.generation);
      return applied;
    },
  });
  const server = createServer((socket) => {
    let data = '';
    socket.on('error', () => {});
    socket.on('data', (chunk) => {
      data += chunk;
      if (data.length > 65536) socket.destroy();
    });
    socket.on('end', () => {
      try {
        const message = JSON.parse(data);
        if (message.kind === 'build')
          coordinator.build(message.running === true, message.success !== false);
        else if (
          message.kind === 'launch' &&
          Array.isArray(message.args) &&
          message.args.every((x) => typeof x === 'string') &&
          typeof message.rendererUrl === 'string'
        )
          coordinator.request(message);
      } catch (error) {
        console.error('[dev:app] Invalid supervisor request', error);
      }
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(path, resolve);
  });
  return {
    path,
    finished,
    async close() {
      coordinator.close();
      await stop();
      await new Promise((resolve) => server.close(resolve));
      await rm(directory, { recursive: true, force: true });
    },
  };
}
