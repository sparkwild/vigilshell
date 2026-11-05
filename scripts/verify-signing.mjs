import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import {
  identifiers,
  loadSigningConfig,
  nativeIdentifier,
  run,
  verifyBundle,
  verifyCode,
} from './macos-signing.mjs';

const { values } = parseArgs({
  options: {
    profile: { type: 'string', default: 'dev' },
    path: { type: 'string' },
    arch: { type: 'string', default: process.arch },
    against: { type: 'string' },
  },
});
if (!values.path || !['arm64', 'x64'].includes(values.arch))
  throw new Error('Use --path <app-or-backend> --arch arm64|x64 [--against <previous-backend>]');
const config = loadSigningConfig(values.profile);
const path = resolve(values.path);
const id = nativeIdentifier(path, config.profile) ?? identifiers(config.profile).backend;
const report = path.endsWith('.app')
  ? verifyBundle(path, config, values.arch)
  : [verifyCode(path, config, values.arch, id)];
if (values.against) {
  if (path.endsWith('.app'))
    throw new Error('Compare the Backend executables for the two-build check');
  const previous = verifyCode(resolve(values.against), config, values.arch, id);
  if (previous.cdhash === report[0].cdhash)
    throw new Error('Two-build validation requires different Backend code hashes');
  run('/usr/bin/codesign', ['--verify', '--strict', '-R', `=${previous.requirement}`, path]);
}
console.log(
  JSON.stringify(
    {
      profile: config.profile,
      identity: config.identity,
      arch: values.arch,
      components: report,
    },
    null,
    2,
  ),
);
