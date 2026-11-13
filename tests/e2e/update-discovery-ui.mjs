import assert from 'node:assert/strict';
import { join } from 'node:path';
export async function verifyDiscoveryUi(application, page, evidence, result) {
  const original = await page.evaluate(async () => {
    const { preferences: p } = await window.vigilshell.getSnapshot();
    return {
      autoCheckUpdates: p.autoCheckUpdates,
      localeMode: p.localeMode,
      themeMode: p.themeMode,
    };
  });
  const bounds = await application.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].getBounds(),
  );
  const change = (patch) =>
    page.evaluate(async (patch) => {
      const { preferences: p } = await window.vigilshell.getSnapshot();
      const base = Object.fromEntries(Object.keys(patch).map((key) => [key, p[key]]));
      await window.vigilshell.updatePreferences({ patch, base, expectedRevision: p.revision });
    }, patch);
  try {
    await change({ autoCheckUpdates: true });
    assert.equal(await page.getByTestId('open-update').count(), 0);
    await page.getByTestId('open-update').waitFor({ timeout: 60000 });
    assert.equal(await page.getByRole('dialog').count(), 0);
    result.automaticDiscoveryNonBlocking = true;
    for (const locale of ['en', 'zh-CN']) {
      for (const theme of ['light', 'dark']) {
        await change({ localeMode: locale, themeMode: theme });
        await page.waitForFunction(
          ({ locale, theme }) =>
            document.documentElement.lang === locale &&
            document.documentElement.classList.contains('dark') === (theme === 'dark'),
          { locale, theme },
        );
        for (const width of [960, 1280]) {
          await application.evaluate(
            ({ BrowserWindow }, width) =>
              BrowserWindow.getAllWindows()[0].setBounds({
                width,
                height: width === 960 ? 640 : 820,
              }),
            width,
          );
          const handle = page.locator('.shell-resize-handle').first();
          const rect = await handle.boundingBox();
          await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
          await page.mouse.down();
          await page.mouse.move(240, rect.y + rect.height / 2, { steps: 10 });
          await page.mouse.up();
          const button = await page.getByTestId('open-update').boundingBox();
          const settings = await page.getByTestId('open-settings').boundingBox();
          const sidebar = await page.locator('#workbench-navigation').boundingBox();
          assert.equal(button.width, 32);
          assert.equal(button.height, 32);
          assert(
            settings.x + settings.width <= button.x,
            'Settings and update click regions overlap',
          );
          assert(
            button.x + button.width <= sidebar.x + sidebar.width,
            'Update button exceeds sidebar',
          );
          assert(
            await page
              .getByTestId('open-settings')
              .evaluate((node) => node.scrollWidth <= node.clientWidth),
            'Settings label clips',
          );
          await page
            .locator('#workbench-navigation [data-slot="sidebar-footer"]')
            .screenshot({ path: join(evidence, `update-footer-${locale}-${theme}-${width}.png`) });
        }
        await page.getByTestId('open-update').click();
        await page.getByRole('dialog').waitFor();
        assert(
          await page.getByTestId('open-settings').isVisible(),
          'Update entry navigated away from workspace',
        );
        await page
          .getByRole('dialog')
          .screenshot({ path: join(evidence, `update-dialog-${locale}-${theme}.png`) });
        await page.keyboard.press('Escape');
        await page.getByRole('dialog').waitFor({ state: 'hidden' });
        assert(
          await page.getByTestId('open-update').evaluate((node) => node === document.activeElement),
        );
        await page.getByTestId('open-settings').click();
        const about = page.locator('.settings-sidebar [data-slot="sidebar-menu-button"]').last();
        await about.click();
        const badge = about.locator('[data-slot="badge"]');
        assert.equal(await badge.getAttribute('data-variant'), 'update');
        await about.screenshot({ path: join(evidence, `update-badge-${locale}-${theme}.png`) });
        await page.locator('.settings-sidebar-header button').click();
      }
    }
    result.responsiveUpdateEntry = true;
    result.sharedDialogPreservesWorkspace = true;
    result.localizedThemePreviews = true;
  } finally {
    if (await page.getByRole('dialog').count()) await page.keyboard.press('Escape');
    await change(original);
    await application.evaluate(
      ({ BrowserWindow }, bounds) => BrowserWindow.getAllWindows()[0].setBounds(bounds),
      bounds,
    );
  }
}
