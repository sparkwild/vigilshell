import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import {
  packageBuildContext,
  packageBuildEnvironment,
  nativeBuildContext,
} from '../../scripts/build-context.mjs';
import { buildNativeBridges } from '../../scripts/build-notification-bridge.mjs';
import { readSigningConfig } from '../../scripts/signing/config.mjs';

const require = createRequire(import.meta.url);
const { load } = require('js-yaml');
const { getFileMatchers } = require('app-builder-lib/out/fileMatcher.js');
const { expandMacro } = require('app-builder-lib/out/util/macroExpander.js');

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-build-isolation-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, 'include'));
  await writeFile(join(root, 'include/node_api.h'), 'fixture');
  await mkdir(join(root, 'signing'));
  return root;
}

test('packaged build paths separate profiles and architectures while retaining live Dev paths', () => {
  const paths = new Set();
  for (const profile of ['dev', 'release']) {
    for (const arch of ['arm64', 'x64']) {
      const context = packageBuildContext(profile, arch, '/project');
      for (const name of ['out', 'native', 'backend', 'cargo']) {
        assert(!paths.has(context[name]));
        paths.add(context[name]);
      }
    }
  }
  assert.deepEqual(nativeBuildContext({}, '/project'), {
    profile: 'dev',
    native: '/project/.cache',
  });
  assert.throws(
    () => nativeBuildContext({ VIGILSHELL_PACKAGE_PROFILE: '' }),
    /Unknown package profile/,
  );
  assert.throws(() => packageBuildContext('release', '../dev'), /Unsupported package architecture/);
});

test('release build drops inherited development launch settings without modifying its caller', () => {
  const original = {
    VIGILSHELL_DEV_APP: '1',
    ELECTRON_RENDERER_URL: 'http://localhost:5173',
    ELECTRON_EXEC_PATH: '/dev/launcher',
    VIGILSHELL_DATA_DIR: '/real/dev/data',
    VIGILSHELL_BUILD_ROOT: 'out',
    CARGO_TARGET_DIR: '/shared/target',
    PATH: '/tools',
  };
  const env = packageBuildEnvironment('release', original);
  assert.equal(env.VIGILSHELL_PACKAGE_PROFILE, 'release');
  assert.equal(resolve(env.VIGILSHELL_BUILD_ROOT), packageBuildContext('release').directory);
  assert.equal(env.CARGO_TARGET_DIR, packageBuildContext('release').cargo);
  for (const key of [
    'VIGILSHELL_DEV_APP',
    'ELECTRON_RENDERER_URL',
    'ELECTRON_EXEC_PATH',
    'VIGILSHELL_DATA_DIR',
  ]) {
    assert.equal(env[key], undefined);
    assert(original[key]);
  }
  assert.equal(env.PATH, original.PATH);
});

test('CI signing reads only explicit selected-profile settings, never saved machine identities', async (t) => {
  const root = await fixture(t);
  const directory = join(root, 'signing');
  const identity = 'A'.repeat(40);
  for (const profile of ['dev', 'release']) {
    await writeFile(
      join(directory, `${profile}.json`),
      JSON.stringify({ identity, keychain: '/saved/keychain' }),
    );
    assert.equal(readSigningConfig(profile, {}, directory).keychain, '/saved/keychain');
    assert.throws(
      () => readSigningConfig(profile, { GITHUB_ACTIONS: 'true' }, directory),
      /Missing .* signing identity/,
    );
  }
  const env = { GITHUB_ACTIONS: 'true', VIGILSHELL_RELEASE_SIGNING_IDENTITY: identity };
  assert.throws(() => readSigningConfig('release', env, directory), /absolute path/);
  env.VIGILSHELL_RELEASE_SIGNING_KEYCHAIN = '/temporary/keychain';
  assert.equal(readSigningConfig('release', env, directory).keychain, '/temporary/keychain');
  assert.throws(() => readSigningConfig('dev', env, directory), /Missing dev signing identity/);
});

for (const profile of ['dev', 'release']) {
  test(`${profile} native bridge build uses its own identity and leaves live Dev files intact`, async (t) => {
    const root = await fixture(t);
    const identity = (profile === 'dev' ? 'A' : 'B').repeat(40);
    const prefix = `VIGILSHELL_${profile.toUpperCase()}_SIGNING_`;
    const environment = {
      GITHUB_ACTIONS: 'true',
      VIGILSHELL_PACKAGE_PROFILE: profile,
      [`${prefix}IDENTITY`]: identity,
      [`${prefix}KEYCHAIN`]: '/temporary/keychain',
    };
    const components = ['notification-native', 'mcp-native'];
    for (const component of components) {
      await mkdir(join(root, '.cache', component), { recursive: true });
      await writeFile(join(root, '.cache', component, 'authorization.node'), 'live Dev');
    }
    const signatures = [];
    await buildNativeBridges({
      root,
      environment,
      include: join(root, 'include'),
      readConfig: (selected, env) => readSigningConfig(selected, env, join(root, 'signing')),
      execute: (command, args) => {
        assert.equal(command, 'xcrun');
        writeFileSync(args[args.indexOf('-o') + 1], 'compiled bridge');
      },
      sign: (path, config, identifier) => {
        assert.equal(config.identity, identity);
        assert.equal(config.profile, profile);
        assert.equal(identifier.includes('.dev.'), profile === 'dev');
        signatures.push(path);
      },
    });
    assert.equal(signatures.length, 2);
    const context = packageBuildContext(profile, process.arch, root);
    for (const component of components) {
      assert.equal(
        await readFile(join(root, '.cache', component, 'authorization.node'), 'utf8'),
        'live Dev',
      );
      assert.equal(
        await readFile(join(context.native, component, 'authorization.node'), 'utf8'),
        'compiled bridge',
      );
    }
  });
}

