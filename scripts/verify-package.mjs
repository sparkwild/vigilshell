import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadSigningConfig, verifyApplicationLayout, verifyBundle } from './macos-signing.mjs';

export default async function verifyPackage(context) {
  const config = loadSigningConfig(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release');
  const app = join(
    context.appOutDir,
    config.profile === 'dev' ? 'VigilShell Dev.app' : 'VigilShell.app',
  );
  verifyApplicationLayout(app, process.arch, config.profile);
  const components = verifyBundle(app, config, process.arch);
  writeFileSync(
    join(context.appOutDir, 'signing-report.json'),
    JSON.stringify(
      {
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
