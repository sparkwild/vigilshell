import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { closeApplication, waitUntil, withCleanup } from '../e2e/ci-macos-lifecycle.mjs';
import { releaseStartupContext } from '../e2e/ci-macos-context.mjs';
import {
  assertHostedVaultContext,
  assertFixtureMarker,
  assertVaultRejected,
} from '../e2e/ci-vault.mjs';
import { stopHostedBundle } from '../e2e/ci-update-cleanup.mjs';

function hostedContext(arch = 'arm64', exists = () => false) {
  return {
    environment: {
      GITHUB_ACTIONS: 'true',
      RUNNER_ENVIRONMENT: 'github-hosted',
      RUNNER_OS: 'macOS',
      RUNNER_TEMP: '/runner/temp',
      EXPECTED_ARCH: arch,
    },
    platform: 'darwin',
    arch,
    home: '/runner/home',
    workspace: '/runner/work',
    exists,
  };
}

for (const [arch, folder] of [
  ['arm64', 'mac-arm64'],
  ['x64', 'mac'],
]) {
  test(`release startup selects the actual ${arch} package and release data directory`, () => {
    const context = releaseStartupContext(hostedContext(arch));
    assert.equal(context.bundle, `/runner/work/release/${folder}/VigilShell.app`);
    assert.equal(context.signingReport, `/runner/work/release/${folder}/signing-report.json`);
    assert.equal(context.data, '/runner/home/Library/Application Support/VigilShell');
  });
}

for (const existing of ['VigilShell', 'VigilShell-dev']) {
  test(`release startup refuses existing ${existing} data`, () => {
    assert.throws(
      () =>
        releaseStartupContext(
          hostedContext(
            'arm64',
            (path) => path === `/runner/home/Library/Application Support/${existing}`,
          ),
        ),
      /Refusing/,
    );
  });
}

test('release startup refuses an architecture mismatch', () => {
  const options = hostedContext();
  options.environment.EXPECTED_ARCH = 'x64';
  assert.throws(() => releaseStartupContext(options));
});

test('native visibility waits for the asynchronous window event', async () => {
  let observations = 0;
  await waitUntil(() => ++observations === 3, 'visible window', { intervalMs: 1 });
  assert.equal(observations, 3);
});

test('a window that never becomes visible fails instead of passing', async () => {
  await assert.rejects(
    waitUntil(() => false, 'visible window', { timeoutMs: 5, intervalMs: 1 }),
    /Timed out waiting for visible window/,
  );
});

test('an unresponsive native visibility query also reaches its deadline', async () => {
  await assert.rejects(
    waitUntil(() => new Promise(() => {}), 'native query', { timeoutMs: 5 }),
    /Timed out waiting for native query/,
  );
});

test('shutdown uses the retained process after Playwright disposes its dispatcher', async () => {
  const child = { exitCode: null, signalCode: null };
  await closeApplication(
    {
      close: async () => {
        child.exitCode = 0;
      },
      process: () => {
        throw new Error('dispatcher disposed');
      },
    },
    child,
  );
});

test('abnormal application exit still fails verification', async () => {
  await assert.rejects(
    closeApplication({ close: async () => {} }, { exitCode: 1, signalCode: null }),
    /Application must quit normally/,
  );
});

test('hosted update cleanup catches late children and force-stops only the test bundle', async () => {
  const bundle = '/fixture/VigilShell Dev.app';
  let entries = [
    { pid: 1, executable: `${bundle}/Contents/MacOS/VigilShell Dev` },
    { pid: 9, executable: '/other/Application' },
  ];
  const signals = [];
  await stopHostedBundle(bundle, () => entries, {
    environment: {
      GITHUB_ACTIONS: 'true',
      RUNNER_ENVIRONMENT: 'github-hosted',
      RUNNER_OS: 'macOS',
    },
    graceMs: 5,
    intervalMs: 1,
    signal: (pid, name) => {
      signals.push([pid, name]);
      if (pid === 1)
        entries = [
          ...entries.filter((entry) => entry.pid !== 1),
          { pid: 2, executable: `${bundle}/Contents/Helpers/backend` },
        ];
      if (pid === 2 && name === 'SIGKILL') entries = entries.filter((entry) => entry.pid !== 2);
    },
  });
  assert(signals.some(([pid, name]) => pid === 2 && name === 'SIGKILL'));
  assert(!signals.some(([pid]) => pid === 9));
  assert.equal(entries[0].pid, 9);
});

test('hosted update cleanup refuses local machines and reports unkillable owned processes', async () => {
  await assert.rejects(
    stopHostedBundle(
      '/fixture/App',
      () => {
        throw Error('must not inspect processes');
      },
      { environment: {} },
    ),
  );
  await assert.rejects(
    stopHostedBundle('/fixture/App', () => [{ pid: 1, executable: '/fixture/App/Contents/main' }], {
      environment: {
        GITHUB_ACTIONS: 'true',
        RUNNER_ENVIRONMENT: 'github-hosted',
        RUNNER_OS: 'macOS',
      },
      signal: () => {},
      graceMs: 0,
      timeoutMs: 5,
      intervalMs: 1,
    }),
    /Timed out waiting for hosted update process cleanup/,
  );
});

