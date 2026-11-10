import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { load } from 'js-yaml';
import ts from 'typescript';
import { getNodeModuleFileMatcher, getFileMatchers } from 'app-builder-lib/out/fileMatcher.js';

const root = process.cwd();
const config = load(readFileSync('electron-builder.yml', 'utf8'));
const manifest = JSON.parse(readFileSync('package.json', 'utf8'));
const bundledRoots = JSON.parse(readFileSync('scripts/bundled-dependencies.json', 'utf8'));
const bundled = [...bundledRoots.packages, ...bundledRoots.stylesheets];

test('renderer dependencies retain license coverage after moving to devDependencies', () => {
  for (const name of bundled) assert(manifest.devDependencies[name], `Undeclared bundle: ${name}`);
  const covered = new Set([...Object.keys(manifest.dependencies), ...bundled]);
  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        if (!['__tests__', 'generated'].includes(entry.name)) visit(path);
        continue;
      }
      if (!/\.(tsx?|css)$/.test(path) || /\.(test|spec)\./.test(path)) continue;
      const source = readFileSync(path, 'utf8');
      const imports = path.endsWith('.css')
        ? [...source.matchAll(/@import\s+['"]([^'"]+)['"]/g)].map((match) => match[1])
        : ts.preProcessFile(source).importedFiles.map((item) => item.fileName);
      for (const specifier of imports) {
        if (/^(\.|\/|@\/|@contracts(?:\/|$)|node:)/.test(specifier)) continue;
        const name = specifier
          .split('/')
          .slice(0, specifier.startsWith('@') ? 2 : 1)
          .join('/');
        assert(covered.has(name), `${path}: missing bundled license root ${name}`);
      }
    }
  }
  visit('src/renderer');
});

test('packaging filters remove development files while retaining executable code and licenses', () => {
  const matcher = getNodeModuleFileMatcher(
    root,
    '/bundle',
    (value) => value,
    {},
    {
      config,
      debugLogger: { isEnabled: false },
    },
  );
  const filter = matcher.createFilter();
  const file = { isDirectory: () => false };
  for (const path of [
    'node_modules/zod/src/v4/classic/tests/string.test.ts',
    'node_modules/json-schema-traverse/spec/index.spec.js',
    'node_modules/ajv/dist/ajv.js.map',
    'node_modules/@modelcontextprotocol/core/dist/index.d.cts',
    'node_modules/@modelcontextprotocol/core/dist/index.d.mts',
    'node_modules/fast-uri/benchmark/index.js',
    'node_modules/ajv/.runkit_example.js',
  ])
    assert.equal(filter(resolve(path), file), false, path);
  for (const path of [
    'node_modules/zod/index.js',
    'node_modules/zod/v4/core/index.cjs',
    'node_modules/zod/src/index.ts',
    'node_modules/ajv/dist/ajv.js',
    'node_modules/@modelcontextprotocol/server/dist/index.js',
    'node_modules/@xterm/headless/lib-headless/xterm-headless.js',
    'node_modules/zod/LICENSE',
    'node_modules/sample-package/LICENSE.md',
    'node_modules/sample-package/NOTICE.txt',
  ])
    assert.equal(filter(resolve(path), file), true, path);
});

test('extra icon resources contain only images loaded by the main process', () => {
  const matchers = getFileMatchers(config, 'extraResources', '/bundle/Resources', {
    defaultSrc: root,
    globalOutDir: resolve('release'),
    customBuildOptions: {},
    macroExpander: (value) =>
      value.replace('${env.VIGILSHELL_BUILD_ROOT}', '.cache/build/dev/arm64'),
  });
  const icons = matchers.find((matcher) => matcher.from === resolve('resources/icons'));
  const filter = icons.createFilter();
  const included = readdirSync('resources/icons').filter((name) =>
    filter(resolve('resources/icons', name), { isDirectory: () => false }),
  );
  assert.deepEqual(included.sort(), ['app-icon-source.png', 'tray-icon.png']);
});
