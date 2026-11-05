import { writeFileSync } from 'node:fs';
import { basename } from 'node:path';
import { loadSigningConfig, requirement, run, sha256 } from './macos-signing.mjs';

export default async function signArtifact(event) {
  if (!event.file?.endsWith('.dmg')) return;
  const config = loadSigningConfig(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release');
  const identifier = 'com.sparkwild.vigilshell.disk-image';
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
    event.file,
  ]);
  run('/usr/bin/codesign', [
    '--verify',
    '--strict',
    '-R',
    `=${requirement(config.identity, identifier)}`,
    event.file,
  ]);
  writeFileSync(`${event.file}.sha256`, `${sha256(event.file)}  ${basename(event.file)}\n`);
}
