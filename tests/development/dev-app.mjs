import assert from 'node:assert/strict';
import { readFile, writeFile, stat, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { devSession, root, bundle, alive, until, execute } from '../helpers/dev-session.mjs';
import { smoke } from '../e2e/dev-smoke.mjs';

const originals = new Map();
const checks = [];
function record(...items) {
  checks.push(...items);
  console.log(items.join('\n'));
}
async function probe(file, change) {
  const path = join(root, file);
  const current = await readFile(path, 'utf8');
  let entry = originals.get(file);
  if (!entry) {
    entry = { original: current, expected: current };
    originals.set(file, entry);
    const directory = join(session.evidence, 'probe-backups');
    await mkdir(directory, { recursive: true });
    await writeFile(join(directory, file.replaceAll('/', '_')), current);
  }
  assert.equal(current, entry.expected, `Concurrent source change: ${file}`);
  const next = change(entry.original);
  assert.notEqual(next, current, `Probe did not change ${file}`);
  await writeFile(path, next);
  entry.expected = next;
}
async function restore() {
  for (const [file, entry] of originals) {
    const path = join(root, file);
    assert.equal(await readFile(path, 'utf8'), entry.expected, `Concurrent source change: ${file}`);
    if (entry.expected !== entry.original) await writeFile(path, entry.original);
    entry.expected = entry.original;
  }
}
async function replaced(previous) {
  await until(
    () => !alive(previous.main) && !alive(previous.backend),
    'Old Main/Backend did not exit',
    120000,
  );
  await session.connect();
  const next = await session.identity();
  assert.notEqual(next.main, previous.main);
  assert.notEqual(next.backend, previous.backend);
  return next;
}
await assert.rejects(
  devSession({ lifecycle: true, startupTimeoutMs: 1 }),
  /Dev CDP did not become ready/,
);
await execute(process.execPath, ['scripts/dev-preflight.mjs', '--process-only'], { cwd: root });
record('startup timeout stops its owned launcher and preserves existing Dev business files');
const session = await devSession({ lifecycle: true });
let stopped = false;
try {
  record(...(await smoke(session)));
  let identity = await session.identity();
  const preferences = await session.page.evaluate(() =>
    window.vigilshell.getSnapshot().then((s) => s.preferences),
  );
  const stamp = join(root, 'node_modules/.cache/vigilshell-dev/stamp.json');
  const stampTime = (await stat(stamp)).mtimeMs;
  assert(
    (await session.visibleWindows(identity.main)) > 0,
    'Dev native window must be visible before hiding',
  );
  await session.page.evaluate(() => window.vigilshell.hideWindow());
  await until(
    async () => (await session.visibleWindows(identity.main)) === 0,
    'Window did not hide',
  );
  assert(alive(identity.main) && alive(identity.backend));
  await execute('open', ['-a', bundle]);
  await until(
    async () => (await session.visibleWindows(identity.main)) > 0,
    'Window did not restore',
  );
  assert.deepEqual(await session.identity(), identity);
  record('hide/activate keeps the same Main and Backend');
  await session.page.evaluate(() => {
    window.devAcceptanceMarker = 'retained';
  });
  await probe('src/renderer/app/Workbench.tsx', (source) => {
    assert.equal(source.split('data-testid="workbench"').length, 2);
    return source.replace(
      'data-testid="workbench"',
      'data-testid="workbench" data-dev-hmr-probe="react-updated"',
    );
  });
  await session.page.locator('[data-dev-hmr-probe="react-updated"]').waitFor();
  assert.equal(await session.page.evaluate(() => window.devAcceptanceMarker), 'retained');
  assert.deepEqual(await session.identity(), identity);
  await probe(
    'src/renderer/styles/index.css',
    (source) => source + '\n:root { --dev-acceptance-probe: live; }\n',
  );
  await until(
    () =>
      session.page.evaluate(
        () =>
          getComputedStyle(document.documentElement)
            .getPropertyValue('--dev-acceptance-probe')
            .trim() === 'live',
      ),
    'CSS HMR missing',
  );
  assert.equal(await session.page.evaluate(() => window.devAcceptanceMarker), 'retained');
  assert.deepEqual(await session.identity(), identity);
  record('React/CSS HMR preserves Renderer, Main and Backend');
  await probe(
    'src/preload/index.ts',
    (source) =>
      source +
      '\nwindow.addEventListener("DOMContentLoaded", () => { document.documentElement.dataset.devPreloadProbe = "reloaded"; });\n',
  );
  await until(
    () =>
      session.page
        .locator('html')
        .getAttribute('data-dev-preload-probe')
        .then((v) => v === 'reloaded'),
    'Preload did not reload Renderer',
  );
  assert.equal(await session.page.evaluate(() => window.devAcceptanceMarker), undefined);
  assert.deepEqual(await session.identity(), identity);
  record('Preload reloads Renderer without replacing Main/Backend');
  await probe(
    'src/main/index.ts',
    (source) => source + '\nconsole.info("native-dev-main-probe");\n',
  );
  identity = await replaced(identity);
  assert(session.output.includes('native-dev-main-probe'));
  record('Main rebuild replaces Main/Backend through the normal supervisor');
  await probe(
    'crates/vigilshell-backend/src/main.rs',
    (source) => source + '\n// Native development acceptance probe\n',
  );
  identity = await replaced(identity);
  const activePath = join(root, '.cache/vigilshell-dev/native/active.json');
  const active = await readFile(activePath, 'utf8');
  await probe(
    'crates/vigilshell-backend/src/main.rs',
    (source) => source + '\ncompile_error!("native-dev-build-failure-probe");\n',
  );
  await until(
    () => session.output.includes('Rust build failed; keeping'),
    'Missing Rust failure report',
    120000,
  );
  assert.deepEqual(await session.identity(), identity);
  assert.equal(await readFile(activePath, 'utf8'), active);
  assert.equal(
    (await session.page.evaluate(() => window.vigilshell.getSnapshot())).runtime.status,
    'ready',
  );
  await probe('crates/vigilshell-backend/src/main.rs', (source) => source);
  identity = await replaced(identity);
  assert.equal((await stat(stamp)).mtimeMs, stampTime);
  assert.deepEqual(
    (await session.page.evaluate(() => window.vigilshell.getSnapshot())).preferences,
    preferences,
  );
  record(
    'Rust rebuild publishes a verified generation; failed build keeps active generation and live processes; repair recovers',
  );
  await session.quit();
  stopped = true;
  await restore();
  await session.start();
  stopped = false;
  const owner = await session.identity();
  const attached = await devSession({ cdpUrl: session.endpoint });
  assert.notEqual(attached.evidence, session.evidence);
  await attached.close();
  assert(alive(owner.main) && alive(owner.backend));
  assert.deepEqual(await session.identity(), owner);
  record('attaching and disconnecting CDP leaves the original Dev processes running');
  assert.deepEqual(
    (await session.page.evaluate(() => window.vigilshell.getSnapshot())).preferences,
    preferences,
  );
  assert.equal((await stat(stamp)).mtimeMs, stampTime);
  await session.quit();
  stopped = true;
  record(
    'normal quit stops launcher; restart retains existing preferences and reuses signed shell',
  );
} finally {
  try {
    if (!stopped) {
      await session.quit();
      stopped = true;
    }
  } finally {
    try {
      await restore();
    } finally {
      await session.close();
    }
  }
}
await writeFile(
  join(session.evidence, 'lifecycle.json'),
  JSON.stringify({ passed: true, checks }, null, 2),
);
console.log(JSON.stringify({ passed: true, checks, evidence: session.evidence }));
