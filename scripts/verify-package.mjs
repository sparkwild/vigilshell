import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadSigningConfig, verifyApplicationLayout, verifyBundle } from './macos-signing.mjs';

export default async function verifyPackage(context) {
  const config = loadSigningConfig(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release');
  const app = join(
    context.appOutDir,
    config.profile === 'dev' ? 'VigilShell Dev.app' : 'VigilShell.app',
  );
  for (const name of [
    'THIRD_PARTY_NOTICES.txt',
    'index.json',
    'electron-LICENSE.txt',
    'LICENSES.chromium.html',
  ]) {
    if (
      !readFileSync(join(app, 'Contents/Resources/licenses', name)).equals(
        readFileSync(join('resources/licenses', name)),
      )
    )
      throw Error(`Packaged license differs: ${name}`);
  }
  verifyApplicationLayout(app, process.arch, config.profile);
  const components = verifyBundle(app, config, process.arch);
  writeFileSync(
    join(context.appOutDir, 'signing-report.json'),
    JSON.stringify(
      {
        version: JSON.parse(readFileSync('package.json', 'utf8')).version,
        sourceSha: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
        profile: config.profile,
        arch: process.arch,
        identity: config.identity,
        components: components.map((component) => ({
          ...component,
          path: component.path.slice(app.length + 1) || '.',
        })),
      },
      null,
      2,
    ) + '\n',
  );
}
