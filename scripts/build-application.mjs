import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { packageBuildContext, packageBuildEnvironment } from './build-context.mjs';
import { loadSigningConfig, preflight } from './macos-signing.mjs';

const profile = process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release';
const context = packageBuildContext(profile);
const { values } = parseArgs({ options: { mode: { type: 'string', default: context.mode } } });
if (values.mode !== context.mode) throw Error('Build mode must match the package profile');
const env = packageBuildEnvironment(profile);
const options = { stdio: 'inherit', env };
const npm = process.env.npm_execpath;
if (!npm) throw Error('Use npm run build');
execFileSync(process.execPath, ['scripts/dev-preflight.mjs', '--process-only'], options);
if (process.platform === 'darwin') preflight(loadSigningConfig(profile, env));
for (const args of [
  ['backend:build', '--', '--release'],
  ['contracts:generate'],
  ['i18n:generate'],
  ['licenses:check'],
])
  execFileSync(process.execPath, [npm, 'run', ...args], options);
execFileSync(process.execPath, ['scripts/build-notification-bridge.mjs'], options);
execFileSync(
  join('node_modules', '.bin', 'electron-vite'),
  ['build', '--mode', context.mode],
  options,
);
