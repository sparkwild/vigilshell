import { desktopProtocolVersion, workspaceProtocolVersion } from './protocol-versions.mjs';
import { backendProductName, backendBundleExecutable } from './backend-bundle.mjs';
import { componentRole } from './signing/components.mjs';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  closeSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
  statSync,
  realpathSync,
} from 'node:fs';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { signAsync } from '@electron/osx-sign';

export const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const inputAppName = 'VigilShell Credential Input.app';
export const inputExecutable = 'vigilshell-credential-prompt';

export function identifiers(profile) {
  if (!['dev', 'release'].includes(profile)) throw new Error('Unknown signing profile');
  const suffix = profile === 'dev' ? '.dev' : '';
  return {
    app: `com.sparkwild.vigilshell.electron${suffix}`,
    backend: `com.sparkwild.vigilshell.electron${suffix}.backend`,
    input: `com.sparkwild.vigilshell.credential-input${suffix}`,
  };
}

export {
  normalizeFingerprint,
  signingConfigDirectory,
  saveSigningConfig,
} from './signing/config.mjs';
import {
  normalizeFingerprint,
  signingConfigDirectory,
  readSigningConfig,
  migrateSigningConfig,
} from './signing/config.mjs';

export function loadSigningConfig(
  profile,
  environment = process.env,
  directory = signingConfigDirectory(),
) {
  identifiers(profile);
  if (directory === signingConfigDirectory())
    migrateSigningConfig(profile, projectRoot, directory, preflight);
  return readSigningConfig(profile, environment, directory);
}

// 签名工具输出只含公开身份；调用者不得把密码放入此执行入口。
export function run(command, args) {
  try {
    const result = execFileSync(command, args, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return result;
  } catch (error) {
    throw new Error(`${command} failed: ${String(error.stderr ?? '').trim() || error.message}`, {
      cause: error,
    });
  }
}

export function preflight(config, execute = run, platform = process.platform) {
  if (platform !== 'darwin') throw new Error('macOS signing requires macOS');
  const identities = execute('/usr/bin/security', [
    'find-identity',
    '-v',
    '-p',
    'codesigning',
    config.keychain,
  ]);
  if (
    !identities
      .split('\n')
      .some(
        (line) =>
          line.match(/^\s*\d+\)\s+([A-Fa-f0-9]{40})\s/)?.[1]?.toUpperCase() === config.identity,
      )
  )
    throw new Error(
      `The ${config.profile} signing identity is unavailable in the selected keychain; see docs/TECH_STACK.md#签名准备`,
    );
}

export function requirement(identity, identifier) {
  if (!/^[A-Za-z0-9._-]+$/.test(identifier))
    throw new Error(`Invalid code identifier: ${identifier}`);
  return `certificate leaf = H"${normalizeFingerprint(identity)}" and identifier "${identifier}"`;
}

function display(path, args = []) {
  // spawnSync 同时保留 codesign 的标准输出与诊断流。
  const result = spawnSync('/usr/bin/codesign', ['--display', ...args, path], {
    encoding: 'utf8',
  });
  if (result.error || result.status !== 0) throw new Error(`Cannot inspect signature: ${path}`);
  return result.stdout + result.stderr;
}

export function codeIdentifier(path) {
  if (path.endsWith('.app') || path.endsWith('.framework')) {
    const plist = path.endsWith('.app')
      ? join(path, 'Contents/Info.plist')
      : join(path, 'Resources/Info.plist');
    return run('/usr/bin/plutil', [
      '-extract',
      'CFBundleIdentifier',
      'raw',
      '-o',
      '-',
      plist,
    ]).trim();
  }
  const identifier = display(path, ['--verbose=4']).match(/^Identifier=(.+)$/m)?.[1];
  if (!identifier) throw new Error(`Missing code identifier: ${path}`);
  return identifier;
}

export function nativeIdentifier(path, profile) {
  const ids = identifiers(profile);
  if (
    path.endsWith('/vigilshell-backend') ||
    (path.endsWith('.app') && codeIdentifier(path) === ids.backend)
  )
    return ids.backend;
  if (path.endsWith(`/${inputAppName}`) || path.endsWith(`/${inputExecutable}`)) return ids.input;
  return undefined;
}

