import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import * as fs from 'node:fs';
import * as path from 'node:path';

test('応募: 一覧から詳細、対応ルール、報告シート、取引モードへ戻る', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await login(page);
  const crmRequests: string[] = [];
  await page.route('**/api/crm/**', async route => { crmRequests.push(route.request().url()); await route.abort(); });
  await page.route('https://docs.google.com/**', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>架空の報告シート</title>' }));
  await page.goto('/app/crm?mode=fixture&crmMode=applicants');
  await expect(page.getByText('2件', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /架空 はるか/ }).click();
  const detail = page.getByRole('main', { name: '応募の詳細' });
  await expect(detail.getByRole('heading', { level: 2 })).toHaveText('架空 はるか');
  await expect(detail.getByText('カクウ ハルカ')).toBeVisible();
  await expect(detail.getByRole('button', { name: 'Zoom で発信' })).toBeDisabled();
  const output = process.env.CRM_APPLICANTS_ARTIFACTS;
  async function shot(name: string) {
    if (output) { fs.mkdirSync(output, { recursive: true }); await page.screenshot({ path: path.join(output, name) }); }
  }
  await shot('01-list-detail.png');
  await detail.getByText('ヒアリングと結果', { exact: true }).scrollIntoViewIfNeeded();
  await expect(detail.getByText('資格なし', { exact: true })).toBeVisible();
  await shot('02-rules-hearing.png');
  await page.getByRole('tab', { name: 'Zoom', exact: true }).click();
  await expect(page.getByText('架空サンプルでは発信できません')).toBeVisible();
  await page.getByRole('tab', { name: '報告シート', exact: true }).click();
  await expect(page.locator('iframe[title^="報告シート"]')).toHaveAttribute('src', 'https://docs.google.com/spreadsheets/d/fictional-applicants-sheet/edit');
  await shot('03-report-sheet.png');
  await page.getByRole('button', { name: /架空 あおい/ }).click();
  await expect(page.getByText('この応募には報告シートの URL がありません')).toBeVisible();
  await shot('04-no-sheet.png');
  expect(crmRequests).toEqual([]);
  await page.getByRole('link', { name: '取引', exact: true }).click();
  await expect(page.getByRole('list', { name: '架電キュー' })).toBeVisible();
  expect(new URL(page.url()).searchParams.get('mode')).toBe('fixture');
  await expect(page.getByRole('link', { name: '取引', exact: true })).toHaveAttribute('aria-current', 'page');
});
