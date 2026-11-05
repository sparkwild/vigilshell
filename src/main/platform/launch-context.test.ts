import { mkdtempSync, mkdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { resolveLaunchContext } from './launch-context';

let root: string;
let resources: string;
beforeEach(() => {
  root = realpathSync(mkdtempSync(join(tmpdir(), 'vigilshell-launch-')));
  resources = join(root, 'Dev.app/Contents/Resources');
  mkdirSync(resources, { recursive: true });
  mkdirSync(join(root, '.'), { recursive: true });
  const generation = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa';
  const native = join(root, '.cache/vigilshell-dev/native');
  mkdirSync(join(native, generation), { recursive: true });
  writeFileSync(join(native, 'active.json'), JSON.stringify({ schemaVersion: 1, generation }));
  writeFileSync(
    join(native, generation, 'manifest.json'),
    JSON.stringify({
      schemaVersion: 1,
      profile: 'dev',
      generation,
      arch: process.arch,
    }),
  );
  writeFileSync(
    join(resources, 'vigilshell-dev.json'),
    JSON.stringify({
      schemaVersion: 1,
      projectRoot: root,
      applicationName: 'VigilShell Dev',
      bundleIdentifier: 'com.sparkwild.vigilshell.electron.dev',
    }),
  );
});
afterEach(() => rmSync(root, { recursive: true, force: true }));
function options() {
  return {
    appPath: join(root, '.'),
    resourcesPath: resources,
    appDataPath: join(root, 'Application Support'),
    isPackaged: false,
    developmentBuild: true,
    environment: {
      VIGILSHELL_DEV_APP: '1',
      ELECTRON_RENDERER_URL: 'http://127.0.0.1:5174',
    },
  };
}
describe('launch context', () => {
  it.skipIf(process.platform !== 'darwin')(
    'rejects missing, escaped or wrong-architecture native generations',
    () => {
      const native = join(root, '.cache/vigilshell-dev/native');
      const generation = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa';
      writeFileSync(
        join(native, generation, 'manifest.json'),
        JSON.stringify({
          schemaVersion: 1,
          profile: 'dev',
          generation,
          arch: 'wrong',
        }),
      );
      expect(() => resolveLaunchContext(options())).toThrow('runtime mismatch');
      writeFileSync(
        join(native, 'active.json'),
        JSON.stringify({ schemaVersion: 1, generation: '../escape' }),
      );
      expect(() => resolveLaunchContext(options())).toThrow('Invalid signed');
      rmSync(join(native, 'active.json'));
      expect(() => resolveLaunchContext(options())).toThrow();
    },
  );
  it.skipIf(process.platform !== 'darwin')(
    'pins a launched generation while the next generation is published',
    () => {
      const generation = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa';
      writeFileSync(
        join(root, '.cache/vigilshell-dev/native/active.json'),
        JSON.stringify({
          schemaVersion: 1,
          generation: 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb',
        }),
      );
      const context = resolveLaunchContext({
        ...options(),
        environment: {
          ...options().environment,
          VIGILSHELL_DEV_GENERATION: generation,
        },
      });
      expect(context.backendExecutable).toContain(generation);
    },
  );
  it.each([true, false])('isolates unpackaged builds (development=%s)', (developmentBuild) => {
    const context = resolveLaunchContext({
      ...options(),
      developmentBuild,
      environment: {},
    });
    expect(context.dataDirectory).toBe(join(root, 'Application Support/VigilShell-dev'));
    expect(context.browserDirectory).toBe(join(context.dataDirectory, 'browser'));
  });
  it.each(['', '   ', 'relative/data'])('rejects invalid override %j', (value) => {
    expect(() =>
      resolveLaunchContext({
        ...options(),
        environment: { VIGILSHELL_DATA_DIR: value },
      }),
    ).toThrow('absolute path');
  });
  it('ignores runtime acceptance and path injection in production', () => {
    const context = resolveLaunchContext({
      ...options(),
      developmentBuild: false,
      isPackaged: true,
      environment: {
        VIGILSHELL_DATA_DIR: '/tmp/ignored',
        VIGILSHELL_ACCEPTANCE_BUILD: '1',
        VIGILSHELL_DEV_APP: '1',
      },
    });
    expect(context.applicationName).toBe('VigilShell');
    expect(context.dataDirectory).toBe(join(root, 'Application Support/VigilShell'));
  });
  it('packaged Dev uses production resources and existing Dev data', () => {
    const context = resolveLaunchContext({
      ...options(),
      isPackaged: true,
      developmentBuild: false,
      developmentPackage: true,
      environment: {
        VIGILSHELL_DATA_DIR: '/tmp/ignored',
        VIGILSHELL_DEV_APP: '1',
        ELECTRON_RENDERER_URL: 'http://malicious.invalid',
      },
    });
    expect(context).toMatchObject({
      applicationName: 'VigilShell Dev',
      nativeDevelopment: false,
      dataDirectory: join(root, 'Application Support/VigilShell-dev'),
      backendExecutable: join(
        resources,
        '../Helpers/VigilShell Dev.app/Contents/MacOS/vigilshell-backend',
      ),
      developmentUrl: undefined,
    });
  });
  it.each([false, true])(
    'resolves native development independently of isPackaged=%s',
    (isPackaged) => {
      expect(resolveLaunchContext({ ...options(), isPackaged })).toMatchObject({
        nativeDevelopment: true,
        applicationName: 'VigilShell Dev',
        developmentUrl: 'http://127.0.0.1:5174/',
        dataDirectory: join(root, 'Application Support/VigilShell-dev'),
        browserDirectory: join(root, 'Application Support/VigilShell-dev/browser'),
        iconDirectory: join(resources, 'icons'),
        backendExecutable:
          process.platform === 'darwin'
            ? join(
                root,
                '.cache/vigilshell-dev/native/aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa/VigilShell Dev.app/Contents/MacOS/vigilshell-backend',
              )
            : join(root, 'target/debug/vigilshell-backend'),
        verifyBackendManifest: false,
        developmentDockIcon: undefined,
      });
    },
  );
  it.each([false, true])(
    'ignores injected development flags in production isPackaged=%s',
    (isPackaged) => {
      rmSync(join(resources, 'vigilshell-dev.json'));
      const context = resolveLaunchContext({
        ...options(),
        isPackaged,
        developmentBuild: false,
      });
      expect(context.nativeDevelopment).toBe(false);
      expect(context.developmentUrl).toBeUndefined();
      expect(context.applicationName).toBe(isPackaged ? 'VigilShell' : 'VigilShell Dev');
    },
  );
  it('keeps packaged production data and resources isolated from test overrides', () => {
    expect(
      resolveLaunchContext({
        ...options(),
        isPackaged: true,
        developmentBuild: false,
        environment: {
          ...options().environment,
          VIGILSHELL_DATA_DIR: '/tmp/ignored',
        },
      }),
    ).toMatchObject({
      dataDirectory: join(root, 'Application Support/VigilShell'),
      browserDirectory: join(root, 'Application Support/VigilShell/browser'),
      backendExecutable:
        process.platform === 'darwin'
          ? join(resources, '../Helpers/VigilShell.app/Contents/MacOS/vigilshell-backend')
          : join(resources, 'backend/vigilshell-backend'),
      verifyBackendManifest: true,
    });
  });
  it('accepts an isolated test directory in native development', () => {
    expect(
      resolveLaunchContext({
        ...options(),
        environment: {
          ...options().environment,
          VIGILSHELL_DATA_DIR: join(root, 'test-data'),
        },
      }),
    ).toMatchObject({
      dataDirectory: join(root, 'test-data'),
      browserDirectory: join(root, 'test-data/browser'),
    });
  });
  it('rejects a native shell belonging to another checkout', () => {
    writeFileSync(
      join(resources, 'vigilshell-dev.json'),
      JSON.stringify({ schemaVersion: 1, projectRoot: '/other' }),
    );
    expect(() => resolveLaunchContext(options())).toThrow(
      'Invalid VigilShell development application',
    );
  });
  it('normalizes macOS temporary directory aliases', () => {
    expect(
      resolveLaunchContext({
        ...options(),
        appPath: options().appPath.replace(/^\/private\/var\//, '/var/'),
      }).nativeDevelopment,
    ).toBe(true);
  });
  it('retains the existing plain development data and icon paths', () => {
    expect(resolveLaunchContext({ ...options(), environment: {} })).toMatchObject({
      nativeDevelopment: false,
      dataDirectory: join(root, 'Application Support/VigilShell-dev'),
      browserDirectory: join(root, 'Application Support/VigilShell-dev/browser'),
      iconDirectory: join(root, 'resources/icons'),
      developmentDockIcon: join(root, 'resources/icons/app-icon-source.png'),
    });
  });
  it.each(['https://example.com/', 'file:///tmp/index.html', 'http://user@localhost:5173/'])(
    'rejects nonlocal or credentialed development pages: %s',
    (url) => {
      expect(() =>
        resolveLaunchContext({
          ...options(),
          environment: { ELECTRON_RENDERER_URL: url },
        }),
      ).toThrow('local HTTP origin');
    },
  );
});
