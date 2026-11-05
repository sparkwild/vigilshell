import { createRequire, isBuiltin } from 'node:module';
import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve, relative } from 'node:path';
const require = createRequire(resolve('package.json'));
const ts = require('typescript');
const root = resolve('src');
let count = 0;
const failures = [];
function walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'generated') walk(file);
      continue;
    }
    if (!/\.tsx?$/.test(file) || /\.test\./.test(file)) continue;
    const layer = relative(root, file).split('/')[0];
    const source = ts.createSourceFile(
      file,
      readFileSync(file, 'utf8'),
      ts.ScriptTarget.Latest,
      true,
    );
    const inspect = (node) => {
      let specifier;
      if (
        (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
        node.moduleSpecifier &&
        ts.isStringLiteral(node.moduleSpecifier)
      )
        specifier = node.moduleSpecifier.text;
      if (
        ts.isCallExpression(node) &&
        (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
          node.expression.getText(source) === 'require')
      ) {
        if (!node.arguments[0] || !ts.isStringLiteral(node.arguments[0]))
          failures.push(`${file}: dynamic module path`);
        else specifier = node.arguments[0].text;
      }
      if (specifier) {
        count++;
        const desktopOnly =
          specifier === 'electron' ||
          isBuiltin(specifier) ||
          /(?:^|\/)(main|preload)(?:\/|$)/.test(specifier);
        const rendererOnly =
          /^react(?:-dom)?(?:\/|$)/.test(specifier) ||
          /(?:^|\/)renderer(?:\/|$)/.test(specifier) ||
          specifier.startsWith('@/');
        if (
          (layer === 'renderer' && desktopOnly) ||
          (['main', 'preload'].includes(layer) && rendererOnly) ||
          (layer === 'shared' && (desktopOnly || rendererOnly))
        )
          failures.push(`${relative(root, file)} imports forbidden ${specifier}`);
      }
      ts.forEachChild(node, inspect);
    };
    inspect(source);
  }
}
walk(root);
if (failures.length) throw Error(failures.join('\n'));
console.log(`Import boundaries: ${count} static imports verified`);