test('release native build refuses Dev-only configuration before compiling or writing outputs', async (t) => {
  const root = await fixture(t);
  await writeFile(
    join(root, 'signing/dev.json'),
    JSON.stringify({ identity: 'A'.repeat(40), keychain: '/dev/keychain' }),
  );
  await assert.rejects(
    buildNativeBridges({
      root,
      environment: { VIGILSHELL_PACKAGE_PROFILE: 'release' },
      include: join(root, 'include'),
      readConfig: (profile, env) => readSigningConfig(profile, env, join(root, 'signing')),
      execute: () => assert.fail('must not compile with a missing release identity'),
      sign: () => assert.fail('must not sign with Dev credentials'),
    }),
    /Missing release signing identity/,
  );
});

test('electron-builder consumes the isolated build while retaining installed out and Helpers layout', async () => {
  const config = load(
    await readFile(new URL('../../electron-builder.yml', import.meta.url), 'utf8'),
  );
  const previous = process.env.VIGILSHELL_BUILD_ROOT;
  try {
    for (const profile of ['dev', 'release']) {
      process.env.VIGILSHELL_BUILD_ROOT = packageBuildEnvironment(profile).VIGILSHELL_BUILD_ROOT;
      const context = packageBuildContext(profile);
      const options = {
        defaultSrc: process.cwd(),
        globalOutDir: resolve('release'),
        customBuildOptions: {},
        macroExpander: (pattern) => expandMacro(pattern, process.arch, {}),
      };
      const files = getFileMatchers(config, 'files', '/bundle/app', options);
      assert(
        files.some((matcher) => matcher.from === context.out && matcher.to === '/bundle/app/out'),
      );
      const resources = getFileMatchers(config, 'extraResources', '/bundle/Resources', options);
      for (const component of ['mcp-native', 'notification-native']) {
        assert(
          resources.some(
            (matcher) => matcher.from === join(context.native, component, 'authorization.node'),
          ),
        );
      }
      assert(resources.some((matcher) => matcher.from === join(context.backend, 'manifest.json')));
      const extras = getFileMatchers(config, 'extraFiles', '/bundle/Contents', options);
      assert(
        extras.some(
          (matcher) =>
            matcher.from === context.backend && matcher.to === '/bundle/Contents/Helpers',
        ),
      );
    }
  } finally {
    if (previous === undefined) delete process.env.VIGILSHELL_BUILD_ROOT;
    else process.env.VIGILSHELL_BUILD_ROOT = previous;
  }
});

test('backend packaging stages each profile from its own Cargo output without touching Dev artifacts', async (t) => {
  const root = await fixture(t);
  const toolchain = await readFile(new URL('../../rust-toolchain.toml', import.meta.url), 'utf8');
  const version = toolchain.match(/^channel\s*=\s*"([\d.]+)"/m)[1];
  await mkdir(join(root, 'bin'));
  const compiler = `#!${process.execPath}
import { mkdirSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
if (process.argv.includes('--version')) {
  console.log(basename(process.argv[1]) + ' ${version}');
} else {
  const output = join(process.argv[process.argv.indexOf('--target-dir') + 1], 'release');
  mkdirSync(output, { recursive: true });
  for (const name of ['vigilshell-backend', 'vigilshell-credential-prompt'])
    writeFileSync(join(output, name), 'compiled ' + process.env.VIGILSHELL_PACKAGE_PROFILE);
}
`;
  for (const name of ['rustc', 'cargo'])
    await writeFile(join(root, 'bin', name), compiler, { mode: 0o755 });
  await mkdir(join(root, 'resources/icons'), { recursive: true });
  await writeFile(join(root, 'resources/icons/icon.icns'), 'fixture icon');
  for (const path of ['target/debug', 'target/release', 'build/backend']) {
    await mkdir(join(root, path), { recursive: true });
    await writeFile(join(root, path, 'vigilshell-backend'), 'existing output');
  }
  for (const profile of ['dev', 'release']) {
    execFileSync(
      process.execPath,
      [fileURLToPath(new URL('../../scripts/build-backend.mjs', import.meta.url)), '--release'],
      {
        cwd: root,
        env: {
          ...process.env,
          PATH: `${join(root, 'bin')}:${process.env.PATH}`,
          VIGILSHELL_PACKAGE_PROFILE: profile,
        },
        stdio: 'pipe',
      },
    );
    const context = packageBuildContext(profile, process.arch, root);
    const name = profile === 'dev' ? 'VigilShell Dev' : 'VigilShell';
    const binary =
      process.platform === 'darwin'
        ? join(context.backend, `${name}.app/Contents/MacOS/vigilshell-backend`)
        : join(context.backend, 'vigilshell-backend');
    assert.equal(await readFile(binary, 'utf8'), `compiled ${profile}`);
    const manifest = JSON.parse(await readFile(join(context.backend, 'manifest.json'), 'utf8'));
    assert.equal(manifest.arch, process.arch);
  }
  for (const path of ['target/debug', 'target/release', 'build/backend']) {
    assert.equal(await readFile(join(root, path, 'vigilshell-backend'), 'utf8'), 'existing output');
  }
});
