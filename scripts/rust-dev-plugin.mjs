import { realpathSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';

/** Cargo 成功后才触发 Main 重载；编译错误不进入 electron-vite 的 closeBundle 重启路径。
 * @param {string} root
 * @returns {{ main: import('vite').Plugin, renderer: import('vite').Plugin }}
 */
export function rustDevelopmentPlugins(root) {
  // Vite 默认忽略 node_modules；重启通知必须位于可监听的工作区缓存。
  root = realpathSync(root);
  const cache = join(root, '.cache/vigilshell-dev');
  const revision = join(cache, 'runtime-revision');
  const crates = join(root, 'crates');
  const manifests = [join(root, 'Cargo.toml'), join(root, 'Cargo.lock')];
  /** @param {string} file */
  const isRustInput = (file) => {
    let path;
    try {
      path = realpathSync(file);
    } catch {
      path = resolve(file).replace(/^\/var\//, '/private/var/');
    }
    return (
      manifests.includes(path) ||
      (path.startsWith(crates + sep) && (path.endsWith('.rs') || path.endsWith('/Cargo.toml')))
    );
  };
  return {
    main: {
      name: 'vigilshell-runtime-reload-trigger',
      async buildStart() {
        await mkdir(cache, { recursive: true });
        try {
          await writeFile(revision, 'initial', { flag: 'wx' });
        } catch (error) {
          if (!(error instanceof Error) || !('code' in error) || error.code !== 'EEXIST')
            throw error;
        }
        this.addWatchFile(revision);
      },
    },
    renderer: {
      name: 'vigilshell-rust-development',
      configureServer(server) {
        let dirty = false;
        let running = false;
        let closed = false;
        /** @type {NodeJS.Timeout | undefined} */
        let timer;
        /** @type {import('node:child_process').ChildProcess | undefined} */
        let building;
        /** @param {string} command @param {string[]} args */
        const run = (command, args) =>
          new Promise((resolve, reject) => {
            const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
            building = child;
            child.once('error', reject);
            child.once('close', (code) => {
              if (building === child) building = undefined;
              if (code === 0) resolve(undefined);
              else reject(new Error(`${command} exited with ${code}`));
            });
          });
        const rebuild = async () => {
          if (running || closed) return;
          running = true;
          try {
            while (dirty && !closed) {
              dirty = false;
              try {
                console.log('[dev:app] Rust changed; rebuilding Backend and desktop contracts');
                await run(process.execPath, [join(root, 'scripts/build-backend.mjs')]);
                await run(process.execPath, [
                  join(root, 'scripts/dev-backend.mjs'),
                  '--stage-only',
                ]);
                await run(process.execPath, [join(root, 'scripts/contracts.mjs')]);
                if (!dirty && !closed) {
                  await run(process.execPath, [join(root, 'scripts/dev-backend.mjs'), '--publish']);
                  await writeFile(revision, randomUUID());
                }
              } catch (error) {
                console.error(
                  '[dev:app] Rust build failed; keeping the previous application running.',
                  error,
                );
              }
            }
          } finally {
            running = false;
          }
        };
        /** @param {string} _event @param {string} file */
        const changed = (_event, file) => {
          if (!isRustInput(file) || closed) return;
          dirty = true;
          if (running) return;
          if (timer) clearTimeout(timer);
          timer = setTimeout(() => {
            void rebuild();
          }, 120);
        };
        server.watcher.add([crates, ...manifests]);
        server.watcher.on('all', changed);
        server.httpServer?.once('close', () => {
          closed = true;
          if (timer) clearTimeout(timer);
          server.watcher.off('all', changed);
          building?.kill('SIGTERM');
        });
      },
      handleHotUpdate(context) {
        if (isRustInput(context.file)) return [];
      },
    },
  };
}
