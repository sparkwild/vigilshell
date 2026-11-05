import { execFileSync } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { mkdir, rename, rm } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { signTool, loadSigningConfig, identifiers } from './macos-signing.mjs';

// Node-API 使用稳定 ABI；构建头文件来自当前受支持的 Node 安装。
if (process.platform === 'darwin') {
  const root = resolve('.');
  const include = resolve(dirname(realpathSync(process.execPath)), '../include/node');
  if (!existsSync(join(include, 'node_api.h')))
    throw new Error('The Node.js installation must include include/node/node_api.h');
  for (const component of [
    {
      directory: 'notification-native',
      source: 'src/main/notifications/native/authorization.mm',
      framework: 'UserNotifications',
      role: 'notifications',
    },
    {
      directory: 'mcp-native',
      source: 'src/main/mcp/native/identity.mm',
      framework: 'LocalAuthentication',
      role: 'mcp-identity',
    },
  ]) {
    const directory = join(root, '.cache', component.directory);
    await mkdir(directory, { recursive: true });
    const temporary = join(directory, `authorization-${process.pid}.node`);
    try {
      execFileSync(
        'xcrun',
        [
          'clang++',
          '-std=c++20',
          '-arch',
          process.arch === 'arm64' ? 'arm64' : 'x86_64',
          '-fobjc-arc',
          '-fblocks',
          '-bundle',
          '-undefined',
          'dynamic_lookup',
          '-mmacosx-version-min=13.0',
          '-DNAPI_VERSION=8',
          '-DNODE_GYP_MODULE_NAME=notification_authorization',
          '-I',
          include,
          '-framework',
          'Foundation',
          '-framework',
          component.framework,
          ...(component.role === 'mcp-identity' ? ['-framework', 'AppKit'] : []),
          join(root, component.source),
          '-o',
          temporary,
        ],
        { stdio: 'inherit' },
      );
      signTool(temporary, loadSigningConfig('dev'), `${identifiers('dev').app}.${component.role}`);
      await rename(temporary, join(directory, 'authorization.node'));
    } finally {
      await rm(temporary, { force: true });
    }
  }
}
