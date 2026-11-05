import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { dirname, resolve, sep } from 'node:path';

/** 编译入口使用 Paraglide CLI，避免把其他打包器的声明引入 Vite 类型环境。 */
export function localeDevelopmentPlugin(root) {
  const require = createRequire(resolve(root, 'package.json'));
  const compiler = resolve(dirname(require.resolve('@inlang/paraglide-js')), '../bin/run.js');
  return {
    name: 'vigilshell-locale-development',
    configureServer(server) {
      let running = false,
        dirty = false,
        closed = false,
        child;
      const messages = resolve(root, 'messages');
      const settings = resolve(root, 'project.inlang/settings.json');
      const rebuild = async () => {
        if (running || closed) return;
        running = true;
        try {
          while (dirty && !closed) {
            dirty = false;
            await new Promise((done) => {
              child = spawn(
                process.execPath,
                [
                  compiler,
                  'compile',
                  '--project',
                  './project.inlang',
                  '--outdir',
                  './src/shared/i18n/generated',
                  '--strategy',
                  'globalVariable',
                  'baseLocale',
                  '--output-structure',
                  'locale-modules',
                ],
                { cwd: root, stdio: 'inherit' },
              );
              child.once('error', (error) => {
                console.error(error);
                done();
              });
              child.once('close', (code) => {
                if (code) console.error('Locale compilation failed', code);
                done();
              });
            });
          }
        } finally {
          running = false;
        }
      };
      const changed = (_event, file) => {
        if (!(file.startsWith(messages + sep) || file === settings)) return;
        dirty = true;
        void rebuild();
      };
      server.watcher.add([messages, settings]);
      server.watcher.on('all', changed);
      server.httpServer?.once('close', () => {
        closed = true;
        server.watcher.off('all', changed);
        child?.kill('SIGTERM');
      });
    },
  };
}
