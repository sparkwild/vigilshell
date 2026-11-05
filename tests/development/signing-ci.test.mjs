import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { cleanupSigning, runCleanupCommand } from '../../scripts/ci-macos-cleanup.mjs';

async function fixture(t, files) {
  const temporary = await mkdtemp(join(tmpdir(), 'vigilshell-ci-cleanup-test-'));
  t.after(() => rm(temporary, { recursive: true, force: true }));
  const root = join(temporary, 'signing');
  const evidence = join(temporary, 'evidence');
  await mkdir(root);
  for (const name of files) await writeFile(join(root, name), 'test fixture');
  return { root, evidence };
}

for (const failedCommand of [null, 'sudo', 'security', 'python3']) {
  test(`cleanup attempts every stage even when ${failedCommand ?? 'nothing'} fails`, async (t) => {
    const { root, evidence } = await fixture(t, [
      'key.pem',
      'identity.p12',
      'certificate.pem',
      'ci.keychain-db',
      'keychains-before.txt',
    ]);
    const commands = [];
    const operation = cleanupSigning(root, evidence, async (command) => {
      assert(!existsSync(join(root, 'key.pem')));
      assert(!existsSync(join(root, 'identity.p12')));
      commands.push(command);
      if (command === failedCommand) throw new Error('simulated failure or timeout');
    });
    if (failedCommand) await assert.rejects(operation, /cleanup incomplete/);
    else await operation;
    assert.deepEqual(commands, ['sudo', 'security', 'python3']);
    assert(!existsSync(root));
    const report = JSON.parse(await readFile(join(evidence, 'signing-cleanup.json'), 'utf8'));
    assert.equal(report.passed, !failedCommand);
    assert.equal(report.stages.filter((stage) => !stage.passed).length, failedCommand ? 1 : 0);
  });
}

test('partial setup and repeated cleanup remove exported files without accessing a keychain', async (t) => {
  const { root, evidence } = await fixture(t, ['key.pem', 'identity.p12']);
  const unexpected = async () => {
    assert.fail('No native command should be needed');
  };
  await cleanupSigning(root, evidence, unexpected);
  await cleanupSigning(root, evidence, unexpected);
  assert(!existsSync(root));
});

test('a hanging cleanup command is killed within its deadline', async (t) => {
  const { root } = await fixture(t, []);
  const pidFile = join(root, 'pid');
  const start = Date.now();
  await assert.rejects(
    runCleanupCommand(
      process.execPath,
      [
        '-e',
        "require('node:fs').writeFileSync(process.argv[1], String(process.pid)); setInterval(() => {}, 1000)",
        pidFile,
      ],
      1000,
    ),
    /timed out/,
  );
  assert(Date.now() - start < 5000);
  const pid = Number(await readFile(pidFile, 'utf8'));
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      process.kill(pid, 0);
    } catch (error) {
      assert.equal(error.code, 'ESRCH');
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  process.kill(pid, 'SIGKILL');
  assert.fail('Timed-out cleanup process survived');
});

test('cleanup refuses a developer machine before touching keychains', () => {
  const result = spawnSync(process.execPath, ['scripts/ci-macos-cleanup.mjs'], {
    env: { ...process.env, GITHUB_ACTIONS: '', RUNNER_ENVIRONMENT: '' },
    encoding: 'utf8',
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /requires a disposable GitHub-hosted macOS runner/);
});
