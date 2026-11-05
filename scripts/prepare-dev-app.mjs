import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import {
  cp,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadSigningConfig, preflight, signBundle, verifyBundle } from './macos-signing.mjs';

const projectRoot = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const require = createRequire(join(projectRoot, 'package.json'));
const iconRoot = join(projectRoot, 'resources/icons');
const applicationName = 'VigilShell Dev';
const bundleIdentifier = 'com.sparkwild.vigilshell.electron.dev';
const cacheRoot = join(projectRoot, 'node_modules/.cache/vigilshell-dev');
const bundlePath = join(cacheRoot, `${applicationName}.app`);
const sourceBundle = resolve(require('electron'), '../../..');
const resources = ['icon.icns', 'tray-icon.png', 'app-icon-source.png'];

/** @param {string} file */
function readPlist(file) {
  return JSON.parse(
    execFileSync('plutil', ['-convert', 'json', '-o', '-', file], {
      encoding: 'utf8',
    }),
  );
}

/** @param {string} file @param {Record<string, unknown>} plist */
function writePlist(file, plist) {
  execFileSync('plutil', ['-convert', 'xml1', '-o', file, '-'], {
    input: JSON.stringify(plist),
  });
}

/** 缓存原生开发外壳；业务代码仍由工作区构建，修改代码不重新签名外壳。 */
export async function prepareDevApp() {
  if (process.platform !== 'darwin') throw new Error('dev:app currently supports macOS only');
  const signing = loadSigningConfig('dev');
  preflight(signing);
  const { version } = JSON.parse(await readFile(join(projectRoot, 'package.json'), 'utf8'));
  if (typeof version !== 'string') throw new Error('VigilShell application version is missing');
  const nativeVersion = version.split(/[+-]/)[0];
  const digest = createHash('sha256')
    .update(projectRoot)
    .update(applicationName)
    .update(bundleIdentifier)
    .update(version)
    .update(JSON.stringify(signing));
  for (const file of [
    fileURLToPath(import.meta.url),
    join(projectRoot, 'scripts/macos-signing.mjs'),
    join(projectRoot, 'scripts/signing/components.mjs'),
    join(projectRoot, 'scripts/signing/native.plist'),
    join(projectRoot, 'scripts/signing/electron.plist'),
    require.resolve('electron/package.json'),
    join(projectRoot, 'node_modules/@electron/osx-sign/package.json'),
    join(sourceBundle, 'Contents/Info.plist'),
    ...resources.map((name) => join(iconRoot, name)),
  ])
    digest.update(await readFile(file));
  const fingerprint = digest.digest('hex');
  const stampPath = join(cacheRoot, 'stamp.json');
  let cached;
  try {
    cached = JSON.parse(await readFile(stampPath, 'utf8'));
  } catch {
    cached = undefined;
  }
  if (cached?.fingerprint === fingerprint) {
    try {
      execFileSync('codesign', ['--verify', '--deep', '--strict', bundlePath], {
        stdio: 'pipe',
      });
      verifyBundle(bundlePath, signing, process.arch);
      console.log('[dev:app] Reusing signed VigilShell Dev.app');
      return {
        bundlePath,
        executable: join(bundlePath, 'Contents/MacOS/Electron'),
        fingerprint,
      };
    } catch {
      /* 缓存被修改或不完整时，从锁定的 Electron 重建。 */
    }
  }
  await mkdir(cacheRoot, { recursive: true });
  const stagingRoot = await mkdtemp(join(cacheRoot, 'staging-'));
  const stagingBundle = join(stagingRoot, `${applicationName}.app`);
  try {
    await cp(sourceBundle, stagingBundle, {
      recursive: true,
      verbatimSymlinks: true,
      preserveTimestamps: true,
    });
    const plistPath = join(stagingBundle, 'Contents/Info.plist');
    const plist = readPlist(plistPath);
    plist.CFBundleName = applicationName;
    plist.CFBundleDisplayName = applicationName;
    plist.CFBundleIdentifier = bundleIdentifier;
    plist.CFBundleIconFile = 'vigilshell.icns';
    plist.CFBundleShortVersionString = nativeVersion;
    plist.CFBundleVersion = nativeVersion;
    writePlist(plistPath, plist);
    const bundleResources = join(stagingBundle, 'Contents/Resources');
    await mkdir(join(bundleResources, 'icons'), { recursive: true });
    for (const name of resources)
      await copyFile(join(iconRoot, name), join(bundleResources, 'icons', name));
    await copyFile(join(iconRoot, 'icon.icns'), join(bundleResources, 'vigilshell.icns'));
    await writeFile(
      join(bundleResources, 'vigilshell-dev.json'),
      JSON.stringify(
        {
          schemaVersion: 1,
          projectRoot,
          applicationName,
          bundleIdentifier,
        },
        null,
        2,
      ),
    );

    const frameworks = join(stagingBundle, 'Contents/Frameworks');
    for (const name of await readdir(frameworks)) {
      if (!name.endsWith('.app')) continue;
      const helper = join(frameworks, name);
      const helperPlistPath = join(helper, 'Contents/Info.plist');
      const helperPlist = readPlist(helperPlistPath);
      helperPlist.CFBundleIdentifier = String(helperPlist.CFBundleIdentifier).replace(
        'com.github.Electron',
        bundleIdentifier,
      );
      helperPlist.CFBundleName = name.replace('Electron', applicationName).replace(/\.app$/, '');
      helperPlist.CFBundleDisplayName = helperPlist.CFBundleName;
      helperPlist.CFBundleShortVersionString = nativeVersion;
      helperPlist.CFBundleVersion = nativeVersion;
      helperPlist.CFBundleIconFile = 'vigilshell.icns';
      await mkdir(join(helper, 'Contents/Resources'), { recursive: true });
      await copyFile(
        join(iconRoot, 'icon.icns'),
        join(helper, 'Contents/Resources/vigilshell.icns'),
      );
      writePlist(helperPlistPath, helperPlist);
    }
    await signBundle(stagingBundle, signing, process.arch);
    execFileSync('codesign', ['--verify', '--deep', '--strict', stagingBundle], { stdio: 'pipe' });
    await rm(bundlePath, { recursive: true, force: true });
    await rename(stagingBundle, bundlePath);
    await writeFile(stampPath, JSON.stringify({ fingerprint, bundlePath }, null, 2));
    console.log('[dev:app] Prepared signed VigilShell Dev.app');
  } finally {
    await rm(stagingRoot, { recursive: true, force: true });
  }
  return {
    bundlePath,
    executable: join(bundlePath, 'Contents/MacOS/Electron'),
    fingerprint,
  };
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(await prepareDevApp()));
}
