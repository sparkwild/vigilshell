import {
  desktopProtocolVersion,
  workspaceProtocolVersion,
} from '../../scripts/protocol-versions.mjs';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import {
  identifiers,
  loadSigningConfig,
  normalizeFingerprint,
  preflight,
  requirement,
  verifyCode,
  verifyApplicationLayout,
} from '../../scripts/macos-signing.mjs';
import { migrateSigningConfig } from '../../scripts/signing/config.mjs';
import { publishBackend } from '../../scripts/dev-backend.mjs';

const fingerprint = 'A'.repeat(40);
const other = 'B'.repeat(40);
const config = {
  profile: 'dev',
  identity: fingerprint,
  keychain: '/keys/development.keychain-db',
};

test('signing requires a complete fingerprint', () => {
  for (const value of [undefined, '-', 'Developer ID', 'A'.repeat(39), 'A'.repeat(41)])
    assert.throws(() => normalizeFingerprint(value), /complete SHA-1/);
  assert.equal(normalizeFingerprint('aa:'.repeat(19) + 'aa'), fingerprint);
});

test('Dev configuration cannot silently select the release identity', async () => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-sign-config-'));
  try {
    await writeFile(join(root, 'dev.json'), JSON.stringify(config));
    assert.equal(
      loadSigningConfig('dev', { VIGILSHELL_RELEASE_SIGNING_IDENTITY: other }, root).identity,
      fingerprint,
    );
    assert.throws(() => loadSigningConfig('release', {}, root), /Missing release signing identity/);
    assert.throws(
      () => loadSigningConfig('dev', { VIGILSHELL_DEV_SIGNING_IDENTITY: '-' }, root),
      /fingerprint/,
    );
    assert.throws(
      () => loadSigningConfig('dev', { VIGILSHELL_DEV_SIGNING_KEYCHAIN: 'relative' }, root),
      /absolute/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('preflight rejects unavailable identities and selects the configured keychain', () => {
  assert.throws(() => preflight(config, () => '0 valid identities found', 'darwin'), /unavailable/);
  assert.throws(
    () => preflight(config, () => `1) ${other} "${fingerprint}"`, 'darwin'),
    /unavailable/,
  );
  assert.throws(() => preflight(config, () => `1) ${other} "same name"`, 'darwin'), /unavailable/);
  preflight(
    config,
    (command, args) => {
      assert.equal(command, '/usr/bin/security');
      assert.equal(args.at(-1), config.keychain);
      return `1) ${fingerprint} "VigilShell Development Signing"`;
    },
    'darwin',
  );
});

test('release layout requires both native executables and a matching manifest', async () => {
  const app = await mkdtemp(join(tmpdir(), 'vigilshell-layout-'));
  try {
    const helper = join(app, 'Contents/Helpers');
    const input = join(helper, 'VigilShell Credential Input.app/Contents/MacOS');
    const resources = join(app, 'Contents/Resources/backend');
    await mkdir(input, { recursive: true });
    await mkdir(resources, { recursive: true });
    await mkdir(join(helper, 'VigilShell.app/Contents/MacOS'), { recursive: true });
    await writeFile(join(helper, 'VigilShell.app/Contents/MacOS/vigilshell-backend'), 'fixture');
    const manifest = {
      application: 'vigilshell',
      platform: 'darwin',
      arch: 'arm64',
      contractVersion: desktopProtocolVersion,
      workspaceContractVersion: workspaceProtocolVersion,
    };
    await writeFile(join(resources, 'manifest.json'), JSON.stringify(manifest));
    assert.throws(() => verifyApplicationLayout(app, 'arm64'));
    await writeFile(join(input, 'vigilshell-credential-prompt'), 'fixture');
    assert.throws(() => verifyApplicationLayout(app, 'arm64'));
    await mkdir(join(app, 'Contents/Resources/native'), { recursive: true });
    await writeFile(join(app, 'Contents/Resources/native/authorization.node'), 'fixture');
    verifyApplicationLayout(app, 'arm64');
    assert.throws(() => verifyApplicationLayout(app, 'x64'), /mismatch/);
    await writeFile(
      join(resources, 'manifest.json'),
      JSON.stringify({ ...manifest, workspaceContractVersion: 4 }),
    );
    assert.throws(() => verifyApplicationLayout(app, 'arm64'), /mismatch/);
  } finally {
    await rm(app, { recursive: true, force: true });
  }
});

test('components have distinct identities and certificate-bound requirements', () => {
  assert.notEqual(identifiers('dev').backend, identifiers('release').backend);
  assert.notEqual(identifiers('dev').backend, identifiers('dev').input);
  assert.equal(
    requirement(fingerprint, identifiers('dev').backend),
    `certificate leaf = H"${fingerprint}" and identifier "com.sparkwild.vigilshell.electron.dev.backend"`,
  );
  assert.throws(() => requirement(fingerprint, 'bad" or true'), /Invalid code/);
});

function inspection(overrides = {}) {
  const settings = {
    arch: 'arm64',
    dr: requirement(fingerprint, identifiers('dev').backend),
    entitlements: '',
    ...overrides,
  };
  return {
    run(command, args) {
      if (command.endsWith('lipo')) return settings.arch;
      assert.ok(args.includes('--strict'));
      assert.equal(
        args[args.indexOf('-R') + 1],
        `=${requirement(fingerprint, identifiers('dev').backend)}`,
      );
      if (settings.signatureError) throw new Error('signature rejected');
      return '';
    },
    display(_path, args) {
      return args.includes('--entitlements')
        ? settings.entitlements
        : `designated => ${settings.dr}\nCDHash=123\n`;
    },
  };
}

test('verification checks architecture, certificate, entitlements and integrity', () => {
  const verify = (options) =>
    verifyCode(
      '/native/vigilshell-backend',
      config,
      'arm64',
      identifiers('dev').backend,
      false,
      inspection(options),
    );
  assert.equal(verify({}).identifier, identifiers('dev').backend);
  assert.throws(() => verify({ arch: 'x86_64' }), /Wrong architecture/);
  assert.throws(() => verify({ dr: 'cdhash H"123"' }), /ad-hoc/);
  assert.throws(() => verify({ dr: requirement(other, identifiers('dev').backend) }), /pin/);
  assert.throws(
    () => verify({ entitlements: '<key>com.apple.security.cs.allow-jit</key>' }),
    /entitlement/,
  );
  assert.throws(() => verify({ signatureError: true }), /signature rejected/);
});

test('failed verification preserves the active native generation', async () => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-native-publish-'));
  const old = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa';
  const next = 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb';
  try {
    await writeFile(
      join(root, 'active.json'),
      JSON.stringify({ schemaVersion: 1, generation: old }),
    );
    await assert.rejects(
      publishBackend(root, next, async () => {
        throw new Error('signature failed');
      }),
      /signature failed/,
    );
    assert.equal(JSON.parse(await readFile(join(root, 'active.json'))).generation, old);
    await publishBackend(root, next, async (path) => assert.equal(path, join(root, next)));
    assert.equal(JSON.parse(await readFile(join(root, 'active.json'))).generation, next);
    await assert.rejects(
      publishBackend(root, '../escape', async () => {}),
      /Invalid/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('legacy public signing configuration migrates only after verification and rejects conflicts', async () => {
  const root = await mkdtemp(join(tmpdir(), 'vigilshell-config-migrate-'));
  const directory = join(root, 'persistent');
  const legacy = join(root, '.cache/signing/dev.json');
  try {
    await mkdir(join(root, '.cache/signing'), { recursive: true });
    await writeFile(legacy, JSON.stringify(config));
    assert.throws(
      () =>
        migrateSigningConfig('dev', root, directory, () => {
          throw Error('unavailable');
        }),
      /unavailable/,
    );
    assert.equal(JSON.parse(await readFile(legacy, 'utf8')).identity, fingerprint);
    let verified = 0;
    migrateSigningConfig('dev', root, directory, () => {
      verified++;
    });
    assert.equal(verified, 1);
    assert.equal(loadSigningConfig('dev', {}, directory).identity, fingerprint);
    await assert.rejects(readFile(legacy), { code: 'ENOENT' });
    migrateSigningConfig('dev', root, directory, () => {
      throw Error('must not repeat');
    });
    await writeFile(legacy, JSON.stringify({ ...config, identity: other }));
    assert.throws(() => migrateSigningConfig('dev', root, directory, () => {}), /conflict/);
    assert.equal(loadSigningConfig('dev', {}, directory).identity, fingerprint);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
