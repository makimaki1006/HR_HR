import { test, expect } from '@playwright/test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { login } from './helpers/login';

test('imported reply appears in activity log using only local sample data', async ({ page }) => {
  await login(page);
  await page.setViewportSize({ width: 1800, height: 1400 });
  await page.route('**/app/crm?mode=fixture', async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: (await response.text()).replace('id="app-root"', 'id="app-root" data-crm-email-enabled="1"') });
  });
  const external: string[] = [];
  page.on('request', req => { if (/gmail\.googleapis\.com|api\.hubapi\.com|turso\.io/u.test(req.url())) external.push(req.url()); });
  await page.goto('/app/crm?mode=fixture');
  await page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row button.cq-row-button').first().click();
  await page.getByRole('button', { name: 'Gmail で送信を許可する', exact: true }).click();
  await page.getByRole('button', { name: 'メールを書く', exact: true }).click();
  await page.getByLabel('件名', { exact: true }).fill('採用のご相談（架空）');
  await page.getByLabel('本文', { exact: true }).fill('採用人数3名について、来週火曜日にお話しできますでしょうか。');
  await page.getByRole('button', { name: '送信', exact: true }).click();
  await expect(page.getByText('送信済み・保存済み')).toBeVisible();
  const activity = page.getByTestId('activity-log');
  const reply = activity.getByTestId('activity-card').filter({ hasText: 'Re: 採用計画について（架空）' });
  await expect(reply.getByText('受信', { exact: true })).toBeVisible();
  await expect(reply.getByText('来週の火曜日、14時', { exact: false })).toBeVisible();
  await expect(activity.getByText('返信の確認 10 分前')).toBeVisible();
  const screenshots = process.env.CRM_EMAIL_SCREENSHOT_DIR ?? path.join(os.tmpdir(), 'hrhr-crm-email');
  fs.mkdirSync(screenshots, { recursive: true });
  await page.screenshot({ path: path.join(screenshots, '04-reply-imported.png'), fullPage: true });
  expect(external).toEqual([]);
});
