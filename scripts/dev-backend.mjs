import { nativeFingerprint } from './dev-runtime.mjs';
import { createBackendBundle, backendAppName, backendBundleExecutable } from './backend-bundle.mjs';
import { randomUUID } from 'node:crypto';
import { cp, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  identifiers,
  inputAppName,
  inputExecutable,
  loadSigningConfig,
  preflight,
  projectRoot,
  run,
  signBundle,
  verifyCode,
} from './macos-signing.mjs';

export const runtimeRoot = join(projectRoot, '.cache/vigilshell-dev/native');

/** 指针只引用已经完整签名的一组文件；失败时不会替换当前可用版本。 */
export async function publishBackend(root, generation, verify) {
  if (!/^[a-f0-9-]{36}$/.test(generation)) throw new Error('Invalid native generation');
  await verify(join(root, generation));
  const temporary = join(root, `${randomUUID()}.json.tmp`);
  try {
    await writeFile(temporary, JSON.stringify({ schemaVersion: 1, generation }) + '\n', {
      flag: 'wx',
    });
    await rename(temporary, join(root, 'active.json'));
  } finally {
    await rm(temporary, { force: true });
  }
}

export async function stageBackend() {
  const config = loadSigningConfig('dev');
  preflight(config);
  const generation = randomUUID();
  const path = join(runtimeRoot, generation);
  await mkdir(path, { recursive: true });
  try {
    const backend = await createBackendBundle(
      path,
      join(projectRoot, 'target/debug/vigilshell-backend'),
      join(projectRoot, 'resources/icons/icon.icns'),
      'dev',
    );
    await cp(join(projectRoot, 'target/debug', inputAppName), join(path, inputAppName), {
      recursive: true,
    });
    run('/usr/bin/plutil', [
      '-replace',
      'CFBundleIdentifier',
      '-string',
      identifiers('dev').input,
      join(path, inputAppName, 'Contents/Info.plist'),
    ]);
    await signBundle(backend, config, process.arch);
    await signBundle(join(path, inputAppName), config, process.arch);
    verifyCode(
      backendBundleExecutable(path, 'dev'),
      config,
      process.arch,
      identifiers('dev').backend,
    );
    await writeFile(
      join(path, 'manifest.json'),
      JSON.stringify({
        schemaVersion: 1,
        profile: 'dev',
        arch: process.arch,
        identity: config.identity,
        generation,
      }) + '\n',
    );
    await writeFile(join(runtimeRoot, 'candidate.json'), JSON.stringify({ generation }) + '\n');
    return generation;
  } catch (error) {
    await rm(path, { recursive: true, force: true });
    throw error;
  }
}

export async function publishCandidate() {
  const { generation } = JSON.parse(await readFile(join(runtimeRoot, 'candidate.json'), 'utf8'));
  const config = loadSigningConfig('dev');
  const active = JSON.parse(
    await readFile(join(runtimeRoot, 'active.json'), 'utf8').catch(() => '{}'),
  );
  if (
    active.generation &&
    active.generation !== generation &&
    (await nativeFingerprint(join(runtimeRoot, active.generation)).catch(() => null)) ===
      (await nativeFingerprint(join(runtimeRoot, generation)))
  ) {
    verifyCode(
      join(runtimeRoot, generation, backendAppName('dev')),
      config,
      process.arch,
      identifiers('dev').backend,
    );
    await writeFile(
      join(runtimeRoot, 'candidate.json'),
      JSON.stringify({ generation: active.generation }) + '\n',
    );
    await rm(join(runtimeRoot, generation), { recursive: true, force: true });
    return active.generation;
  }
  await publishBackend(runtimeRoot, generation, async (path) => {
    const manifest = JSON.parse(await readFile(join(path, 'manifest.json'), 'utf8'));
    if (manifest.identity !== config.identity || manifest.arch !== process.arch)
      throw new Error('Candidate signing configuration changed');
    verifyCode(join(path, backendAppName('dev')), config, process.arch, identifiers('dev').backend);
    verifyCode(
      backendBundleExecutable(path, 'dev'),
      config,
      process.arch,
      identifiers('dev').backend,
    );
    verifyCode(join(path, inputAppName), config, process.arch, identifiers('dev').input);
    verifyCode(
      join(path, inputAppName, 'Contents/MacOS', inputExecutable),
      config,
      process.arch,
      identifiers('dev').input,
    );
  });
  return generation;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv.includes('--publish')) await stageBackend();
  if (!process.argv.includes('--stage-only')) await publishCandidate();
}
