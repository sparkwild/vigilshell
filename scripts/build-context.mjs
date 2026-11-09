import { join, relative, resolve } from 'node:path';

/** Packaged Dev and release builds never overwrite the live development runtime. */
export function packageBuildContext(profile, arch = process.arch, root = process.cwd()) {
  if (!['dev', 'release'].includes(profile)) throw Error('Unknown package profile');
  if (!['arm64', 'x64'].includes(arch)) throw Error('Unsupported package architecture');
  const directory = resolve(root, '.cache/build', profile, arch);
  return {
    profile,
    directory,
    out: join(directory, 'out'),
    native: join(directory, 'native'),
    backend: join(directory, 'backend'),
    cargo: join(directory, 'cargo'),
    mode: profile === 'dev' ? 'development-package' : 'production',
  };
}

export function packageBuildEnvironment(profile, environment = process.env) {
  const context = packageBuildContext(profile);
  const result = { ...environment };
  for (const name of [
    'ELECTRON_RUN_AS_NODE',
    'ELECTRON_RENDERER_URL',
    'ELECTRON_EXEC_PATH',
    'VIGILSHELL_DEV_APP',
    'VIGILSHELL_DEV_EXECUTABLE',
    'VIGILSHELL_DEV_GENERATION',
    'VIGILSHELL_DATA_DIR',
    'VIGILSHELL_DEV_PACKAGE_BUILD',
    'VIGILSHELL_DEVELOPMENT_BUILD',
  ])
    delete result[name];
  return {
    ...result,
    VIGILSHELL_PACKAGE_PROFILE: profile,
    VIGILSHELL_BUILD_ROOT: relative(process.cwd(), context.directory),
    CARGO_TARGET_DIR: context.cargo,
  };
}

export function nativeBuildContext(environment = process.env, root = process.cwd()) {
  const profile = environment.VIGILSHELL_PACKAGE_PROFILE;
  if (profile === undefined) return { profile: 'dev', native: resolve(root, '.cache') };
  return packageBuildContext(profile, process.arch, root);
}
