import { packageArchitecture } from './package-policy.mjs';
import { packageBuildEnvironment } from './build-context.mjs';
import { verifyDmg } from './verify-dmg.mjs';
import { mkdir, mkdtemp, rm, readdir, writeFile } from 'node:fs/promises';
import { publishArtifacts } from './publish-artifacts.mjs';
import { execFileSync } from 'node:child_process';
import { parseArgs } from 'node:util';
import { join } from 'node:path';
import { verifyUpdateArtifacts } from './verify-update-artifacts.mjs';
import {
  loadSigningConfig,
  preflight,
  projectRoot,
  identifiers,
  verifyBundle,
} from './macos-signing.mjs';

const { values } = parseArgs({
  options: {
    profile: { type: 'string', default: 'release' },
    arch: { type: 'string', default: process.arch },
    dir: { type: 'boolean', default: false },
  },
});
const target = packageArchitecture(values.arch);
const rustHost = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1];
if (rustHost !== target.rust) throw Error('Rust host does not match package architecture');
const config = loadSigningConfig(values.profile);
const ids = identifiers(config.profile);
const name = config.profile === 'dev' ? 'VigilShell Dev' : 'VigilShell';
const buildEnvironment = packageBuildEnvironment(config.profile);
execFileSync(process.execPath, ['scripts/dev-preflight.mjs', '--process-only'], {
  cwd: projectRoot,
  stdio: 'inherit',
});
preflight(config);
const npm = process.env.npm_execpath;
if (!npm) throw new Error('Use npm run package');
execFileSync(
  process.execPath,
  [
    npm,
    'run',
    'build',
    '--',
    '--mode',
    config.profile === 'dev' ? 'development-package' : 'production',
  ],
  {
    cwd: projectRoot,
    stdio: 'inherit',
    env: buildEnvironment,
  },
);
await mkdir(join(projectRoot, '.cache/packaging'), { recursive: true });
const work = await mkdtemp(join(projectRoot, '.cache/packaging/build-'));
const stage = join(work, 'artifacts');
const output = join(projectRoot, config.profile === 'dev' ? 'release/dev' : 'release');
try {
  const builderConfiguration = join(work, 'builder.json');
  await writeFile(
    builderConfiguration,
    JSON.stringify({
      extends: join(projectRoot, 'electron-builder.yml'),
      appId: ids.app,
      productName: name,
      directories: { output: stage },
      mac: { identity: config.identity },
      publish:
        config.profile === 'dev'
          ? { provider: 'generic', url: 'http://127.0.0.1/' }
          : { provider: 'github', owner: 'sparkwild', repo: 'vigilshell', releaseType: 'draft' },
      ...(config.profile === 'dev' ? { extraMetadata: { name: 'vigilshell-dev' } } : {}),
    }),
  );
  execFileSync(
    join(projectRoot, 'node_modules/.bin/electron-builder'),
    [
      '--mac',
      ...(values.dir ? ['--dir'] : ['dmg', 'zip']),
      `--${values.arch}`,
      '--publish',
      'never',
      '--config',
      builderConfiguration,
    ],
    {
      cwd: join(projectRoot, '.'),
      stdio: 'inherit',
      env: {
        ...buildEnvironment,
        CSC_IDENTITY_AUTO_DISCOVERY: 'false',
        CSC_KEYCHAIN: config.keychain,
      },
    },
  );

  if (!values.dir) {
    const images = (await readdir(stage)).filter((name) => name.endsWith('.dmg'));
    const image = images[0];
    if (!image || images.length !== 1) throw Error('Expected exactly one DMG per architecture');
    await verifyDmg(join(stage, image), name, (path) => verifyBundle(path, config, values.arch));
    await verifyUpdateArtifacts(stage, name, (path) => verifyBundle(path, config, values.arch));
  }
  await publishArtifacts(stage, output);
} finally {
  await rm(work, { recursive: true, force: true });
}
