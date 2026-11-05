import assert from 'node:assert/strict';
import { join, resolve } from 'node:path';

// Product data may be created only in a fresh hosted runner, never on a developer's Mac.
export function releaseStartupContext({ environment, platform, arch, home, workspace, exists }) {
  assert(
    environment.GITHUB_ACTIONS === 'true' &&
      environment.RUNNER_ENVIRONMENT === 'github-hosted' &&
      environment.RUNNER_OS === 'macOS' &&
      platform === 'darwin',
    'Startup smoke requires a disposable GitHub-hosted macOS runner',
  );
  assert(environment.RUNNER_TEMP);
  assert(['arm64', 'x64'].includes(arch), 'Unsupported native release architecture');
  assert.equal(arch, environment.EXPECTED_ARCH);
  const data = join(home, 'Library/Application Support/VigilShell');
  const developmentData = join(home, 'Library/Application Support/VigilShell-dev');
  assert(!exists(data), 'Refusing to use existing release data');
  assert(!exists(developmentData), 'Refusing to run alongside existing Dev data');
  const output = resolve(workspace, 'release', arch === 'arm64' ? 'mac-arm64' : 'mac');
  return {
    data,
    developmentData,
    bundle: join(output, 'VigilShell.app'),
    signingReport: join(output, 'signing-report.json'),
    evidence: join(environment.RUNNER_TEMP, 'vigilshell-ci-evidence'),
  };
}
