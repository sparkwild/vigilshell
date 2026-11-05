/** Explicit roles for the bundled Electron 44 code; discovery only detects missing
 * or unregistered code, never chooses an identity from an existing signature.
 * @typedef {{appId:string, productName:string, mainExecutable:string, backendExecutable:string, credentialsApp:string, credentialsExecutable:string, credentialsId:string, nativeRoot?:"credentials"|"backend"}} ComponentConfiguration
 */
const frameworks = /** @type {Record<string, {identifier:string, executable:string}>} */ ({
  'Electron Framework.framework': {
    identifier: 'com.github.Electron.framework',
    executable: 'Electron Framework',
  },
  'Mantle.framework': { identifier: 'com.electron.mantle', executable: 'Mantle' },
  'ReactiveObjC.framework': { identifier: 'com.electron.reactive', executable: 'ReactiveObjC' },
  'Squirrel.framework': { identifier: 'com.github.Squirrel', executable: 'Squirrel' },
});
const electronTools = /** @type {Record<string,string>} */ ({
  'Electron Framework.framework/Versions/A/Helpers/chrome_crashpad_handler': 'crashpad',
  'Electron Framework.framework/Versions/A/Libraries/libffmpeg.dylib': 'ffmpeg',
  'Electron Framework.framework/Versions/A/Libraries/libvk_swiftshader.dylib': 'swiftshader',
  'Electron Framework.framework/Versions/A/Libraries/libEGL.dylib': 'egl',
  'Electron Framework.framework/Versions/A/Libraries/libGLESv2.dylib': 'gles',
  'Squirrel.framework/Versions/A/Resources/ShipIt': 'shipit',
});
/** @param {string} path @param {ComponentConfiguration} config */
export function componentRole(path, config) {
  const native = { identifier: config.credentialsId, role: 'credentials', electron: false };
  const backend = { identifier: `${config.appId}.backend`, role: 'backend', electron: false };
  if (path === '' || path === '.' || path === `Contents/MacOS/${config.mainExecutable}`)
    return config.nativeRoot
      ? config.nativeRoot === 'backend'
        ? backend
        : native
      : { identifier: config.appId, role: 'application', electron: true };
  if (config.nativeRoot) throw Error(`Unregistered native signing component: ${path}`);
  const backendApp = `Contents/Helpers/${config.productName}.app`;
  if (path === backendApp || path === `${backendApp}/Contents/MacOS/${config.backendExecutable}`)
    return backend;
  const input = `Contents/Helpers/${config.credentialsApp}`;
  if (path === input || path === `${input}/Contents/MacOS/${config.credentialsExecutable}`)
    return native;
  if (path === 'Contents/Resources/native/mcp-identity.node')
    return { identifier: `${config.appId}.mcp-identity`, role: 'mcp-identity', electron: false };
  if (path === 'Contents/Resources/native/authorization.node')
    return {
      identifier: `${config.appId}.notifications`,
      role: 'notification-authorization',
      electron: false,
    };
  const prefix = 'Contents/Frameworks/';
  if (!path.startsWith(prefix)) throw Error(`Unregistered signing component: ${path}`);
  const part = path.slice(prefix.length);
  for (const [name, value] of Object.entries(frameworks)) {
    if (part === name || part === `${name}/Versions/A/${value.executable}`)
      return { identifier: value.identifier, role: 'electron-framework', electron: true };
  }
  if (Object.hasOwn(electronTools, part))
    return {
      identifier: `${config.appId}.electron.${electronTools[part]}`,
      role: 'electron-tool',
      electron: true,
    };
  for (const name of ['Electron', config.productName]) {
    for (const suffix of ['', ' (GPU)', ' (Plugin)', ' (Renderer)']) {
      const helper = `${name} Helper${suffix}`;
      if (part === `${helper}.app` || part === `${helper}.app/Contents/MacOS/${helper}`) {
        const role = suffix ? suffix.slice(2, -1) : 'default';
        return {
          identifier: `${config.appId}.helper.${role}`,
          role: `electron-helper-${role}`,
          electron: true,
        };
      }
    }
  }
  throw Error(`Unregistered signing component: ${path}`);
}
