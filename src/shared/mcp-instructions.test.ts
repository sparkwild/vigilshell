import { afterEach, expect, it, vi } from 'vitest';
import electronConfig from '../../electron.vite.config';
import { mcpHttpConfiguration, mcpInstructions } from './mcp-instructions';

afterEach(() => vi.unstubAllEnvs());

it.each([
  { command: 'build', mode: 'production', name: 'vigilshell' },
  { command: 'serve', mode: 'development', name: 'vigilshell-dev' },
  { command: 'build', mode: 'development-package', name: 'vigilshell-dev' },
] as const)('exports a stable name for $command/$mode in Main and Renderer', async (channel) => {
  if (typeof electronConfig !== 'function') throw new Error('expected configuration factory');
  const config = await electronConfig(channel);
  const definitions = config.main!.define!;
  for (const key of ['VIGILSHELL_DEVELOPMENT_BUILD', 'VIGILSHELL_DEV_PACKAGE_BUILD']) {
    expect(config.renderer!.define![`import.meta.env.${key}`]).toBe(
      definitions[`process.env.${key}`],
    );
    expect(definitions[`import.meta.env.${key}`]).toBe(definitions[`process.env.${key}`]);
    vi.stubEnv(key, JSON.parse(definitions[`process.env.${key}`] as string));
  }
  const endpoint = 'http://127.0.0.1:51602/mcp';
  for (const token of ['first-entry-token', 'rotated-or-other-entry-token']) {
    const configuration = mcpHttpConfiguration(endpoint, token);
    expect(JSON.parse(configuration)).toEqual({
      mcpServers: {
        [channel.name]: {
          type: 'http',
          url: endpoint,
          headers: { Authorization: `Bearer ${token}` },
        },
      },
    });
    for (const locale of ['en', 'zh-CN'] as const)
      expect(mcpInstructions(locale, endpoint, token)).toContain(configuration);
  }
});
