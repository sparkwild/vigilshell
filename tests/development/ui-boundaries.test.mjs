import { test } from 'node:test';
import assert from 'node:assert/strict';
import { inspectUiSource, inspectUiCss } from '../../scripts/check-ui.mjs';

const file = 'src/renderer/features/example/View.tsx';
test('blocks primitive imports, reexports, dynamic imports and native replacement controls', () => {
  for (const source of [
    'import { Dialog } from "@base-ui/react/dialog"',
    'export { Dialog } from "@base-ui/react/dialog"',
    'const lib = import("@base-ui/react/dialog")',
    'import { Button } from "react-aria-components"',
    'const element = <button>Save</button>',
    'const element = <div role="dialog" />',
  ])
    assert.ok(inspectUiSource(file, source).problems.length > 0, source);
});
test('detects aliases, competing providers and visual overrides while accepting layout', () => {
  assert.ok(
    inspectUiSource(
      file,
      'import { Button as Action } from "../../components/ui/button"; const el = <Action className="bg-red-500" />',
    ).problems.length,
  );
  assert.ok(
    inspectUiSource(
      file,
      'import { ToastProvider as NoticeScope } from "../../components/ui/toast"',
    ).problems.length,
  );
  assert.deepEqual(
    inspectUiSource(
      file,
      'import { Button } from "../../components/ui/button"; const el = <Button variant="destructive" className="ml-auto w-full" />',
    ).problems,
    [],
  );
  assert.ok(
    inspectUiSource(
      'src/renderer/components/ui/bad.tsx',
      'import { store } from "../../features/connections/workspace-store"',
    ).problems.length,
  );
});
test('limits specialized exceptions and detects CSS theme overrides through media queries', () => {
  assert.deepEqual(
    inspectUiSource(
      'src/renderer/components/ui/data-tree.tsx',
      'export { Tree } from "react-aria-components"',
    ).problems,
    [],
  );
  assert.ok(
    inspectUiSource(
      'src/renderer/shared/UiProvider.tsx',
      'import { Button } from "react-aria-components"',
    ).problems.length,
  );
  const css = '@media (min-width: 600px) { .editor [data-slot="input"] { color: red; } }';
  assert.ok(inspectUiCss('app.css', css).length);
  assert.ok(
    inspectUiCss(
      'app.css',
      '.custom-control:hover { background: red; }',
      new Set(['custom-control']),
    ).length,
  );
  assert.deepEqual(
    inspectUiCss('app.css', '.editor [data-slot="input"] { width: 12rem; margin-left: auto; }'),
    [],
  );
});
