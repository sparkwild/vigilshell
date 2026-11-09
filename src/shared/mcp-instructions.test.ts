import { afterEach, expect, it, vi } from 'vitest';
import electronConfig from '../../electron.vite.config';
import { mcpHttpConfiguration, mcpInstructions } from './mcp-instructions';
import { packageBuildContext } from '../../scripts/build-context.mjs';
import { join } from 'node:path';

afterEach(() => vi.unstubAllEnvs());

it.each([
  { command: 'build', mode: 'production', name: 'vigilshell' },
  { command: 'serve', mode: 'development', name: 'vigilshell-dev' },
  { command: 'build', mode: 'development-package', name: 'vigilshell-dev' },
] as const)('exports a stable name for $command/$mode in Main and Renderer', async (channel) => {
  vi.stubEnv(
    'VIGILSHELL_PACKAGE_PROFILE',
    channel.mode === 'development-package' ? 'dev' : 'release',
  );
  if (typeof electronConfig !== 'function') throw new Error('expected configuration factory');
  const config = await electronConfig(channel);
  if (channel.command === 'build') {
    const output = packageBuildContext(
      channel.mode === 'development-package' ? 'dev' : 'release',
    ).out;
    for (const part of ['main', 'preload', 'renderer'] as const)
      expect(config[part]!.build!.outDir).toBe(join(output, part));
  } else {
    for (const part of ['main', 'preload', 'renderer'] as const)
      expect(config[part]!.build!.outDir).toBeUndefined();
  }
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

it('rejects a packaged build whose mode disagrees with its signing profile', async () => {
  vi.stubEnv('VIGILSHELL_PACKAGE_PROFILE', 'release');
  if (typeof electronConfig !== 'function') throw new Error('expected configuration factory');
  expect(() => electronConfig({ command: 'build', mode: 'development-package' })).toThrow(
    /profile/,
  );
});
