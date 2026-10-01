import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

test('競合調査を独立ページから作成し4タブと最低賃金を確認する', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: '競合調査', exact: true }).click();
  await expect(page).toHaveURL(/\/competitor$/);
  await page.locator('#survey-title').fill('大阪府・施設長');
  await page.locator('#prefecture').selectOption('大阪府');
  await page.locator('input[name="include_google"]').uncheck();
  await page.locator('#csv-file').setInputFiles({
    name: 'competitors.csv',
    mimeType: 'text/csv',
    buffer: Buffer.from('タイトル,会社名,勤務地,給与,雇用形態\n施設長,A社,大阪府大阪市,月給 25万円 ~ 30万円,正社員\n施設長,B社,大阪府大阪市,月給 35万円 ~ 40万円,正社員\n'),
  });
  await page.getByRole('button', { name: '競合調査レポートを作成' }).click();
  await expect(page).toHaveURL(/\/report\/competitor$/);
  await expect(page.locator('#panel-excel')).toBeVisible();
  await expect(page.locator('[role="tab"]')).toHaveCount(4);
  for (const name of ['google', 'indeed', 'population', 'excel']) {
    await page.locator(`#tab-${name}`).click();
    await expect(page.locator(`#panel-${name}`)).toBeVisible();
    await expect(page.locator('[role="tabpanel"]:visible')).toHaveCount(1);
    await expect(page.locator(`#tab-${name}`)).toHaveAttribute('aria-selected', 'true');
  }
  await page.locator('#tab-population').click();
  const region = page.locator('#panel-population');
  await expect(region).toContainText('1,231');
  await expect(region).toContainText('2026-10-01');
  await expect(region).toContainText('厚生労働省の公式改定一覧');
  await page.locator('#tab-population').focus();
  await page.keyboard.press('Home');
  await expect(page.locator('#panel-excel')).toBeVisible();
});