test('cleanup preserves the original failure and reports its own failure too', async () => {
  const original = new Error('window was not visible');
  const cleanup = new Error('close failed');
  await assert.rejects(
    withCleanup(
      async () => {
        throw original;
      },
      async () => {
        throw cleanup;
      },
    ),
    (error) => {
      assert.deepEqual(error.errors, [original, cleanup]);
      return true;
    },
  );
});

for (const [name, github, runner] of [
  ['local developer machine', '', ''],
  ['self-hosted runner', 'true', 'self-hosted'],
]) {
  test(`CI startup refuses a ${name} before accessing application data`, () => {
    const result = spawnSync(process.execPath, ['tests/e2e/ci-macos-startup.mjs'], {
      encoding: 'utf8',
      env: { ...process.env, GITHUB_ACTIONS: github, RUNNER_ENVIRONMENT: runner },
      timeout: 10000,
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Startup smoke requires a disposable GitHub-hosted macOS runner/);
  });
}

// Pure guard tests only: never launch the app, query a database or access Keychain.
function validateVaultContext(options, name = 'VigilShell') {
  return assertHostedVaultContext(
    options.environment,
    options.platform,
    options.arch,
    `${options.home}/Library/Application Support/${name}`,
    options.home,
    options.exists,
  );
}
for (const arch of ['arm64', 'x64']) {
  for (const name of ['VigilShell', 'VigilShell-dev']) {
    test(`vault fixture accepts fresh hosted ${arch} ${name} context`, () => {
      validateVaultContext(hostedContext(arch), name);
    });
  }
}
for (const [key, value] of [
  ['GITHUB_ACTIONS', undefined],
  ['RUNNER_ENVIRONMENT', 'self-hosted'],
  ['RUNNER_OS', 'Linux'],
  ['EXPECTED_ARCH', 'x64'],
  ['RUNNER_TEMP', ''],
]) {
  test(`vault fixture refuses unsafe ${key}`, () => {
    const options = hostedContext();
    options.environment[key] = value;
    assert.throws(() => validateVaultContext(options));
  });
}
test('vault fixture refuses existing data, unsupported architecture and unrelated paths', () => {
  assert.throws(() => validateVaultContext(hostedContext('arm64', () => true)));
  assert.throws(() => validateVaultContext(hostedContext('ia32')));
  assert.throws(() => validateVaultContext(hostedContext(), '../OtherApp'));
  const options = hostedContext();
  options.platform = 'linux';
  assert.throws(() => validateVaultContext(options));
});

test('vault marker checks reject common loss of name or notes', () => {
  assertFixtureMarker({ name: 'fixture', notes: 'fixture' }, 'fixture');
  for (const config of [
    {},
    { name: 'fixture' },
    { notes: 'fixture' },
    { name: 'wrong', notes: 'fixture' },
  ]) {
    assert.throws(() => assertFixtureMarker(config, 'fixture'));
  }
});
function repliesPage(...replies) {
  return {
    evaluate: async () => {
      assert(replies.length, 'Unexpected IPC call');
      return replies.shift();
    },
  };
}
const missingReply = {
  ok: false,
  code: 'vault_key_missing',
  requestId: 'request',
  runtimeId: 'runtime',
};
test('vault blocked probe accepts backend get/create rejection', async () => {
  for (const method of ['getConnection', 'createConnection'])
    await assertVaultRejected(repliesPage(missingReply), method, {});
});
test('vault blocked probe accepts asynchronously failed create', async () => {
  await assertVaultRejected(
    repliesPage(
      { ok: true, value: { operation: { id: 'operation', phase: 'working' } } },
      {
        ok: true,
        value: {
          operation: {
            id: 'operation',
            phase: 'failed',
            error: 'vault_key_missing',
            connection: null,
          },
        },
      },
    ),
    'createConnection',
    {},
  );
});
test('vault blocked probe rejects readable data, successful saves and unrelated errors', async () => {
  await assert.rejects(() =>
    assertVaultRejected(repliesPage({ ok: true, value: { connection: {} } }), 'getConnection', {}),
  );
  for (const reply of [
    { ok: false, code: 'runtime_unavailable' },
    { ok: false, code: 'vault_key_missing' },
  ])
    await assert.rejects(() => assertVaultRejected(repliesPage(reply), 'getConnection', {}));
  for (const operation of [
    { id: 'operation', phase: 'success', error: null, connection: {} },
    { id: 'operation', phase: 'failed', error: 'invalid_input', connection: null },
  ]) {
    await assert.rejects(() =>
      assertVaultRejected(
        repliesPage(
          { ok: true, value: { operation: { id: 'operation' } } },
          { ok: true, value: { operation } },
        ),
        'createConnection',
        {},
      ),
    );
  }
});
