import { desktopProtocolVersion, workspaceProtocolVersion } from './protocol-versions.mjs';
import { createBackendBundle } from './backend-bundle.mjs';
import { checkRustToolchain } from './check-rust-toolchain.mjs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, copyFileSync, writeFileSync, chmodSync, cpSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { packageBuildContext } from './build-context.mjs';
checkRustToolchain();
const release = process.argv.includes('--release');
const context = release
  ? packageBuildContext(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release')
  : undefined;
const targetDirectory = context?.cargo ?? resolve('target');
const binaryDirectory = join(targetDirectory, release ? 'release' : 'debug');
execFileSync(
  'cargo',
  [
    'build',
    '--locked',
    '--target-dir',
    targetDirectory,
    '-p',
    'vigilshell-backend',
    '-p',
    'vigilshell-credentials',
    '--bins',
    ...(release ? ['--release'] : []),
  ],
  { stdio: 'inherit' },
);
// Bundle metadata gives the native input process its own macOS identity and accessibility surface.
if (process.platform === 'darwin') {
  const contents = join(binaryDirectory, 'VigilShell Credential Input.app/Contents');
  mkdirSync(`${contents}/MacOS`, { recursive: true });
  mkdirSync(`${contents}/Resources`, { recursive: true });
  rmSync(`${contents}/Resources/vigilshell.icns`, { force: true });
  copyFileSync('resources/icons/icon.icns', `${contents}/Resources/vigilshell.icns`);
  copyFileSync(
    join(binaryDirectory, 'vigilshell-credential-prompt'),
    `${contents}/MacOS/vigilshell-credential-prompt`,
  );
  chmodSync(`${contents}/MacOS/vigilshell-credential-prompt`, 0o755);
  writeFileSync(
    `${contents}/Info.plist`,
    `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.sparkwild.vigilshell.credential-input</string><key>CFBundleName</key><string>VigilShell Credential Input</string><key>CFBundleExecutable</key><string>vigilshell-credential-prompt</string><key>CFBundleIconFile</key><string>vigilshell.icns</string><key>CFBundlePackageType</key><string>APPL</string><key>LSUIElement</key><true/><key>NSHighResolutionCapable</key><true/></dict></plist>`,
  );
}
if (release) {
  const binary = 'vigilshell-backend' + (process.platform === 'win32' ? '.exe' : '');
  const backend = context.backend;
  rmSync(backend, { recursive: true, force: true });
  mkdirSync(backend, { recursive: true });
  if (process.platform !== 'darwin') {
    copyFileSync(join(binaryDirectory, binary), join(backend, binary));
    if (process.platform !== 'win32') chmodSync(join(backend, binary), 0o755);
  }
  if (process.platform === 'darwin') {
    await createBackendBundle(
      backend,
      join(binaryDirectory, binary),
      'resources/icons/icon.icns',
      context.profile,
    );
    cpSync(
      join(binaryDirectory, 'VigilShell Credential Input.app'),
      join(backend, 'VigilShell Credential Input.app'),
      { recursive: true },
    );
    rmSync(join(backend, 'vigilshell-credential-prompt'), { force: true });
  }
  writeFileSync(
    join(backend, 'manifest.json'),
    JSON.stringify({
      platform: process.platform,
      arch: process.arch,
      application: 'vigilshell',
      contractVersion: desktopProtocolVersion,
      workspaceContractVersion: workspaceProtocolVersion,
    }),
  );
}
