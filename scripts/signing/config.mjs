import { existsSync, readFileSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { isAbsolute, join } from 'node:path';

export const signingConfigDirectory = () =>
  join(homedir(), 'Library/Application Support/VigilShell Signing');
export function normalizeFingerprint(value) {
  const fingerprint = String(value ?? '')
    .replaceAll(':', '')
    .toUpperCase();
  if (!/^[A-F0-9]{40}$/.test(fingerprint))
    throw Error(
      "Signing identity must be a complete SHA-1 certificate fingerprint (not a name or '-')",
    );
  return fingerprint;
}
function validate(profile, value) {
  if (!['dev', 'release'].includes(profile)) throw Error('Unknown signing profile');
  const identity = normalizeFingerprint(value.identity);
  if (typeof value.keychain !== 'string' || !isAbsolute(value.keychain))
    throw Error('Signing keychain must be an absolute path; see docs/TECH_STACK.md#签名准备');
  return { profile, identity, keychain: value.keychain };
}
const read = (path) => (existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : null);
export function readSigningConfig(profile, environment, directory = signingConfigDirectory()) {
  const saved =
    environment.GITHUB_ACTIONS === 'true' ? {} : (read(join(directory, `${profile}.json`)) ?? {});
  const prefix = `VIGILSHELL_${profile.toUpperCase()}_SIGNING_`;
  if (!(environment[`${prefix}IDENTITY`] ?? saved.identity))
    throw Error(
      `Missing ${profile} signing identity; run npm run signing:configure and see docs/TECH_STACK.md#签名准备`,
    );
  return validate(profile, {
    identity: environment[`${prefix}IDENTITY`] ?? saved.identity,
    keychain: environment[`${prefix}KEYCHAIN`] ?? saved.keychain,
  });
}
export function saveSigningConfig(config, directory = signingConfigDirectory()) {
  const value = validate(config.profile, config);
  const path = join(directory, `${value.profile}.json`);
  const existing = read(path);
  if (existing) {
    if (JSON.stringify(validate(value.profile, existing)) !== JSON.stringify(value))
      throw Error('Signing configuration conflict; refusing to replace an existing identity');
    return;
  }
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  // Create-only: a concurrent writer cannot silently replace the selected identity.
  writeFileSync(path, JSON.stringify(value, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
}
export function migrateSigningConfig(profile, workspaceRoot, directory, verify) {
  const legacyPath = join(workspaceRoot, '.cache/signing', `${profile}.json`);
  const legacy = read(legacyPath);
  if (!legacy) return;
  const value = validate(profile, legacy);
  const existing = read(join(directory, `${profile}.json`));
  if (existing && JSON.stringify(validate(profile, existing)) !== JSON.stringify(value))
    throw Error('Signing configuration conflict between persistent and legacy configuration');
  verify(value);
  saveSigningConfig(value, directory);
  if (JSON.stringify(readSigningConfig(profile, {}, directory)) !== JSON.stringify(value))
    throw Error('Signing configuration migration verification failed');
  rmSync(legacyPath);
}
