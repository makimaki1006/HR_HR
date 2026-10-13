import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/** 枠の拡張機能の目印 (<html data-hrhr-frames>) が無いときだけ、リンクの操作行に「枠で開けないサイトがあります」が出る。入っている (正常な) ときは何も出ない */
test('拡張機能の属性が無いとリンクの操作行にバッジが出て、付くと消える', async ({ page }) => {
  await login(page);
  await page.route(/^https:\/\/www\.example\.com\//, route =>
    route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>stub</title><p>stub</p>' }));
  await page.goto('/app/crm?mode=fixture');
  await page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row button.cq-row-button').first().click();
  await page.getByRole('tablist', { name: '求人検索・リンク先の表示' }).getByRole('tab', { name: 'ホームページ' }).click();

  const bar = page.locator('#cq-cpanel-home .cq-linkbar');
  await expect(bar).toBeVisible();
  const badge = page.getByTestId('frame-extension-badge').first();
  await expect(badge).toHaveText('枠で開けないサイトがあります');
  await page.evaluate(() => { document.documentElement.dataset.hrhrFrames = '9.9.9'; });
  await expect(page.getByTestId('frame-extension-badge')).toHaveCount(0);
});
