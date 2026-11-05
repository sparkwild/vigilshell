import { loadSigningConfig, preflight, signBundle } from './macos-signing.mjs';

/** electron-builder 26 在选择身份后调用此入口；使用指纹避免同名身份歧义。 */
export default async function signPackage(options, packager) {
  const config = loadSigningConfig(process.env.VIGILSHELL_PACKAGE_PROFILE ?? 'release');
  preflight(config);
  const arch = process.arch;
  if (!['arm64', 'x64'].includes(arch)) throw new Error('Unsupported release architecture');
  if (packager.platformSpecificBuildOptions.identity !== config.identity)
    throw new Error('Release identity must be selected by the package entrypoint');
  await signBundle(options.app, config, arch);
}
