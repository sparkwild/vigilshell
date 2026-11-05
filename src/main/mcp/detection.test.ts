// @vitest-environment node
import { expect, it } from 'vitest';
import { detectClients, type DetectionPorts } from './detection';
it('distinguishes installations from residual configuration without launching clients', async () => {
  const executableCalls: string[] = [];
  const ports: DetectionPorts = {
    home: '/users/test',
    realpath: async (path) => path,
    path: '/custom tools/bin',
    application: (id) =>
      id === 'com.todesktop.230313mzl4w4u92' ? '/Apps/Custom Cursor.app' : undefined,
    file: async (path) =>
      path === '/Apps/Custom Cursor.app'
        ? 'directory'
        : path === '/users/test/.claude.json'
          ? 'file'
          : null,
    executable: async (path) => {
      executableCalls.push(path);
      return path === '/custom tools/bin/codex';
    },
    nodeVersions: async () => [],
  };
  const list = await detectClients(ports);
  expect(list.find((c) => c.client === 'cursor')?.installed).toBe(true);
  expect(list.find((c) => c.client === 'codex')?.installed).toBe(true);
  expect(list.find((c) => c.client === 'claudeCode')).toMatchObject({
    installed: false,
    configurationFound: true,
  });
  expect(executableCalls).toContain('/custom tools/bin/codex');
});
it('reports inaccessible discovery data while retaining manual setup', async () => {
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '',
    application: () => {
      throw Error('unavailable');
    },
    file: async () => {
      throw Error('permission');
    },
    executable: async () => false,
    nodeVersions: async () => [],
  });
  expect(list.every((c) => !c.installed && c.unavailable)).toBe(true);
});
it('discovers DSH by its desktop identity without requiring a CLI', async () => {
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '',
    application: (id) =>
      id === 'com.deepseek.dsh' ? '/Applications/DeepSeek Harness.app' : undefined,
    file: async (path) =>
      path === '/Applications/DeepSeek Harness.app'
        ? 'directory'
        : path === '/users/test/.dsh/cordis.patch.yml'
          ? 'file'
          : null,
    executable: async () => false,
    nodeVersions: async () => [],
  });
  expect(list.find((c) => c.client === 'dsh')).toEqual({
    client: 'dsh',
    installed: true,
    configurationFound: true,
    unavailable: false,
  });
  expect(list.find((c) => c.client === 'claudeCode')).toEqual({
    client: 'claudeCode',
    installed: false,
    configurationFound: false,
    unavailable: false,
  });
});
it('isolates unreadable executable paths and node installations from other clients', async () => {
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '/denied',
    application: (id) =>
      id === 'com.deepseek.dsh' ? '/Applications/DeepSeek Harness.app' : undefined,
    file: async (path) => (path === '/Applications/DeepSeek Harness.app' ? 'directory' : null),
    executable: async (path) => {
      if (path.startsWith('/denied/')) throw Object.assign(Error('denied'), { code: 'EACCES' });
      return path === '/users/test/.local/bin/codex';
    },
    nodeVersions: async () => {
      throw Object.assign(Error('denied'), { code: 'EACCES' });
    },
  });
  expect(list.find((c) => c.client === 'codex')).toMatchObject({ installed: true });
  expect(list.find((c) => c.client === 'claudeCode')).toMatchObject({
    installed: false,
    unavailable: true,
  });
  expect(list.find((c) => c.client === 'dsh')).toMatchObject({
    installed: true,
    unavailable: false,
  });
});
it('requires configuration paths to have the expected file type', async () => {
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '',
    application: () => undefined,
    file: async (path) =>
      path.endsWith('.claude.json') ? 'directory' : path.endsWith('.cursor') ? 'file' : null,
    executable: async () => false,
    nodeVersions: async () => [],
  });
  expect(list.every((c) => !c.installed && !c.configurationFound)).toBe(true);
});
it('checks standard desktop locations when Launch Services has no result', async () => {
  const calls: Array<[string, string | undefined]> = [];
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '',
    application: (id, candidate) => {
      calls.push([id, candidate]);
      return id === 'com.deepseek.dsh' &&
        candidate === '/users/test/Applications/DeepSeek Harness.app'
        ? candidate
        : undefined;
    },
    file: async (path) =>
      path === '/users/test/Applications/DeepSeek Harness.app' ? 'directory' : null,
    executable: async () => false,
    nodeVersions: async () => [],
  });
  expect(list.find((c) => c.client === 'dsh')?.installed).toBe(true);
  expect(calls).toContainEqual(['com.deepseek.dsh', '/Applications/DeepSeek Harness.app']);
  expect(list.find((c) => c.client === 'claudeCode')?.installed).toBe(false);
});
it.each(['desktop', 'both'])('finds Claude %s and keeps one family result', async (mode) => {
  const list = await detectClients({
    home: '/users/test',
    realpath: async (path) => path,
    path: '/bin',
    application: (id) =>
      id === 'com.anthropic.claudefordesktop' ? '/Applications/Claude.app' : undefined,
    file: async (path) => (path === '/Applications/Claude.app' ? 'directory' : null),
    executable: async (path) => mode === 'both' && path === '/bin/claude',
    nodeVersions: async () => [],
  });
  expect(list.filter((c) => c.client === 'claudeCode')).toEqual([
    { client: 'claudeCode', installed: true, configurationFound: false, unavailable: false },
  ]);
});
it.each(['dsh', 'cursor-agent'])(
  'detects a CLI-only %s installation without configuration',
  async (command) => {
    const list = await detectClients({
      home: '/users/test',
      realpath: async (path) => path,
      path: '/bin',
      application: () => undefined,
      file: async () => null,
      executable: async (path) => path === `/bin/${command}`,
      nodeVersions: async () => [],
    });
    expect(list.find((c) => c.client === (command === 'dsh' ? 'dsh' : 'cursor'))).toMatchObject({
      installed: true,
      configurationFound: false,
    });
  },
);
