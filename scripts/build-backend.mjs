import { desktopProtocolVersion, workspaceProtocolVersion } from './protocol-versions.mjs';
import { createBackendBundle } from './backend-bundle.mjs';
import { checkRustToolchain } from './check-rust-toolchain.mjs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, copyFileSync, writeFileSync, chmodSync, cpSync, rmSync } from 'node:fs';
checkRustToolchain();
const release = process.argv.includes('--release');
execFileSync(
  'cargo',
  [
    'build',
    '--locked',
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
  const profile = release ? 'release' : 'debug';
  const contents = `target/${profile}/VigilShell Credential Input.app/Contents`;
  mkdirSync(`${contents}/MacOS`, { recursive: true });
  mkdirSync(`${contents}/Resources`, { recursive: true });
  rmSync(`${contents}/Resources/vigilshell.icns`, { force: true });
  copyFileSync('resources/icons/icon.icns', `${contents}/Resources/vigilshell.icns`);
  copyFileSync(
    `target/${profile}/vigilshell-credential-prompt`,
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
  rmSync('build/backend', { recursive: true, force: true });
  mkdirSync('build/backend', { recursive: true });
  if (process.platform !== 'darwin') {
    copyFileSync('target/release/' + binary, 'build/backend/' + binary);
    if (process.platform !== 'win32') chmodSync('build/backend/' + binary, 0o755);
  }
  if (process.platform === 'darwin') {
    await createBackendBundle(
      'build/backend',
      'target/release/' + binary,
      'resources/icons/icon.icns',
      process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release',
    );
    cpSync(
      'target/release/VigilShell Credential Input.app',
      'build/backend/VigilShell Credential Input.app',
      { recursive: true },
    );
    rmSync('build/backend/vigilshell-credential-prompt', { force: true });
  }
  writeFileSync(
    'build/backend/manifest.json',
    JSON.stringify({
      platform: process.platform,
      arch: process.arch,
      application: 'vigilshell',
      contractVersion: desktopProtocolVersion,
      workspaceContractVersion: workspaceProtocolVersion,
    }),
  );
}
