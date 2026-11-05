import { parseArgs } from 'node:util';
import {
  identifiers,
  loadSigningConfig,
  normalizeFingerprint,
  preflight,
  saveSigningConfig,
} from './macos-signing.mjs';

const { values } = parseArgs({
  options: {
    profile: { type: 'string', default: 'dev' },
    identity: { type: 'string' },
    keychain: { type: 'string' },
    check: { type: 'boolean', default: false },
  },
});
identifiers(values.profile);
if (values.check) {
  preflight(loadSigningConfig(values.profile));
  console.log(`Signing identity ready: ${values.profile}`);
} else {
  const prefix = `VIGILSHELL_${values.profile.toUpperCase()}_SIGNING_`;
  const config = loadSigningConfig(values.profile, {
    [`${prefix}IDENTITY`]: normalizeFingerprint(values.identity),
    [`${prefix}KEYCHAIN`]: values.keychain,
  });
  preflight(config);
  saveSigningConfig(config);
  console.log(`Configured ${values.profile} signing identity ${config.identity}`);
}
