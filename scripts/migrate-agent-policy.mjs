import { execFileSync } from 'node:child_process';
if (process.argv.length !== 2) throw new Error('Only the default Dev directory is supported');
execFileSync(
  'cargo',
  ['run', '--locked', '-p', 'vigilshell-storage-sqlite', '--bin', 'migrate-agent-policy'],
  { stdio: 'inherit' },
);
