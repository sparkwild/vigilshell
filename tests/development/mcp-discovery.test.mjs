import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const supported = process.platform === 'darwin';
let directory, bridge;
before(() => {
  if (!supported) return;
  directory = mkdtempSync(join(tmpdir(), 'vigilshell-discovery-'));
  const binary = join(directory, 'identity.node');
  execFileSync(
    'xcrun',
    [
      'clang++',
      '-std=c++20',
      '-fobjc-arc',
      '-fblocks',
      '-bundle',
      '-undefined',
      'dynamic_lookup',
      '-mmacosx-version-min=13.0',
      '-DNAPI_VERSION=8',
      '-DNODE_GYP_MODULE_NAME=discovery_test',
      '-I',
      resolve(dirname(realpathSync(process.execPath)), '../include/node'),
      '-framework',
      'Foundation',
      '-framework',
      'LocalAuthentication',
      '-framework',
      'AppKit',
      resolve('src/main/mcp/native/identity.mm'),
      '-o',
      binary,
    ],
    { stdio: 'pipe' },
  );
  bridge = createRequire(import.meta.url)(binary);
});
after(() => {
  if (directory) rmSync(directory, { recursive: true, force: true });
});

function bundle(name, id, mode) {
  const path = join(directory, `${name}.app`);
  mkdirSync(join(path, 'Contents/MacOS'), { recursive: true });
  writeFileSync(
    join(path, 'Contents/Info.plist'),
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>${id}</string>
<key>CFBundleExecutable</key><string>client</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>`,
  );
  if (mode !== null)
    writeFileSync(join(path, 'Contents/MacOS/client'), '#!/bin/sh\nexit 99\n', { mode });
  return path;
}
test(
  'native discovery validates identity and executable without launching the application',
  { skip: !supported },
  () => {
    const valid = bundle('Valid', 'com.deepseek.dsh', 0o755);
    assert.equal(bridge.findApplication('com.deepseek.dsh', valid), valid);
    assert.equal(bridge.findApplication('com.openai.codex', valid), undefined);
    assert.equal(
      bridge.findApplication('com.deepseek.dsh', bundle('Missing', 'com.deepseek.dsh', null)),
      undefined,
    );
    assert.equal(
      bridge.findApplication(
        'com.deepseek.dsh',
        bundle('NotExecutable', 'com.deepseek.dsh', 0o644),
      ),
      undefined,
    );
    assert.equal(
      bridge.findApplication('com.deepseek.dsh', join(directory, 'Absent.app')),
      undefined,
    );
    assert.throws(
      () => bridge.findApplication('com.deepseek.dsh', 'relative.app'),
      /Invalid application path/,
    );
  },
);
