import assert from 'node:assert/strict';
import { test } from 'node:test';
import { componentRole } from '../../scripts/signing/components.mjs';
const config = {
  appId: 'example.app.dev',
  productName: 'Example Dev',
  mainExecutable: 'Electron',
  backendExecutable: 'example-backend',
  credentialsApp: 'Example Credentials.app',
  credentialsExecutable: 'example-native',
  credentialsId: 'example.credentials.dev',
};
test('native signing roles use product declarations and never inherit Electron capabilities', () => {
  assert.deepEqual(
    componentRole('Contents/Helpers/Example Dev.app/Contents/MacOS/example-backend', config),
    {
      identifier: 'example.app.dev.backend',
      role: 'backend',
      electron: false,
    },
  );
  assert.equal(
    componentRole('Contents/Helpers/Example Credentials.app/Contents/MacOS/example-native', config)
      .identifier,
    config.credentialsId,
  );
  assert.equal(componentRole('Contents/Helpers/Example Credentials.app', config).electron, false);
  assert.throws(() => componentRole('Contents/Helpers/other-backend', config), /Unregistered/);
});
test('Electron helper roles stay distinct across shell and packaged executable names', () => {
  const identities = new Set();
  for (const suffix of ['', ' (GPU)', ' (Plugin)', ' (Renderer)']) {
    const shell = componentRole(`Contents/Frameworks/Electron Helper${suffix}.app`, config);
    const packaged = componentRole(
      `Contents/Frameworks/Example Dev Helper${suffix}.app/Contents/MacOS/Example Dev Helper${suffix}`,
      config,
    );
    assert.equal(shell.identifier, packaged.identifier);
    identities.add(shell.identifier);
  }
  assert.equal(identities.size, 4);
});
test('unregistered nested code is rejected instead of deriving an identity from its filename', () => {
  assert.equal(
    componentRole('Contents/Frameworks/Electron Framework.framework', config).identifier,
    'com.github.Electron.framework',
  );
  assert.throws(
    () => componentRole('Contents/Frameworks/Unregistered.framework/Versions/A/payload', config),
    /Unregistered/,
  );
  assert.throws(() => componentRole('Contents/Resources/payload', config), /Unregistered/);
  assert.throws(
    () => componentRole('Contents/Frameworks/Electron Helper (Other).app', config),
    /Unregistered/,
  );
});

test('Backend bundle and its executable have native permissions, including a standalone Dev generation', () => {
  assert.equal(componentRole('Contents/Helpers/Example Dev.app', config).role, 'backend');
  const native = { ...config, nativeRoot: 'backend', mainExecutable: 'example-backend' };
  assert.deepEqual(
    componentRole('Contents/MacOS/example-backend', native),
    componentRole('', native),
  );
  assert.equal(componentRole('', native).electron, false);
  assert.throws(() => componentRole('Contents/Helpers/example-backend', config), /Unregistered/);
  assert.throws(() => componentRole('Contents/Resources/unknown', native), /Unregistered/);
});

test('notification bridge has a dedicated native identity', () => {
  assert.deepEqual(componentRole('Contents/Resources/native/authorization.node', config), {
    identifier: 'example.app.dev.notifications',
    role: 'notification-authorization',
    electron: false,
  });
});
