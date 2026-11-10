import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadSigningConfig, verifyApplicationLayout, verifyBundle } from './macos-signing.mjs';
import { verifyPackageContents } from './verify-package-contents.mjs';

export default async function verifyPackage(context) {
  const config = loadSigningConfig(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release');
  const app = join(
    context.appOutDir,
    config.profile === 'dev' ? 'VigilShell Dev.app' : 'VigilShell.app',
  );
  const resources = join(app, 'Contents/Resources');
  const chromiumNotice = 'LICENSES.chromium.html';
  for (const name of ['THIRD_PARTY_NOTICES.txt', 'index.json', 'electron-LICENSE.txt']) {
    if (
      !readFileSync(join(resources, 'licenses', name)).equals(
        readFileSync(join('resources/licenses', name)),
      )
    )
      throw Error(`Packaged license differs: ${name}`);
  }
  if (
    existsSync(join(resources, chromiumNotice)) ||
    existsSync(join(resources, 'licenses', chromiumNotice))
  )
    throw Error('Chromium notices belong beside the application');
  verifyApplicationLayout(app, process.arch, config.profile);
  verifyPackageContents(app, config.profile);
  const components = verifyBundle(app, config, process.arch);
  copyFileSync(
    join('node_modules/electron/dist', chromiumNotice),
    join(context.appOutDir, chromiumNotice),
  );
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
