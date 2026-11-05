import { spawnSync } from 'node:child_process';

// 沿用发起 npm script 的 npm 版本，不重新解析或改写锁文件。
const npmPath = process.env.npm_execpath;
const result = spawnSync(
  npmPath ? process.execPath : 'npm',
  [...(npmPath ? [npmPath] : []), 'ls', '--all', '--json'],
  { encoding: 'utf8', maxBuffer: 20 * 1024 * 1024 },
);
if (result.error) throw result.error;

let tree;
try {
  tree = JSON.parse(result.stdout);
} catch (error) {
  throw new Error('无法读取 npm 依赖树。', { cause: error });
}

if (result.status !== 0 || tree.problems?.length) {
  console.error((tree.problems ?? [tree.error?.summary ?? result.stderr]).join('\n'));
  process.exitCode = 1;
} else {
  console.log('依赖树与 peerDependencies 检查通过。');
}
