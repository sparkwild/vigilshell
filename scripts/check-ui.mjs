import { readdirSync, readFileSync } from 'node:fs';
import { resolve, relative, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const renderer = 'src/renderer/';
const ariaEntries = new Set([
  'components/ui/data-tree.tsx',
  'components/ui/data-table.tsx',
  'shared/UiProvider.tsx', // Only I18nProvider: one locale boundary for the whole app.
]);
const controls = new Set([
  'Button',
  'IconButton',
  'CloseButton',
  'ViewCloseButton',
  'Input',
  'Textarea',
  'SelectTrigger',
  'Switch',
  'Checkbox',
  'Badge',
  'Toggle',
  'ToggleGroup',
  'ToggleGroupItem',
  'TabsTrigger',
  'DialogContent',
  'AlertDialogContent',
  'PopoverContent',
  'TooltipContent',
  'SidebarMenuButton',
  'PopoverTitle',
  'Progress',
  'Meter',
  'Field',
  'FieldLabel',
  'FieldError',
  'SelectItem',
  'DropdownMenuItem',
  'ContextMenuItem',
  'DialogHeader',
  'DialogFooter',
  'Card',
  'CardContent',
  'ResizableHandle',
  'Table',
  'Tree',
  'TreeItem',
  'Column',
  'Row',
  'Cell',
  'TableHeader',
  'TableBody',
  'ColumnResizer',
]);
const nativeControls = new Set([
  'button',
  'input',
  'select',
  'textarea',
  'progress',
  'meter',
  'dialog',
  'label',
]);
const disclosures = new Set([
  'features/server-tools/TransferPanel.tsx', // Read-only transfer diagnostics.
  'features/terminal/ConnectionFailure.tsx', // Native disclosure for selectable diagnostics.
]);
const appearance =
  /(?:^|:)(?:bg-|text-(?!(?:left|right|center|start|end|justify|ellipsis|clip|wrap|nowrap|balance|pretty)$)|font-|rounded|shadow|ring-|outline-|border-(?![trblxyse](?:-|$))|dark:)/;
const visualProperty =
  /^(?:color|background(?:-.+)?|font(?:-.+)?|line-height|border(?:-.+)?|border-radius|box-shadow|outline(?:-.+)?|z-index)$/;

export function inspectUiSource(file, text) {
  const name = file.replaceAll('\\', '/').replace(renderer, '');
  const ui = name.startsWith('components/ui/');
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const problems = [],
    importedControls = new Set(),
    controlClasses = new Set();
  const report = (node, message) =>
    problems.push(
      `${name}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1}: ${message}`,
    );
  const modulePath = (node) => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    )
      return node.moduleSpecifier.text;
    if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        node.expression.getText(source) === 'require') &&
      node.arguments[0] &&
      ts.isStringLiteral(node.arguments[0])
    )
      return node.arguments[0].text;
    return null;
  };
  for (const node of source.statements) {
    if (!ts.isImportDeclaration(node) || !ts.isStringLiteral(node.moduleSpecifier)) continue;
    const bindings = node.importClause?.namedBindings;
    if (!bindings || !ts.isNamedImports(bindings)) continue;
    for (const specifier of bindings.elements) {
      const symbol = specifier.propertyName?.text ?? specifier.name.text;
      if (['Toast', 'ToastProvider', 'ToastViewport', 'useToastManager'].includes(symbol))
        report(specifier, 'Toast channels are removed; use local feedback');
      if (node.moduleSpecifier.text.includes('components/ui/') && controls.has(symbol))
        importedControls.add(specifier.name.text);
      if (!ui && symbol === 'TooltipProvider' && name !== 'shared/UiProvider.tsx')
        report(specifier, `${symbol} must be mounted by shared/UiProvider`);
      if (
        name === 'shared/UiProvider.tsx' &&
        node.moduleSpecifier.text === 'react-aria-components' &&
        symbol !== 'I18nProvider'
      )
        report(specifier, 'UiProvider may only import I18nProvider from React Aria');
    }
  }
  function visit(node) {
    const module = modulePath(node);
    if (module) {
      if (/(?:^|\/)(?:toast|sonner)(?:$|\/)/.test(module))
        report(node, 'Toast channels are removed; use local feedback');
      if (/^(?:@radix-ui\/|radix-ui$|sonner$)/.test(module))
        report(node, 'use the established Base UI component route');
      if (!ui && module.startsWith('@base-ui/'))
        report(node, 'use the project UI entry instead of Base UI directly');
      if (module.startsWith('react-aria-components') && !ariaEntries.has(name))
        report(node, 'use the data-tree/data-table adapter');
      if (ui && /(?:^|\/)(?:features|app|platform)(?:\/|$)|@vigilshell\/contracts/.test(module))
        report(node, 'UI primitives must not depend on application state or business contracts');
    }
    if (!ui && (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node))) {
      const tag = node.tagName.getText(source);
      if (nativeControls.has(tag)) report(node, `use the shared control instead of <${tag}>`);
      if ((tag === 'details' || tag === 'summary') && !disclosures.has(name))
        report(node, 'native disclosures require a documented component entry');
      for (const attr of node.attributes.properties) {
        if (!ts.isJsxAttribute(attr) || !attr.initializer) continue;
        const key = attr.name.getText(source);
        if (key === 'title' && (tag[0] === tag[0]?.toLowerCase() || importedControls.has(tag)))
          report(attr, 'use Tooltip/TruncatedText instead of native title');
        if (
          key === 'role' &&
          ts.isStringLiteral(attr.initializer) &&
          /^(button|dialog|alertdialog|progressbar|meter|combobox|switch|checkbox|tab)$/.test(
            attr.initializer.text,
          )
        )
          report(attr, 'use the shared accessible control instead of a hand-built role');
        if (key === 'className' && importedControls.has(tag)) {
          const literals = [];
          const collect = (value) => {
            if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value))
              literals.push(value.text);
            ts.forEachChild(value, collect);
          };
          collect(attr.initializer);
          for (const value of literals)
            for (const token of value.split(/\s+/)) {
              if (appearance.test(token)) report(attr, `move ${token} to a component variant`);
              if (/^[a-z][\w-]*$/.test(token)) controlClasses.add(token);
            }
        }
        if (key === 'style' && importedControls.has(tag)) {
          const collect = (value) => {
            if (
              ts.isPropertyAssignment(value) &&
              /^(color|background\w*|font\w*|border\w*|boxShadow|zIndex)$/.test(
                value.name.getText(source),
              )
            )
              report(value, 'move visual inline styles to a component variant');
            ts.forEachChild(value, collect);
          };
          collect(attr.initializer);
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  for (const diagnostic of source.parseDiagnostics ?? [])
    problems.push(`${name}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')}`);
  visit(source);
  return { problems, controlClasses };
}