export function isMachO(path) {
  const buffer = Buffer.alloc(4);
  const fd = openSync(path, 'r');
  try {
    readSync(fd, buffer, 0, 4, 0);
  } finally {
    closeSync(fd);
  }
  return [
    'feedface',
    'cefaedfe',
    'feedfacf',
    'cffaedfe',
    'cafebabe',
    'bebafeca',
    'cafebabf',
    'bfbafeca',
  ].includes(buffer.toString('hex'));
}

export function collectCode(root) {
  const paths = [];
  function visit(path) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const child = join(path, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        visit(child);
        if (/\.(app|framework)$/.test(child)) paths.push(child);
      } else if (entry.isFile() && isMachO(child)) paths.push(child);
    }
  }
  visit(root);
  if (/\.(app|framework)$/.test(root)) paths.push(root);
  return paths;
}

function normalizeRequirement(value) {
  return value
    .replace(/"([A-Za-z0-9._-]+)"/g, '$1')
    .trim()
    .toLowerCase()
    .split(/\s+and\s+/)
    .map((part) => part.replace(/\s+/g, ' '))
    .sort()
    .join(' && ');
}

export function verifyCode(
  path,
  config,
  arch,
  expectedIdentifier,
  electron = false,
  commands = { run, display },
) {
  const req = requirement(config.identity, expectedIdentifier);
  commands.run('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${req}`, path]);
  const details = commands.display(path, ['--verbose=4', '-r-']);
  if (/Signature=adhoc/.test(details) || /designated => cdhash/.test(details))
    throw new Error(`Unstable ad-hoc identity: ${path}`);
  const actualDR = details.match(/^#?\s*designated => (.+)$/m)?.[1];
  if (!actualDR || normalizeRequirement(actualDR) !== normalizeRequirement(req))
    throw new Error(`Designated requirement does not pin the selected certificate: ${path}`);
  const entitlements = commands.display(path, ['--entitlements', ':-']);
  const keys = [...entitlements.matchAll(/<key>([^<]+)<\/key>/g)].map((match) => match[1]);
  if (keys.some((key) => !electron || key !== 'com.apple.security.cs.allow-jit'))
    throw new Error(`Unexpected signing entitlement: ${path}`);
  let architectures;
  if (!/\.(app|framework)$/.test(path)) {
    architectures = commands.run('/usr/bin/lipo', ['-archs', path]).trim().split(/\s+/);
    const expected = arch === 'x64' ? 'x86_64' : 'arm64';
    if (!architectures.includes(expected))
      throw new Error(`Wrong architecture in ${path}: expected ${expected}`);
  }
  return {
    path,
    identifier: expectedIdentifier,
    architectures,
    requirement: actualDR,
    cdhash: details.match(/^CDHash=(.+)$/m)?.[1],
    entitlements: keys,
  };
}

export function signTool(path, config, identifier) {
  run('/usr/bin/codesign', [
    '--force',
    '--sign',
    config.identity,
    '--keychain',
    config.keychain,
    '--identifier',
    identifier,
    '--timestamp=none',
    '--requirements',
    `=designated => ${requirement(config.identity, identifier)}`,
    path,
  ]);
}

function componentConfiguration(app, config) {
  const ids = identifiers(config.profile);
  const nativeRoot = app.endsWith(`/${inputAppName}`)
    ? 'credentials'
    : codeIdentifier(app) === ids.backend
      ? 'backend'
      : undefined;
  return {
    appId: ids.app,
    productName: backendProductName(config.profile),
    mainExecutable: run('/usr/bin/plutil', [
      '-extract',
      'CFBundleExecutable',
      'raw',
      '-o',
      '-',
      join(app, 'Contents/Info.plist'),
    ]).trim(),
    backendExecutable: 'vigilshell-backend',
    credentialsApp: inputAppName,
    credentialsExecutable: inputExecutable,
    credentialsId: ids.input,
    nativeRoot,
  };
}
function signingComponents(app, config) {
  const configuration = componentConfiguration(app, config);
  const root = realpathSync(app);
  return collectCode(app).map((path) => ({
    path,
    canonical: realpathSync(path),
    ...componentRole(relative(root, realpathSync(path)), configuration),
  }));
}
/** Build an explicit registry before touching signatures. Resources are sealed by their containing bundle. */
export async function signBundle(app, config, arch) {
  preflight(config);
  const expected = nativeIdentifier(app, config.profile) ?? identifiers(config.profile).app;
  if (codeIdentifier(app) !== expected) throw Error(`Unexpected application identifier: ${app}`);
  const components = signingComponents(app, config);
  for (const item of components) {
    if (
      item.path.endsWith('.app') &&
      (item.role.startsWith('electron-helper-') ||
        item.role === 'credentials' ||
        item.role === 'backend')
    )
      run('/usr/bin/plutil', [
        '-replace',
        'CFBundleIdentifier',
        '-string',
        item.identifier,
        join(item.path, 'Contents/Info.plist'),
      ]);
    if (item.path.endsWith('.framework') && codeIdentifier(item.path) !== item.identifier)
      throw Error(`Unexpected framework identifier: ${item.path}`);
  }
  const registered = new Map(components.map((item) => [item.canonical, item]));
  await signAsync({
    app,
    identity: config.identity,
    keychain: config.keychain,
    identityValidation: false,
    platform: 'darwin',
    type: config.profile === 'dev' ? 'development' : 'distribution',
    preAutoEntitlements: false,
    preEmbedProvisioningProfile: false,
    ignore: (file) => !registered.has(realpathSync(file)),
    optionsForFile: (file) => {
      const item = registered.get(realpathSync(file));
      if (!item) throw Error(`Unregistered signing component: ${file}`);
      return {
        hardenedRuntime: false,
        timestamp: 'none',
        entitlements: join(
          projectRoot,
          'scripts/signing',
          item.electron ? 'electron.plist' : 'native.plist',
        ),
        requirements: `=designated => ${requirement(config.identity, item.identifier)}`,
        additionalArguments: ['--identifier', item.identifier],
      };
    },
  });
  return verifyBundle(app, config, arch);
}
export function verifyBundle(app, config, arch) {
  if (config.profile === 'release' && nativeIdentifier(app, config.profile) === undefined)
    verifyApplicationLayout(app, arch, config.profile);
  run('/usr/bin/codesign', ['--verify', '--deep', '--strict', app]);
  return signingComponents(app, config).map((item) => {
    if (item.path.endsWith('.app') && item.role === 'backend') {
      for (const key of ['CFBundleName', 'CFBundleDisplayName']) {
        const value = run('/usr/bin/plutil', [
          '-extract',
          key,
          'raw',
          '-o',
          '-',
          join(item.path, 'Contents/Info.plist'),
        ]).trim();
        if (value !== backendProductName(config.profile))
          throw Error(`Backend product name mismatch: ${item.path}`);
      }
      if (!statSync(join(item.path, 'Contents/Resources/vigilshell.icns')).isFile())
        throw Error('Backend product icon missing');
    }
    if (item.path.endsWith('.app') && codeIdentifier(item.path) !== item.identifier)
      throw Error(`Bundle and signing identifiers differ: ${item.path}`);
    return {
      ...verifyCode(item.path, config, arch, item.identifier, item.electron),
      role: item.role,
    };
  });
}

export function verifyApplicationLayout(app, arch, profile = 'release') {
  const helpers = join(app, 'Contents/Helpers');
  for (const path of [
    join(app, 'Contents/Resources/native/authorization.node'),
    backendBundleExecutable(helpers, profile),
    join(helpers, inputAppName, 'Contents/MacOS', inputExecutable),
  ]) {
    if (!statSync(path).isFile()) throw new Error(`Missing packaged executable: ${path}`);
  }
  const manifest = JSON.parse(
    readFileSync(join(app, 'Contents/Resources/backend/manifest.json'), 'utf8'),
  );
  if (
    manifest.arch !== arch ||
    manifest.platform !== 'darwin' ||
    manifest.application !== 'vigilshell' ||
    manifest.contractVersion !== desktopProtocolVersion ||
    manifest.workspaceContractVersion !== workspaceProtocolVersion
  )
    throw new Error('Packaged Backend manifest mismatch');
}

export function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}
