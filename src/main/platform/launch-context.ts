import { readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';

interface LaunchContextOptions {
  appPath: string;
  resourcesPath: string;
  appDataPath: string;
  isPackaged: boolean;
  developmentBuild: boolean;
  developmentPackage?: boolean;
  environment: NodeJS.ProcessEnv;
}

/** 将编译通道、原生开发身份与资源位置分开；发行构建不接受开发页面入口。 */
export function resolveLaunchContext(options: LaunchContextOptions) {
  const { appPath, resourcesPath, appDataPath, isPackaged, developmentBuild, environment } =
    options;
  const developmentPackage = options.developmentPackage === true;
  const nativeDevelopment =
    !developmentPackage && developmentBuild && environment.VIGILSHELL_DEV_APP === '1';
  const projectRoot = resolve(appPath);
  if (nativeDevelopment) {
    const manifest = JSON.parse(readFileSync(join(resourcesPath, 'vigilshell-dev.json'), 'utf8'));
    if (
      manifest?.schemaVersion !== 1 ||
      manifest.projectRoot !== realpathSync(projectRoot) ||
      manifest.applicationName !== 'VigilShell Dev' ||
      manifest.bundleIdentifier !== 'com.sparkwild.vigilshell.electron.dev'
    )
      throw new Error('Invalid VigilShell development application; run npm run dev:app again');
  }
  const useWorkspace = !developmentPackage && (nativeDevelopment || !isPackaged);
  const developmentChannel = developmentPackage || useWorkspace;
  const applicationName = developmentChannel ? 'VigilShell Dev' : 'VigilShell';
  let developmentUrl: string | undefined;
  if (!developmentPackage && developmentBuild && environment.ELECTRON_RENDERER_URL) {
    const url = new URL(environment.ELECTRON_RENDERER_URL);
    if (
      url.protocol !== 'http:' ||
      !['localhost', '127.0.0.1', '[::1]'].includes(url.hostname) ||
      url.username ||
      url.password
    )
      throw new Error('Development renderer must use a local HTTP origin');
    developmentUrl = url.href;
  }
  const override = environment.VIGILSHELL_DATA_DIR;
  if (useWorkspace && override !== undefined && (!override.trim() || !isAbsolute(override)))
    throw new Error('VIGILSHELL_DATA_DIR must be a non-empty absolute path');
  const dataDirectory =
    useWorkspace && override !== undefined
      ? resolve(override)
      : join(appDataPath, developmentChannel ? 'VigilShell-dev' : 'VigilShell');
  const iconDirectory =
    nativeDevelopment || isPackaged
      ? join(resourcesPath, 'icons')
      : join(appPath, 'resources/icons');
  const backendName = 'vigilshell-backend' + (process.platform === 'win32' ? '.exe' : '');
  let developmentBackend = join(projectRoot, 'target/debug', backendName);
  if (useWorkspace && process.platform === 'darwin') {
    const nativeRoot = join(projectRoot, '.cache/vigilshell-dev/native');
    const active = JSON.parse(readFileSync(join(nativeRoot, 'active.json'), 'utf8'));
    const generation =
      nativeDevelopment && environment.VIGILSHELL_DEV_GENERATION
        ? environment.VIGILSHELL_DEV_GENERATION
        : active.generation;
    if (
      active.schemaVersion !== 1 ||
      typeof generation !== 'string' ||
      !/^[a-f0-9-]{36}$/.test(generation)
    )
      throw new Error('Invalid signed development runtime; run npm run dev:app');
    const directory = realpathSync(join(nativeRoot, generation));
    if (directory !== join(realpathSync(nativeRoot), generation))
      throw new Error('Invalid signed development runtime directory');
    const manifest = JSON.parse(readFileSync(join(directory, 'manifest.json'), 'utf8'));
    if (
      manifest.schemaVersion !== 1 ||
      manifest.generation !== generation ||
      manifest.profile !== 'dev' ||
      manifest.arch !== process.arch
    )
      throw new Error('Signed development runtime mismatch');
    developmentBackend = join(directory, 'VigilShell Dev.app/Contents/MacOS', backendName);
  }
  return {
    applicationName,
    mcpIdentityBridgePath: useWorkspace
      ? resolve(appPath, '.cache/mcp-native/authorization.node')
      : join(resourcesPath, 'native/mcp-identity.node'),
    notificationBridgePath: useWorkspace
      ? resolve(appPath, '.cache/notification-native/authorization.node')
      : join(resourcesPath, 'native/authorization.node'),
    notificationBundleId:
      applicationName === 'VigilShell Dev'
        ? 'com.sparkwild.vigilshell.electron.dev'
        : 'com.sparkwild.vigilshell.electron',
    nativeDevelopment,
    developmentUrl,
    dataDirectory,
    browserDirectory: join(dataDirectory, 'browser'),
    iconDirectory,
    backendExecutable: useWorkspace
      ? developmentBackend
      : process.platform === 'darwin'
        ? join(resourcesPath, '../Helpers', `${applicationName}.app/Contents/MacOS`, backendName)
        : join(resourcesPath, 'backend', backendName),
    verifyBackendManifest: !useWorkspace,
    developmentDockIcon:
      !isPackaged && !nativeDevelopment ? join(iconDirectory, 'app-icon-source.png') : undefined,
  };
}
