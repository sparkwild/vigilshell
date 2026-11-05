import { desktopProtocolVersion, workspaceProtocolVersion } from './protocol-versions.mjs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { compile } from 'json-schema-to-typescript';
import Ajv from 'ajv';
import standaloneCode from 'ajv/dist/standalone/index.js';
import { _ } from 'ajv/dist/compile/codegen/index.js';
import ucs2length from 'ajv/dist/runtime/ucs2length.js';

const schema = JSON.parse(
  execFileSync(
    'cargo',
    ['run', '--locked', '--quiet', '-p', 'vigilshell-desktop-protocol', '--example', 'export'],
    { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 },
  ),
);
// Renderer 校验器在构建时编译，保持桌面 CSP 禁止运行时字符串执行。
const ajv = new Ajv({
  allErrors: true,
  strict: false,
  validateFormats: false,
  code: { source: true, esm: true, lines: true },
});
ajv.addKeyword({
  keyword: 'x-vigil-rule',
  schemaType: 'string',
  code(context) {
    context.fail(_`!semanticRule(${context.schema}, ${context.data})`);
  },
});
ajv.addSchema(schema, 'contract');
const rendererValidator = standaloneCode(ajv, {
  validateConnectionPolicy: 'contract#/definitions/ConnectionPolicy',
}).replaceAll(
  'require("ajv/dist/runtime/ucs2length").default',
  `(${ucs2length.default.toString()})`,
);
if (/\brequire\(|\bnew Function\(|\beval\(/.test(rendererValidator))
  throw Error('Renderer validator must be standalone ESM without runtime code generation');
const files = {
  'protocol-versions.ts': `/* Generated from Rust protocol constants. */\nexport const desktopProtocolVersion = ${desktopProtocolVersion} as const;\nexport const workspaceProtocolVersion = ${workspaceProtocolVersion} as const;\n`,
  'renderer-validation.js':
    '/* Generated from Rust DTOs. Run npm run contracts:generate; do not edit. */\n' +
    (rendererValidator.includes('semanticRule(')
      ? "import { semanticRule } from '../semantic-validation';\n"
      : '') +
    rendererValidator,
  'renderer-validation.d.ts':
    '/* Generated from Rust DTOs. Run npm run contracts:generate; do not edit. */\n' +
    "import type { ValidateFunction } from 'ajv';\n" +
    "import type { ConnectionPolicy } from './runtime';\n" +
    'export const validateConnectionPolicy: ValidateFunction<ConnectionPolicy>;\n',
  'runtime.schema.json': JSON.stringify(schema, null, 2) + '\n',
  'runtime.ts': await compile(schema, 'DesktopContract', {
    bannerComment: '/* Generated from Rust DTOs. Run npm run contracts:generate; do not edit. */',
    unreachableDefinitions: true,
    ignoreMinAndMaxItems: true,
    additionalProperties: false,
    style: { semi: false, singleQuote: false, printWidth: 80 },
  }),
};
const directory = 'src/contracts/generated';
const index = "export type * from './generated/runtime';\n";
const check = process.argv.includes('--check');
function update(path, content) {
  const previous = existsSync(path) ? readFileSync(path, 'utf8') : '';
  if (previous === content) return;
  if (check) throw Error(`${path}: generated contracts differ; run npm run contracts:generate`);
  writeFileSync(path, content);
}
if (!check) mkdirSync(directory, { recursive: true });
for (const name of readdirSync(directory)) {
  if (Object.hasOwn(files, name)) continue;
  if (check) throw Error(`Obsolete generated contract: ${name}`);
  rmSync(join(directory, name));
}
for (const [name, content] of Object.entries(files)) update(join(directory, name), content);
update('src/contracts/index.ts', index);
console.log('Rust DTO → JSON Schema → TypeScript and CSP-safe Renderer validators verified');
