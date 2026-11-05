import { chmod, copyFile, mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

export const backendExecutable = 'vigilshell-backend';
export function backendProductName(profile) {
  if (!['dev', 'release'].includes(profile)) throw Error('Unknown Backend profile');
  return profile === 'dev' ? 'VigilShell Dev' : 'VigilShell';
}
export function backendAppName(profile) {
  return `${backendProductName(profile)}.app`;
}
export function backendBundleExecutable(root, profile) {
  return join(root, backendAppName(profile), 'Contents/MacOS', backendExecutable);
}

/** The actual Keychain caller carries the product name without changing its code identity. */
export async function createBackendBundle(root, binary, icon, profile) {
  const name = backendProductName(profile);
  const identifier = `com.sparkwild.vigilshell.electron${profile === 'dev' ? '.dev' : ''}.backend`;
  const app = join(root, backendAppName(profile));
  const contents = join(app, 'Contents');
  await mkdir(join(contents, 'MacOS'), { recursive: true });
  await mkdir(join(contents, 'Resources'), { recursive: true });
  await copyFile(binary, backendBundleExecutable(root, profile));
  await chmod(backendBundleExecutable(root, profile), 0o755);
  await copyFile(icon, join(contents, 'Resources/vigilshell.icns'));
  await writeFile(
    join(contents, 'Info.plist'),
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>${identifier}</string>
<key>CFBundleName</key><string>${name}</string>
<key>CFBundleDisplayName</key><string>${name}</string>
<key>CFBundleExecutable</key><string>${backendExecutable}</string>
<key>CFBundleIconFile</key><string>vigilshell.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>\n`,
  );
  return app;
}
