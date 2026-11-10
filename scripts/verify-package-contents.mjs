import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { extractFile, listPackage } from '@electron/asar';
import { packageBuildContext } from './build-context.mjs';

/** Check the final archive against the build outputs and license index before distribution. */
export function verifyPackageContents(app, profile) {
  const resources = join(app, 'Contents/Resources');
  const archive = join(resources, 'app.asar');
  const paths = listPackage(archive).map((path) => path.replace(/^\//, ''));
  const manifest = JSON.parse(extractFile(archive, 'package.json').toString());
  const bundledRoots = JSON.parse(readFileSync('scripts/bundled-dependencies.json', 'utf8'));
  const bundled = [...bundledRoots.packages, ...bundledRoots.stylesheets];
  const notices = JSON.parse(readFileSync(join(resources, 'licenses/index.json'), 'utf8'));
  const licensed = new Set(notices.map((entry) => `${entry.name}@${entry.version}`));
  for (const name of Object.keys(manifest.dependencies)) {
    assert(paths.includes(`node_modules/${name}/package.json`), `Missing runtime package: ${name}`);
  }
  for (const name of bundled) {
    assert(
      !paths.includes(`node_modules/${name}/package.json`),
      `Duplicate bundled package: ${name}`,
    );
    const source = JSON.parse(readFileSync(join('node_modules', name, 'package.json'), 'utf8'));
    assert(licensed.has(`${source.name}@${source.version}`), `Unlicensed bundle: ${name}`);
  }
  for (const path of paths) {
    assert(!path.endsWith('.map'), `Packaged sourcemap: ${path}`);
    if (!path.startsWith('node_modules/')) continue;
    assert(!/\.d\.(ts|mts|cts)$/.test(path), `Packaged type declaration: ${path}`);
    assert(
      !/\/(tests?|__tests__|spec|fixtures|__fixtures__|__snapshots__|benchmarks?|examples?)(\/|$)/.test(
        path,
      ),
      `Packaged development directory: ${path}`,
    );
    if (!path.endsWith('/package.json')) continue;
    const dependency = JSON.parse(extractFile(archive, path).toString());
    if (dependency.name && dependency.version)
      assert(
        licensed.has(`${dependency.name}@${dependency.version}`),
        `Unlicensed package: ${path}`,
      );
  }
  const output = packageBuildContext(profile).out;
  let count = 0;
  function verifyOutput(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) verifyOutput(path);
      else if (!path.endsWith('.map')) {
        const name = `out/${relative(output, path)}`;
        assert(
          extractFile(archive, name).equals(readFileSync(path)),
          `Changed build output: ${name}`,
        );
        count++;
      }
    }
  }
  verifyOutput(output);
  assert.equal(
    paths.filter(
      (path) => path.startsWith('out/') && !paths.some((child) => child.startsWith(`${path}/`)),
    ).length,
    count,
  );
  assert.deepEqual(readdirSync(join(resources, 'icons')).sort(), [
    'app-icon-source.png',
    'tray-icon.png',
  ]);
  for (const name of ['app-icon-source.png', 'tray-icon.png'])
    assert(
      readFileSync(join(resources, 'icons', name)).equals(
        readFileSync(join('resources/icons', name)),
      ),
    );
  console.log(
    `Verified ${count} unchanged build files, runtime dependencies, icons and license coverage`,
  );
}
