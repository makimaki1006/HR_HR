import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/** 枠の拡張機能の目印 (<html data-hrhr-frames>) が後から付いたら、リンクの操作行に「拡張機能: 有効」が出る。無いときは何も出ない */
test('拡張機能の属性を付けるとリンクの操作行にバッジが出る', async ({ page }) => {
  await login(page);
  await page.route(/^https:\/\/www\.example\.com\//, route =>
    route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>stub</title><p>stub</p>' }));
  await page.goto('/app/crm?mode=fixture');
  await page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row button.cq-row-button').first().click();
  await page.getByRole('region', { name: 'リンク' }).getByRole('link').filter({ hasNotText: /求人を検索する/ }).first().click();

  const bar = page.locator('.cq-linkbar').first();
  await expect(bar).toBeVisible();
  await expect(page.getByTestId('frame-extension-badge')).toHaveCount(0);
  await page.evaluate(() => { document.documentElement.dataset.hrhrFrames = '9.9.9'; });
  const badge = page.getByTestId('frame-extension-badge').first();
  await expect(badge).toHaveText('拡張機能: 有効');
  await expect(badge).toHaveAttribute('data-version', '9.9.9');
});
