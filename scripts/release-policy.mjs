import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function checkVersions(root = '.') {
  const read = (name) => readFileSync(resolve(root, name), 'utf8');
  const pkg = JSON.parse(read('package.json'));
  assert(
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(pkg.version),
    'Expected a stable X.Y.Z version',
  );
  const lock = JSON.parse(read('package-lock.json'));
  assert.equal(lock.version, pkg.version);
  assert.equal(lock.packages[''].version, pkg.version);
  assert.equal(read('Cargo.toml').match(/^version = "([^"]+)"/m)?.[1], pkg.version);
  for (const block of read('Cargo.lock').split('[[package]]')) {
    if (/\nname = "vigilshell-[^"]+"/.test(block))
      assert.equal(block.match(/\nversion = "([^"]+)"/)?.[1], pkg.version);
  }
  return pkg.version;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log({ version: checkVersions() });
}