export function inspectUiCss(file, text, controlClasses = new Set()) {
  if (file.replaceAll('\\', '/').includes('/components/ui/')) return [];
  const problems = [];
  const rules = text.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g);
  for (const [, rawSelector, declarations] of rules) {
    const selector = rawSelector.trim();
    const targetsControl =
      /\[data-slot=["']?(?:button|input|field|select|tabs|toggle|badge|popover|dialog|alert|switch|checkbox)/.test(
        selector,
      ) ||
      /(?:^|[ >+~,])(?:button|input|select|textarea)\b/.test(selector) ||
      [...controlClasses].some((name) => new RegExp(`\\.${name}(?![\\w-])`).test(selector));
    if (!targetsControl) continue;
    for (const declaration of declarations.split(';')) {
      const property = declaration.split(':')[0]?.trim();
      if (property && visualProperty.test(property))
        problems.push(`${file}: ${selector}: move ${property} to the owning UI component`);
    }
  }
  return problems;
}

export function checkUi(root = process.cwd()) {
  const files = [],
    problems = [],
    controlClasses = new Set();
  function walk(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const file = join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (/\.(?:tsx?|css)$/.test(file) && !/\.test\./.test(file)) files.push(file);
    }
  }
  walk(resolve(root, renderer));
  for (const file of files.filter((file) => !file.endsWith('.css'))) {
    const result = inspectUiSource(relative(root, file), readFileSync(file, 'utf8'));
    problems.push(...result.problems);
    result.controlClasses.forEach((name) => controlClasses.add(name));
  }
  for (const file of files.filter((file) => file.endsWith('.css')))
    problems.push(
      ...inspectUiCss(relative(root, file), readFileSync(file, 'utf8'), controlClasses),
    );
  if (problems.length) throw new Error(problems.join('\n'));
  console.log(`UI boundaries: ${files.length} source and style files verified`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) checkUi();
