import { expect, test } from '@playwright/test';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { login } from './helpers/login';

// 外部サービスを使わず、5分類すべてと補助カードの合成データを返す。
test('ホームの8枚のカードと、応募ゼロの求人の課金を確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1500 });
  await page.route('**/api/job-copy/home', route => route.fulfill({ json: demoHome }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy');
  const cards = page.locator('.jh-card');
  await expect(cards).toHaveCount(8);
  for (const [index, card] of demoHome.home.cards.entries()) {
    await expect(cards.nth(index).locator('.jh-count')).toHaveText(`${card.count}件`);
    await expect(cards.nth(index).locator('.jh-card-title')).toHaveText(card.title);
    await expect(cards.nth(index).locator('.jh-rule')).toHaveText(card.rule);
  }
  await expect(cards.nth(0)).toHaveClass(/jh-alert/);
  await expect(cards.nth(1)).toHaveClass(/jh-alert/);
  await expect(page.getByText(/2件以上、または20%以上の差を増減/)).toBeVisible();
  for (const [index, applications] of ['0 → 0 件', '1 → 0 件', '5 → 4 件', '0 → 1 件', '10 → 9 件'].entries()) {
    await cards.nth(index).click();
    const list = page.getByRole('region', { name: `${demoHome.home.cards[index]!.title}の一覧` });
    await expect(list.getByRole('cell', { name: applications, exact: true }).first()).toBeVisible();
  }
  await cards.nth(0).click();
  const list = page.getByRole('region', { name: '応募がゼロのままの求人の一覧' });
  await expect(list.getByRole('columnheader', { name: '課金（直近 4 週）' })).toBeVisible();
  await expect(list.locator('tbody tr').nth(0)).toContainText('¥24,000');
  await expect(list.locator('tbody tr').nth(1)).toContainText('¥0');
  await expect(page.getByRole('columnheader', { name: '応募（前の 4 週 → 直近 4 週）' })).toBeVisible();
  if (process.env.HOME_TREND_SCREENSHOT) {
    // 認証用の合成メールアドレスも写さず、ホームの主領域を保存する。
    await page.mouse.move(1400, 100);
    await page.keyboard.press('Escape');
    await expect(page.locator('.jc-sidebar > .jc-list')).toBeHidden();
    await page.locator('.jc-workspace').screenshot({ path: process.env.HOME_TREND_SCREENSHOT });
  }
});
