import { readdirSync } from 'node:fs';
import { resolve, join, relative } from 'node:path';
import { spawnSync } from 'node:child_process';
const desktop = resolve('.');
const tests = [];
function walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name);
    if (entry.isDirectory()) walk(file);
    else if (
      entry.name.endsWith('.test.tsx') ||
      (file.includes('components/ui/') && entry.name.endsWith('.test.ts'))
    )
      tests.push(relative(desktop, file));
  }
}
walk(join(desktop, 'src/renderer'));
if (!tests.length) throw new Error('No renderer UI tests found');
const result = spawnSync(
  process.execPath,
  [resolve('node_modules/vitest/vitest.mjs'), 'run', ...tests.sort()],
  { cwd: desktop, stdio: 'inherit', env: process.env },
);
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
