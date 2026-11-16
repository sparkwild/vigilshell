import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { existsSync, lstatSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { waitUntil } from './ci-macos-lifecycle.mjs';

const service = 'com.sparkwild.vigilshell.vault';
export function assertHostedVaultContext(environment, platform, arch, data, home, exists) {
  assert.equal(environment.GITHUB_ACTIONS, 'true');
  assert.equal(environment.RUNNER_ENVIRONMENT, 'github-hosted');
  assert.equal(environment.RUNNER_OS, 'macOS');
  assert.equal(platform, 'darwin');
  assert(['arm64', 'x64'].includes(arch));
  assert.equal(environment.EXPECTED_ARCH, arch);
  assert(environment.RUNNER_TEMP);
  assert(
    ['VigilShell', 'VigilShell-dev'].some(
      (name) => data === join(home, 'Library/Application Support', name),
    ),
  );
  assert(!exists(data), 'Vault probe requires fresh disposable runner data');
}

export function assertFixtureMarker(config, marker) {
  assert.equal(config?.name, marker, 'Fixture name did not round-trip');
  assert.equal(config?.notes, marker, 'Fixture notes did not round-trip');
}

// Public product IPC may reject immediately or return an asynchronously failed save.
export async function assertVaultRejected(page, name, params) {
  const reply = await page.evaluate(
    ({ name, params }) => window.vigilshell.workspace[name](params),
    { name, params },
  );
  if (!reply.ok) {
    assert.equal(reply.code, 'vault_key_missing');
    // Distinguish backend rejection from renderer validation/transport failure.
    assert(reply.requestId && reply.runtimeId, 'Expected a backend rejection');
    return;
  }
  assert.equal(name, 'createConnection', 'Blocked configuration was readable');
  let operation = reply.value?.operation;
  assert(operation?.id, 'Blocked create did not return an operation');
  await waitUntil(
    async () => {
      const current = await page.evaluate(
        (id) => window.vigilshell.workspace.getOperation({ id }),
        operation.id,
      );
      assert.equal(current.ok, true);
      operation = current.value.operation;
      return operation.phase !== 'working';
    },
    'blocked fixture save',
    { timeoutMs: 15000 },
  );
  assert.equal(operation.phase, 'failed', 'Blocked create unexpectedly succeeded');
  assert.equal(operation.error, 'vault_key_missing');
  assert.equal(operation.connection, null);
}

// Construct BEFORE launching the app. Only this fresh hosted-runner database may be inspected.
// No Keychain value is read, exported or printed. Deletion targets one owned namespace.
export function hostedVaultProbe(data) {
  assertHostedVaultContext(
    process.env,
    process.platform,
    process.arch,
    data,
    homedir(),
    existsSync,
  );
  let namespace;
  let connection;
  let encryptedConfiguration;
  const marker = `ci-vault-${randomUUID()}`;
  const database = join(data, 'app.sqlite3');
  function sql(query) {
    assert(!lstatSync(data).isSymbolicLink());
    assert(!lstatSync(database).isSymbolicLink());
    return execFileSync('/usr/bin/sqlite3', ['-readonly', database, query], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  }
  function capture() {
    const value = sql('SELECT namespace FROM workspace_meta WHERE id=1;');
    assert.match(value, /^[0-9a-f]{32}$/);
    if (namespace) assert.equal(value, namespace, 'Vault namespace changed');
    namespace = value;
    assert.equal(sql('SELECT initialized FROM credential_vault WHERE id=1;'), '1');
  }
  function keyOperation(action) {
    assert(namespace, 'No owned vault namespace');
    const result = spawnSync(
      '/usr/bin/security',
      [action, '-s', service, '-a', `${namespace}/key/1`],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
    // Never include tool output: find returns metadata, not secret data, but it is not evidence.
    assert([0, 44].includes(result.status), 'Fixture Keychain operation failed');
    return result.status;
  }
  async function call(page, name, params) {
    const result = await page.evaluate(
      ({ name, params }) => window.vigilshell.workspace[name](params),
      { name, params },
    );
    assert.equal(result.ok, true, `Vault fixture ${name} failed: ${result.code ?? 'unknown'}`);
    return result.value;
  }
  async function ready(page) {
    await page.waitForFunction(
      async () => {
        const access = await window.vigilshell.getConnectionAccess();
        return ['ready', 'blocked'].includes(access.state?.phase);
      },
      null,
      { timeout: 60000 },
    );
    const access = await page.evaluate(() => window.vigilshell.getConnectionAccess());
    assert.equal(
      access.state?.phase,
      'ready',
      `Vault initialization failed: ${access.state?.code ?? 'unknown'}`,
    );
    capture();
  }
  return {
    async seed(page) {
      await ready(page);
      assert.equal(
        keyOperation('find-generic-password'),
        0,
        'App did not create its Keychain item',
      );
      const saved = await call(page, 'createConnection', {
        connectionId: null,
        expectedVersion: null,
        groupId: null,
        draftId: null,
        config: {
          name: marker,
          host: '127.0.0.1',
          port: 22,
          username: 'fixture',
          auth: 'password',
          notes: marker,
          proxyId: null,
          tunnels: [],
          environment: 'test',
          terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
        },
      });
      let operation = saved.operation;
      assert(operation?.id);
      await waitUntil(
        async () => {
          operation = (await call(page, 'getOperation', { id: operation.id })).operation;
          return operation.phase !== 'working';
        },
        'fixture connection save',
        { timeoutMs: 15000 },
      );
      assert.equal(operation.phase, 'success');
      connection = operation.connection;
      assert.match(connection?.id ?? '', /^[0-9a-f-]{36}$/);
      assertFixtureMarker(connection.config, marker);
      encryptedConfiguration = sql(
        `SELECT hex(config) FROM connection_profiles WHERE id='${connection.id}';`,
      );
      assert(encryptedConfiguration.length >= 96);
      assert(
        !Buffer.from(encryptedConfiguration, 'hex').includes(Buffer.from(marker)),
        'Fixture configuration was persisted as plaintext',
      );
    },
    async verify(page) {
      await ready(page);
      const saved = await call(page, 'getConnection', { id: connection.id });
      assertFixtureMarker(saved.connection.config, marker);
      assert.deepEqual(
        saved.connection.config,
        connection.config,
        'Encrypted configuration did not survive restart',
      );
      assert.equal(
        sql(`SELECT hex(config) FROM connection_profiles WHERE id='${connection.id}';`),
        encryptedConfiguration,
      );
    },
    removeKey() {
      capture();
      assert.equal(
        keyOperation('delete-generic-password'),
        0,
        'Fixture key was unexpectedly missing',
      );
    },
    async verifyMissing(page) {
      await page.waitForFunction(
        async () => (await window.vigilshell.getConnectionAccess()).state?.phase === 'blocked',
        null,
        { timeout: 60000 },
      );
      const access = await page.evaluate(() => window.vigilshell.getConnectionAccess());
      assert.equal(access.state.code, 'vault_key_missing');
      const before = sql('SELECT COUNT(*) FROM connection_profiles;');
      await assertVaultRejected(page, 'getConnection', { id: connection.id });
      await assertVaultRejected(page, 'createConnection', {
        connectionId: null,
        expectedVersion: null,
        groupId: null,
        draftId: null,
        config: { ...connection.config, name: `${marker}-blocked` },
      });
      assert.equal(
        sql('SELECT COUNT(*) FROM connection_profiles;'),
        before,
        'Blocked create persisted a connection',
      );
      capture();
      assert.equal(keyOperation('find-generic-password'), 44, 'Missing key was silently recreated');
      assert.equal(
        sql(`SELECT hex(config) FROM connection_profiles WHERE id='${connection.id}';`),
        encryptedConfiguration,
        'Missing key changed encrypted data',
      );
    },
    cleanup() {
      // The app may have initialized before an earlier assertion failed. Capture only owned data.
      if (!namespace && existsSync(database)) {
        const value = sql('SELECT namespace FROM workspace_meta WHERE id=1;');
        assert.match(value, /^[0-9a-f]{32}$/);
        namespace = value;
      }
      if (namespace) keyOperation('delete-generic-password');
    },
  };
}
