import { test, expect } from '@playwright/test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { login } from './helpers/login';

const screenshots = process.env.CRM_EMAIL_SCREENSHOT_DIR ?? path.join(os.tmpdir(), 'hrhr-crm-email');

test('Gmail consent, composed email and sent activity use only local sample data', async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 1800, height: 1400 });
  // The real server flag remains OFF. Explicitly enable the capability on this local fake page.
  await page.route('**/app/crm?mode=fixture', async route => {
    const response = await route.fetch();
    const html = (await response.text()).replace('id="app-root"', 'id="app-root" data-crm-email-enabled="1"');
    await route.fulfill({ response, body: html });
  });
  const external: string[] = [];
  page.on('request', req => {
    if (/gmail\.googleapis\.com|api\.hubapi\.com|turso\.io/u.test(req.url())) external.push(req.url());
  });
  await page.goto('/app/crm?mode=fixture');
  await page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row button.cq-row-button').first().click();
  await expect(page.getByText('Gmail での送信を許可してください')).toBeVisible();
  fs.mkdirSync(screenshots, { recursive: true });
  await page.screenshot({ path: `${screenshots}/01-before-permission.png`, fullPage: true });
  await page.getByRole('button', { name: 'Gmail で送信を許可する', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Gmail の許可を取り消す' })).toHaveCount(0);
  await page.getByRole('button', { name: 'メールを書く', exact: true }).click();
  await expect(page.getByText('Gmail の許可を取り消す', { exact: true })).toBeHidden();
  await page.getByText('その他', { exact: true }).click();
  await expect(page.getByRole('button', { name: 'Gmail の許可を取り消す' })).toBeVisible();
  await page.getByText('その他', { exact: true }).click();
  await page.getByLabel('件名', { exact: true }).fill('採用のご相談（架空）');
  await page.getByLabel('本文', { exact: true }).fill('丁野様\n採用人数3名のご相談について、資料をご案内します。\n資料（https://example.invalid/docs?a=1&b=2）。\n来週の火曜日にお話しできますでしょうか。');
  await expect(page.getByLabel('宛先', { exact: true })).toContainText('sample@example.invalid');
  await page.screenshot({ path: `${screenshots}/02-compose-email.png`, fullPage: true });
  await page.getByRole('button', { name: '送信', exact: true }).click();
  await expect(page.getByText('送信済み・保存済み')).toBeVisible();
  const activity = page.getByTestId('activity-log');
  await expect(activity.getByText('採用のご相談（架空）', { exact: true })).toBeVisible();
  await expect(activity.getByText('採用人数3名', { exact: false })).toBeVisible();
  await expect(activity.getByText('保存済み', { exact: true }).first()).toBeVisible();
  const sent = activity.getByTestId('activity-card').filter({ hasText: '採用のご相談（架空）' });
  await sent.getByRole('button', { name: '全文を表示' }).click();
  await expect(sent.getByRole('link', { name: 'https://example.invalid/docs?a=1&b=2' })).toHaveAttribute('href', 'https://example.invalid/docs?a=1&b=2');
  await expect(activity.getByText('添付 0 件', { exact: false })).toHaveCount(0);
  await page.screenshot({ path: `${screenshots}/03-sent-activity-log.png`, fullPage: true });
  expect(external).toEqual([]);
});
