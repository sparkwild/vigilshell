import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
// Only foundation contracts: no live database/SSH credentials or product-data reset.
const app = 'vigilshell';
execFileSync(process.execPath, ['--run', 'test:signing'], { stdio: 'inherit' });
execFileSync(
  process.execPath,
  [
    resolve('node_modules/vitest/vitest.mjs'),
    'run',
    ...[
      'src/main',
      'src/shared/workspace-validation.test.ts',
      'src/shared/i18n/locale.test.ts',
      'src/renderer/components/ui',
      'src/renderer/shared/ConfirmationProvider.test.tsx',
    ],
  ],
  { stdio: 'inherit' },
);
execFileSync(
  'cargo',
  [
    'test',
    '--locked',
    ...['desktop-protocol', 'backend', 'storage-sqlite', 'credentials'].flatMap((role) => [
      '-p',
      `${app}-${role}`,
    ]),
  ],
  { stdio: 'inherit' },
);
