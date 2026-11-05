import { access, stat, readdir, realpath } from 'node:fs/promises';
import { constants } from 'node:fs';
import { basename, delimiter, join } from 'node:path';
import { homedir } from 'node:os';
import type { AgentClientDetection } from '@contracts';

export interface DetectionPorts {
  application(bundleId: string, candidatePath?: string): string | undefined;
  home: string;
  path: string;
  file(path: string): Promise<'file' | 'directory' | null>;
  executable(path: string): Promise<boolean>;
  nodeVersions(): Promise<string[]>;
  realpath(path: string): Promise<string>;
}
interface ClientDefinition {
  client: AgentClientDetection['client'];
  bundles: [string, string][];
  commands: { name: string; targetName?: string }[];
  configs: [string, 'file' | 'directory'][];
}
const clients: ClientDefinition[] = [
  {
    client: 'codex',
    bundles: [['com.openai.codex', 'ChatGPT.app']],
    commands: [{ name: 'codex' }],
    configs: [['.codex/config.toml', 'file']],
  },
  {
    client: 'claudeCode',
    bundles: [['com.anthropic.claudefordesktop', 'Claude.app']],
    commands: [{ name: 'claude' }],
    configs: [['.claude.json', 'file']],
  },
  {
    client: 'cursor',
    bundles: [['com.todesktop.230313mzl4w4u92', 'Cursor.app']],
    commands: [
      { name: 'cursor' },
      { name: 'cursor-agent' },
      { name: 'agent', targetName: 'cursor-agent' },
    ],
    configs: [['.cursor/mcp.json', 'file']],
  },
  {
    client: 'dsh',
    bundles: [['com.deepseek.dsh', 'DeepSeek Harness.app']],
    commands: [{ name: 'dsh' }],
    configs: [['.dsh/cordis.patch.yml', 'file']],
  },
];
function missing(error: unknown) {
  return ['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '');
}
export function detectionPorts(application: DetectionPorts['application']): DetectionPorts {
  const home = homedir();
  return {
    application,
    realpath,
    home,
    path: process.env.PATH ?? '',
    file: async (path) => {
      try {
        const value = await stat(path);
        return value.isFile() ? 'file' : value.isDirectory() ? 'directory' : null;
      } catch (error) {
        if (missing(error)) return null;
        throw error;
      }
    },
    executable: async (path) => {
      try {
        if (!(await stat(path)).isFile()) return false;
        await access(path, constants.X_OK);
        return true;
      } catch (error) {
        if (missing(error)) return false;
        if ((error as NodeJS.ErrnoException).code === 'EACCES') {
          await access(path, constants.F_OK);
          return false;
        }
        throw error;
      }
    },
    nodeVersions: async () =>
      (
        await readdir(join(home, '.nvm/versions/node')).catch((error: unknown) => {
          if (missing(error)) return [];
          throw error;
        })
      )
        .slice(0, 64)
        .map((v) => join(home, '.nvm/versions/node', v, 'bin')),
  };
}
export async function detectClients(ports: DetectionPorts): Promise<AgentClientDetection[]> {
  let nodePathsUnavailable = false;
  const nodePaths = await ports.nodeVersions().catch(() => {
    nodePathsUnavailable = true;
    return [];
  });
  const paths = [
    ...new Set([
      ...ports.path.split(delimiter).filter((p) => p.startsWith('/')),
      join(ports.home, '.local/bin'),
      '/opt/homebrew/bin',
      '/usr/local/bin',
      ...nodePaths,
    ]),
  ];
  const check = async ({ client, bundles, commands, configs }: ClientDefinition) => {
    let installed = false,
      configurationFound = false,
      unavailable = false;
    for (const [id, name] of bundles) {
      // Launch Services 在受限进程中可能没有结果，标准安装位置仍需核对 bundle 身份。
      for (const candidate of [
        undefined,
        join('/Applications', name),
        join(ports.home, 'Applications', name),
      ]) {
        try {
          const path = ports.application(id, candidate);
          if (path && (await ports.file(path)) === 'directory') {
            installed = true;
            break;
          }
        } catch {
          unavailable = true;
        }
      }
    }
    for (const command of commands) {
      if (installed) break;
      for (const path of paths) {
        try {
          const candidate = join(path, command.name);
          if (!(await ports.executable(candidate))) continue;
          // 官方 agent 别名指向 cursor-agent；同名的其他工具不能算作 Cursor。
          if (
            command.targetName &&
            basename(await ports.realpath(candidate)) !== command.targetName
          )
            continue;
          installed = true;
          break;
        } catch (error) {
          if (!missing(error)) unavailable = true;
        }
      }
    }
    for (const [relative, kind] of configs) {
      try {
        configurationFound ||= (await ports.file(join(ports.home, relative))) === kind;
      } catch {
        unavailable = true;
      }
    }
    unavailable ||= !installed && commands.length > 0 && nodePathsUnavailable;
    return { client, installed, configurationFound, unavailable };
  };
  return Promise.all(clients.map(check));
}
