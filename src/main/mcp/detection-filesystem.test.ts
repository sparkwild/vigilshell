// @vitest-environment node
import { mkdtemp, mkdir, writeFile, symlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it } from 'vitest';
import { detectClients, detectionPorts } from './detection';

it.each(['cursor-alias', 'other-alias', 'ordinary-agent', 'broken-alias', 'not-executable'])(
  'checks actual executable and alias identity for %s without launching it',
  async (kind) => {
    const home = await mkdtemp(join(tmpdir(), 'vigil-client-discovery-'));
    try {
      const bin = join(home, '.local/bin');
      await mkdir(bin, { recursive: true });
      const agent = join(bin, 'agent');
      if (kind === 'ordinary-agent') {
        await writeFile(agent, '#!/bin/sh\nexit 99\n', { mode: 0o755 });
      } else {
        const target = join(
          home,
          'versions',
          kind === 'other-alias' ? 'other-tool' : 'cursor-agent',
        );
        await mkdir(join(home, 'versions'), { recursive: true });
        if (kind !== 'broken-alias')
          await writeFile(target, '#!/bin/sh\nexit 99\n', {
            mode: kind === 'not-executable' ? 0o644 : 0o755,
          });
        await symlink(target, agent);
      }
      const ports = detectionPorts(() => undefined);
      const list = await detectClients({
        ...ports,
        home,
        path: bin,
        executable: (path) =>
          path.startsWith(`${home}/`) ? ports.executable(path) : Promise.resolve(false),
        nodeVersions: async () => [],
      });
      expect(list.find((c) => c.client === 'cursor')).toMatchObject({
        installed: kind === 'cursor-alias',
        unavailable: false,
      });
    } finally {
      await rm(home, { recursive: true, force: true });
    }
  },
);
