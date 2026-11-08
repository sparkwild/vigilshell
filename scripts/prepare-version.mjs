import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkVersions } from './release-policy.mjs';

export function prepareVersion(version, root = '.') {
  assert(/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version), 'Expected X.Y.Z');
  checkVersions(root);
  const names = ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock'];
  const original = names.map((name) => readFileSync(resolve(root, name), 'utf8'));
  const pkg = JSON.parse(original[0]);
  const lock = JSON.parse(original[1]);
  pkg.version = lock.version = lock.packages[''].version = version;
  const next = [
    JSON.stringify(pkg, null, 2) + '\n',
    JSON.stringify(lock, null, 2) + '\n',
    original[2].replace(/^(version = ")[^"]+(")/m, `$1${version}$2`),
    original[3].replace(/(\nname = "vigilshell-[^"]+"\nversion = ")[^"]+(")/g, `$1${version}$2`),
  ];
  try {
    names.forEach((name, i) => writeFileSync(resolve(root, name), next[i]));
    checkVersions(root);
  } catch (error) {
    names.forEach((name, i) => writeFileSync(resolve(root, name), original[i]));
    throw error;
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  assert.equal(process.argv.length, 3, 'Use npm run version:prepare -- X.Y.Z');
  prepareVersion(process.argv[2]);
  console.log('Version files updated. Review and commit before creating a tag.');
}
