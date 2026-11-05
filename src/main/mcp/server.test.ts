import { request as httpRequest } from 'node:http';
// @vitest-environment node
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import type { AgentCall } from '@contracts';
import { LocalMcpServer } from './server';
import actionFixtures from '../../../tests/fixtures/mcp-actions.json';
import schema from '../../contracts/generated/runtime.schema.json';

const token = 't'.repeat(43);
const digest = createHash('sha256').update(token).digest('hex');
const servers: LocalMcpServer[] = [];
const clients: Client[] = [];
afterEach(async () => {
  await Promise.all(clients.splice(0).map((client) => client.close()));
  await Promise.all(servers.splice(0).map((server) => server.dispose()));
  vi.unstubAllEnvs();
});
async function fixture() {
  const calls: AgentCall[] = [];
  let enabled = true;
  const rejected: string[] = [];
  const observations: string[] = [];
  const server = new LocalMcpServer({
    observed: async (entryId, generation) => {
      observations.push(`${entryId}:${generation}`);
    },
    rejected: async (_digest, _tool, code) => {
      rejected.push(code);
    },
    authenticate: async (value) => {
      if (!enabled || value !== digest) throw new Error('access_denied');
      return { endpointRevision: 0, id: 'test-entry', name: 'Test', enabled: true, generation: 1 };
    },
    invoke: async (value, call) => {
      expect(value).toBe(digest);
      calls.push(call);
      return { kind: 'connections', connections: [] };
    },
  });
  servers.push(server);
  await server.start(0);
  return {
    server,
    calls,
    rejected,
    observations,
    url: `http://127.0.0.1:${server.port}/mcp`,
    revoke: () => {
      enabled = false;
    },
  };
}
describe('app-owned MCP transport', () => {
  it.each([
    ['0', '0', 'vigilshell', 'VigilShell'],
    ['1', '0', 'vigilshell-dev', 'VigilShell Dev'],
    ['0', '1', 'vigilshell-dev', 'VigilShell Dev'],
  ])(
    'advertises channel identity for development=%s and package=%s',
    async (development, packaged, name, title) => {
      vi.stubEnv('VIGILSHELL_DEVELOPMENT_BUILD', development);
      vi.stubEnv('VIGILSHELL_DEV_PACKAGE_BUILD', packaged);
      const f = await fixture();
      const client = new Client({ name: 'identity-check', version: '1' });
      clients.push(client);
      await client.connect(
        new StreamableHTTPClientTransport(new URL(f.url), {
          requestInit: { headers: { Authorization: `Bearer ${token}` } },
        }),
      );
      expect(client.getServerVersion()).toMatchObject({ name, title, version: '0.1.0' });
      expect((await client.listTools()).tools.length).toBeGreaterThan(0);
    },
  );
  it('dispatches every public action and rejects identity injection and unknown actions', async () => {
    const f = await fixture();
    const client = new Client({ name: 'all-action-contract', version: '1' });
    clients.push(client);
    await client.connect(
      new StreamableHTTPClientTransport(new URL(f.url), {
        requestInit: { headers: { Authorization: `Bearer ${token}` } },
      }),
    );
    const advertised = await client.listTools();
    expect([...new Set(actionFixtures.map(({ call }) => call.tool))].sort()).toEqual(
      advertised.tools.map((t) => t.name).sort(),
    );
    const expected: string[] = [];
    for (const variant of schema.definitions.AgentCall.oneOf) {
      const v = variant as unknown as {
        properties: { tool: { const: string }; action?: { enum?: string[]; $ref?: string } };
        oneOf?: { properties: { action: { const: string } } }[];
      };
      const ref = v.properties.action?.$ref?.split('/').at(-1);
      const referenced = ref
        ? (schema.definitions as Record<string, { enum?: string[] }>)[ref]?.enum
        : undefined;
      for (const action of v.oneOf?.map((b) => b.properties.action.const) ??
        v.properties.action?.enum ??
        referenced ?? [''])
        expected.push(`${v.properties.tool.const}:${action}`);
    }
    expect(
      actionFixtures
        .map(({ call }) => `${call.tool}:${'action' in call ? call.action : ''}`)
        .sort(),
    ).toEqual(expected.sort());
    for (const { call } of actionFixtures) {
      const { tool, ...args } = call;
      const before = f.calls.length;
      expect(
        (await client.callTool({ name: tool, arguments: args })).isError,
        `${tool}:${'action' in args ? args.action : ''}`,
      ).not.toBe(true);
      expect(f.calls.at(-1)).toEqual(call);
      expect(f.calls).toHaveLength(before + 1);
      for (const injected of [
        { ...args, uid: 0 },
        { ...args, action: 'unknown_action' },
      ]) {
        expect((await client.callTool({ name: tool, arguments: injected })).isError).toBe(true);
        expect(f.calls).toHaveLength(before + 1);
      }
    }
  });
  it('rejects every retired tool and cross-branch or nested authority input', async () => {
    const f = await fixture();
    const client = new Client({ name: 'negative-contract', version: '1' });
    clients.push(client);
    await client.connect(
      new StreamableHTTPClientTransport(new URL(f.url), {
        requestInit: { headers: { Authorization: `Bearer ${token}` } },
      }),
    );
    for (const name of [
      'task_inspect_recovery',
      'dns_update',
      'system_settings_update',
      'scripts_run',
      'session_open',
      'session_get',
      'session_close',
      'terminal_write',
      'terminal_read',
      'terminal_resize',
      'terminal_close',
      'tasks_list',
      'task_get',
      'task_read_output',
      'transfers_upload',
      'transfers_download',
      'operations_batch',
      'policy_check',
      'terminal_open',
      'files_list',
      'files_stat',
      'files_edit',
      'files_mkdir',
      'files_copy',
      'files_move',
      'files_set_attributes',
      'files_link',
      'system_get',
      'processes_list',
      'services_list',
      'services_status',
      'services_logs',
      'packages_list',
      'packages_install',
      'packages_update',
      'packages_remove',
      'package_sources_create',
      'package_sources_update',
      'package_sources_remove',
      'package_sources_list',
      'services_register',
      'services_adopt',
      'services_update',
      'services_set_enabled',
      'services_unregister',
    ]) {
      await expect(client.callTool({ name, arguments: {} })).rejects.toThrow(/not found/);
    }
    const find = (tool: string, action: string) =>
      structuredClone(
        actionFixtures.find(
          ({ call }) => call.tool === tool && 'action' in call && call.action === action,
        )!.call,
      ) as Record<string, unknown>;
    const command = find('command_run', 'command');
    const script = find('command_run', 'script');
    const definition = find('services_define', 'create');
    const transfer = find('files_transfer', 'download');
    const service = find('services_control', 'start');
    const cases: Array<Record<string, unknown>> = [
      { ...command, source: { kind: 'inline', text: 'pwd' } },
      { ...script, command: 'pwd' },
      {
        ...definition,
        definition: {
          ...(definition.definition as object),
          content: { kind: 'unit', text: '[Service]', name: 'drop.conf' },
        },
      },
      { ...service, target: { manager: 'system', name: 'fixture.service', uid: 0 } },
      { ...service, target: { manager: 'anotherUser', name: 'fixture.service' } },
      { ...transfer, local: '/etc/passwd' },
      { ...command, policy: { level: 'operate' } },
      { ...command, action: undefined },
    ];
    for (const { tool, ...args } of cases) {
      expect((await client.callTool({ name: String(tool), arguments: args })).isError).toBe(true);
    }
    expect(f.calls).toHaveLength(0);
  });
  it.each(['legacy', 'modern'] as const)(
    'serves %s protocol with a per-request identity',
    async (era) => {
      const f = await fixture();
      const client = new Client(
        { name: 'protocol-test', version: '1' },
        {
          versionNegotiation:
            era === 'legacy' ? { mode: 'legacy' } : { mode: { pin: '2026-07-28' } },
        },
      );
      clients.push(client);
      await client.connect(
        new StreamableHTTPClientTransport(new URL(f.url), {
          requestInit: { headers: { Authorization: `Bearer ${token}` } },
        }),
      );
      const list = await client.listTools();
      expect(f.observations).toContain('test-entry:1');
      expect(list.tools).toHaveLength(22);
      expect(list.tools.map((t) => t.name)).not.toContain('policy_check');
      expect(list.tools.map((tool) => tool.name)).toContain('terminal_manage');
      expect(list.tools.some((tool) => /prepare|commit|configuration|trust/.test(tool.name))).toBe(
        false,
      );
      await client.callTool({ name: 'connections_list', arguments: {} });
      expect(f.calls).toEqual([{ tool: 'connections_list' }]);
      const bad = await client.callTool({
        name: 'connections_list',
        arguments: { action: 'commit', challenge_id: 'forged' },
      });
      expect(bad.isError).toBe(true);
      expect(f.calls).toHaveLength(1);
      expect(f.rejected).toContain('invalid_tool_arguments');
      await client.callTool({
        name: 'session_manage',
        arguments: { action: 'open', connection_id: 'connection', request_key: 'key' },
      });
      expect(f.calls.at(-1)).toEqual({
        tool: 'session_manage',
        action: 'open',
        connection_id: 'connection',
        request_key: 'key',
      });
      const injected = await client.callTool({
        name: 'session_manage',
        arguments: { action: 'open', connection_id: 'connection', request_key: 'key', uid: 0 },
      });
      expect(injected.isError).toBe(true);
      expect(f.calls).toHaveLength(2);
      await client.callTool({
        name: 'session_manage',
        arguments: {
          action: 'open',
          connection_id: 'connection',
          request_key: 'existing',
          tab_id: 'terminal-tab',
        },
      });
      expect(f.calls.at(-1)).toEqual({
        tool: 'session_manage',
        action: 'open',
        connection_id: 'connection',
        request_key: 'existing',
        tab_id: 'terminal-tab',
      });
      f.revoke();
      await expect(client.listTools()).rejects.toThrow();
    },
  );
  it('rejects missing credentials, DNS rebinding, browser origins and oversized input', async () => {
    const f = await fixture();
    expect((await fetch(f.url, { method: 'POST' })).status).toBe(401);
    const rebound = await new Promise<number>((resolve, reject) => {
      const req = httpRequest(
        f.url,
        { method: 'POST', headers: { Host: 'attacker.example', Authorization: `Bearer ${token}` } },
        (response) => {
          response.resume();
          response.on('end', () => resolve(response.statusCode ?? 0));
        },
      );
      req.on('error', reject);
      req.end();
    });
    expect(rebound).toBe(403);
    expect(
      (
        await fetch(f.url, {
          method: 'POST',
          headers: { Origin: 'http://127.0.0.1', Authorization: `Bearer ${token}` },
        })
      ).status,
    ).toBe(403);
    expect(
      (
        await fetch(f.url, {
          method: 'POST',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
          body: ' '.repeat(2 * 1024 * 1024 + 1),
        })
      ).status,
    ).toBe(413);
    expect(f.calls).toHaveLength(0);
    await f.server.stop();
    await expect(fetch(f.url)).rejects.toThrow();
  });
});
it('reports unexpected listener loss once, but never reports an explicit stop', async () => {
  let losses = 0;
  const server = new LocalMcpServer({
    unavailable: () => {
      losses++;
    },
    observed: async () => {},
    rejected: async () => {},
    authenticate: async () => {
      throw Error('unused');
    },
    invoke: async () => ({ kind: 'done' }),
  });
  servers.push(server);
  await server.start(0);
  const listener = (server as unknown as { server: import('node:http').Server }).server;
  listener.emit('error', Error('synthetic listener failure'));
  expect(losses).toBe(1);
  expect(server.port).toBe(0);
  await server.stop();
  expect(losses).toBe(1);
  await server.start(0);
  await server.stop();
  expect(losses).toBe(1);
});

