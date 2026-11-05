import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { devSession, until } from '../helpers/dev-session.mjs';

export async function smoke(session) {
  const page = session.page;
  const snapshot = await page.evaluate(() => window.vigilshell.getSnapshot());
  const labels = JSON.parse(await readFile(`messages/${snapshot.appearance.locale}.json`, 'utf8'));
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  assert.equal(await page.evaluate(() => typeof window.require), 'undefined');
  const api = (name, params) =>
    page.evaluate(
      async ({ name, params }) => {
        const result = await window.vigilshell.workspace[name](params);
        if (!result.ok) throw Error(result.code);
        return result.value;
      },
      { name, params },
    );
  const before = await api('listCatalogue', {
    kind: 'connections',
    query: '',
    offset: 0,
    version: null,
  });
  assert.equal(
    await page.locator('[data-size="editor"]').count(),
    0,
    'An existing editor must be left untouched',
  );
  await page.getByRole('button', { name: labels.ws_new_connection, exact: true }).first().click();
  const editor = page.locator('[data-size="editor"]');
  await editor.waitFor();
  assert.equal(await editor.getByRole('tab').count(), 4);
  await page.locator('#connection-name').fill('Unsaved Dev acceptance');
  await page.locator('#connection-host').fill('127.0.0.1');
  await page.locator('#connection-user').fill('fixture');
  await page.locator('#connection-notes').fill('中文草稿\n不保存');
  for (let i = 0; i < 4; i++) {
    await editor.getByRole('tab').nth(i).click();
    const fits = await editor.evaluate((element) => {
      const box = element.getBoundingClientRect();
      const footer = element.querySelector('[data-slot="dialog-footer"]').getBoundingClientRect();
      return (
        box.x >= 0 &&
        box.y >= 0 &&
        box.right <= innerWidth + 1 &&
        footer.bottom <= innerHeight + 1 &&
        element.scrollWidth <= element.clientWidth + 1
      );
    });
    assert(fits, `Editor section ${i} must fit the real Dev viewport`);
  }
  await editor.getByRole('tab').nth(2).click();
  await editor.getByRole('button', { name: labels.ws_new_proxy, exact: true }).click();
  const child = page.locator('[data-size="form"]');
  await child.waitFor();
  await child.getByRole('button', { name: labels.ws_cancel, exact: true }).click();
  await child.waitFor({ state: 'detached' });
  await editor.getByRole('tab').nth(3).click();
  await editor.getByRole('button', { name: labels.ws_new_tunnel, exact: true }).click();
  await child.waitFor();
  await page.locator('#tunnel-kind').click();
  await page.getByRole('option', { name: labels.ws_tunnel_dynamic, exact: true }).click();
  assert.equal(await page.locator('#tunnel-targetHost').count(), 0);
  assert.equal(await page.locator('#tunnel-targetPort').count(), 0);
  await child.getByRole('button', { name: labels.ws_cancel, exact: true }).click();
  await page
    .getByRole('alertdialog')
    .getByRole('button', { name: labels.ws_confirm, exact: true })
    .click();
  await child.waitFor({ state: 'detached' });
  await editor.getByRole('tab').first().click();
  assert.equal(await page.locator('#connection-notes').inputValue(), '中文草稿\n不保存');
  await page.screenshot({ path: join(session.evidence, 'unsaved-editor.png') });
  await editor.getByRole('button', { name: labels.ws_close, exact: true }).first().click();
  const discard = page.getByRole('alertdialog');
  await discard.waitFor();
  await discard.getByRole('button', { name: labels.ws_confirm, exact: true }).click();
  await editor.waitFor({ state: 'detached' });
  const after = await api('listCatalogue', {
    kind: 'connections',
    query: '',
    offset: 0,
    version: null,
  });
  assert.deepEqual(after, before);
  const navigation = page.locator('#workbench-navigation');
  const search = navigation.getByRole('textbox').first();
  const originalSearch = await search.inputValue();
  await search.fill('no-matching-dev-smoke-connection');
  await navigation.getByRole('tab', { name: labels.nav_groups, exact: true }).click();
  assert.equal(await navigation.getByRole('textbox').count(), 0);
  await navigation.getByRole('tab', { name: labels.nav_connections, exact: true }).click();
  assert.equal(await search.inputValue(), 'no-matching-dev-smoke-connection');
  await search.fill(originalSearch);
  const toggle = page.getByTestId('toggle-left');
  await toggle.click();
  await session.move(700, 300);
  await session.hover(toggle);
  const preview = page.locator('#workbench-navigation-preview');
  await preview.waitFor();
  await preview.getByRole('textbox').focus();
  await page.keyboard.press('Escape');
  await preview.waitFor({ state: 'detached' });
  await session.move(700, 300);
  await session.hover(toggle);
  await preview.waitFor();
  await toggle.click();
  await preview.waitFor({ state: 'detached' });
  await until(
    async () =>
      (await page
        .locator('#sidebar')
        .evaluate((element) => element.getBoundingClientRect().width)) >= 239.5,
    'Pinned sidebar lost its minimum width',
  );
  assert.deepEqual(
    (await page.evaluate(() => window.vigilshell.getSnapshot())).preferences,
    snapshot.preferences,
  );
  assert.deepEqual(errors, []);
  return [
    'native Dev identity and signed Backend',
    'renderer isolation',
    'four editor sections and draft preservation',
    'proxy cancellation and conditional SOCKS5 fields without saving',
    'real viewport/footer fit',
    'group view retains connection search; preview Escape/reopen/pin',
    'discard leaves catalogue and preferences unchanged',
  ];
}

if (import.meta.url === new URL(process.argv[1], 'file:').href) {
  const session = await devSession();
  let checks;
  try {
    checks = await smoke(session);
  } finally {
    await session.close();
  }
  await writeFile(join(session.evidence, 'smoke.json'), JSON.stringify({ passed: true, checks }));
  console.log(JSON.stringify({ passed: true, checks }));
}
