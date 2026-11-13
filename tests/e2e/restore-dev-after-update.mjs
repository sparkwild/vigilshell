import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile, readdir, rename, unlink, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { loadSigningConfig, verifyBundle } from '../../scripts/macos-signing.mjs';

const evidence = resolve(process.argv[2] ?? '');
assert(
  evidence.startsWith(resolve('.cache/native-update') + '/run-'),
  'Expected native-update evidence directory',
);
const state = JSON.parse(await readFile(join(evidence, 'restore.json'), 'utf8'));
const live = resolve('node_modules/.cache/vigilshell-dev/VigilShell Dev.app');
const original = join(evidence, 'original/VigilShell Dev.app');
assert.equal(state.live, live);
assert.equal(state.original, original);
execFileSync(process.execPath, ['scripts/dev-preflight.mjs', '--process-only']);
verifyBundle(original, loadSigningConfig('dev', {}), process.arch);
await rename(live, join(evidence, 'restored-test-package.app'));
await rename(original, live);
const result = JSON.parse(await readFile(join(evidence, 'result.json'), 'utf8'));
await writeFile(
  join(evidence, 'result.json'),
  JSON.stringify({ ...result, needsDevShellRestoration: false, devShellRestored: true }, null, 2),
);
const cache = join(homedir(), 'Library/Application Support/VigilShell-dev/updates');
const pending = await readFile(join(cache, 'pending.json'), 'utf8')
  .then(JSON.parse)
  .catch((error) => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
if (pending?.release?.notes === 'VigilShell native update acceptance.') {
  for (const name of await readdir(cache))
    if (name === 'pending.json' || /^archive(?:-[a-f0-9]{128})?\.zip(?:\.tmp)?$/.test(name))
      await unlink(join(cache, name));
}
console.log('Restored the original Dev shell without reverting Dev data.');