it('advertises and validates existing-tab control over the real MCP HTTP transport', async () => {
  const f = await fixture();
  const client = new Client({ name: 'takeover-contract', version: '1' });
  clients.push(client);
  await client.connect(
    new StreamableHTTPClientTransport(new URL(f.url), {
      requestInit: { headers: { Authorization: `Bearer ${token}` } },
    }),
  );
  for (const command of [
    { action: 'list_tabs' },
    { action: 'takeover', tab_id: 'tab', request_key: 'request' },
    { action: 'read_tab', tab_id: 'tab', lease_id: 'lease', after: 0 },
    { action: 'write_tab', tab_id: 'tab', lease_id: 'lease', request_key: 'write', text: 'pwd\n' },
  ]) {
    const args = { action: 'workspace', session_id: 'session', command };
    expect((await client.callTool({ name: 'terminal_manage', arguments: args })).isError).not.toBe(
      true,
    );
    expect(f.calls.at(-1)).toEqual({ tool: 'terminal_manage', ...args });
    const before = f.calls.length;
    expect(
      (
        await client.callTool({
          name: 'terminal_manage',
          arguments: { ...args, command: { ...command, entry_id: 'forged' } },
        })
      ).isError,
    ).toBe(true);
    expect(f.calls).toHaveLength(before);
  }
});
