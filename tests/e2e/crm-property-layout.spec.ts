import { expect, test } from '@playwright/test';
import path from 'node:path';
const dir = process.env.SCREENSHOT_DIR;
test('admin can save an ordered BPO standard; BPO reads it and resets personal settings', async ({ page }) => {
  let layout = ['dealname', 'amount'];
  let admin = true;
  await page.route('**/api/crm/property-layouts**', async route => {
    if (route.request().method() === 'PUT') { expect(route.request().headers()['x-requested-with']).toBe('fetch'); layout = route.request().postDataJSON().properties; await route.fulfill({ json: { saved: true } }); }
    else await route.fulfill({ json: { role: admin ? 'admin' : 'bpo', can_edit: admin, layouts: { deal: layout } } });
  });
  await page.goto('/static/app/__layout_preview');
  await page.getByText('表示する項目を選ぶ').click();
  await page.getByLabel('案件名を下へ').click();
  await page.getByRole('button', { name: 'この並びを BPO の標準にする' }).click();
  await expect(page.getByText('BPOの案件の標準を保存しました。')).toBeVisible();
  expect(layout).toEqual(['amount', 'dealname']);
  if (dir) await page.screenshot({ path: path.join(dir, 'layout-admin.png'), fullPage: true });
  admin = false;
  await page.reload();
  const names = page.locator('.pp-list dt');
  await expect(names).toHaveText(['金額', '案件名']);
  await page.getByText('表示する項目を選ぶ').click();
  await expect(page.getByRole('button', { name: /この並びを/ })).toHaveCount(0);
  await page.getByLabel('金額を下へ').click();
  await page.getByText('この項目で表示する').click();
  await expect(names).toHaveText(['案件名', '金額']);
  await page.getByText('役割の標準に戻す').click();
  await expect(names).toHaveText(['金額', '案件名']);
  if (dir) await page.screenshot({ path: path.join(dir, 'layout-bpo.png'), fullPage: true });
});
