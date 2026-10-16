import { expect, test } from '@playwright/test';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { login } from './helpers/login';

// 外部サービスを使わず、5分類すべてと補助カードの合成データを返す。
test('ホームの9枚のカードと、変更求人の一覧・求人と案件への移動を確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1500 });
  await page.route('**/api/job-copy/recent-changes', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 3, listings: [
    { id: '1', title: '合成終了求人', media: 'airwork', publication_status: '掲載終了', deals: [{ id: '10', name: '合成案件A', company: '合成事業所' }, { id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文', '給与', '求人名'], last_changed_at: '2026-10-15T16:12:00Z' },
    { id: '3', title: '合成配送求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文'], last_changed_at: '2026-10-12T03:00:00Z' },
    { id: '2', title: '合成掲載求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '10', name: '合成案件A', company: null }], fields: ['給与'], last_changed_at: '2026-10-09T15:00:00Z' },
  ] } } }));
  await page.route('**/api/job-copy/home', route => route.fulfill({ json: demoHome }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy');
  const cards = page.locator('.jh-card');
  await expect(cards).toHaveCount(9);
  await expect(page.getByRole('button', { name: /直近7日に変更/ }).locator('.jh-count')).toHaveText('3件');
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
  await page.getByRole('button', { name: /直近7日に変更/ }).click();
  const changes = page.getByRole('region', { name: '直近7日に変更の一覧' });
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await expect(changes.locator('tbody tr').nth(0)).toHaveText('合成事業所合成案件A・合成案件B合成終了求人AirWork掲載終了本文・給与・求人名2026/10/16 01:12');
  await expect(changes.locator('tbody tr').nth(1)).toHaveText('合成事業所合成案件B合成配送求人HRハッカー掲載中本文2026/10/12 12:00');
  await expect(changes.locator('tbody tr').nth(2)).toHaveText('—合成案件A合成掲載求人HRハッカー掲載中給与2026/10/10 00:00');
  if (process.env.HOME_TREND_SCREENSHOT) {
    // 認証用の合成メールアドレスも写さず、ホームの主領域を保存する。
    await page.mouse.move(1400, 100);
    await page.keyboard.press('Escape');
    await expect(page.locator('.jc-sidebar > .jc-list')).toBeHidden();
    await page.locator('.jc-workspace').screenshot({ path: process.env.HOME_TREND_SCREENSHOT });
  }
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [], index_built_at: '2026-10-16T00:00:00Z' } }));
  const ended = { id: '1', title: '合成終了求人', media: 'airwork', media_job_id: 'synthetic', account_id: null, publication_status: '掲載終了', prefecture: null, municipality: null, category: null, last_csv_detected_at: null, application_count: 0 };
  await page.route('**/api/job-copy/listings/1/versions', route => route.fulfill({ json: { listing: ended, current_fields: {}, versions: [{ written_at: '2026-10-15T16:12:00Z', body: '仕事内容：合成の検証用求人です。', image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } }));
  await changes.getByRole('button', { name: '合成終了求人' }).click();
  await expect(page).toHaveURL(/listing=1/);
  await expect(page.getByRole('heading', { name: '合成終了求人', exact: true })).toBeVisible();
  await expect(page.locator('.jc-detail-heading')).toContainText('掲載終了');
  await page.goBack();
  await expect(changes).toBeVisible();
  expect(new URL(page.url()).searchParams.get('card')).toBe('recent_changes');
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await page.goForward();
  await expect(page.getByRole('heading', { name: '合成終了求人', exact: true })).toBeVisible();
  await page.goBack();
  await changes.getByRole('button', { name: '合成案件A' }).first().click();
  await expect(page).toHaveURL(/deal=10/);
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
  await expect(page.locator('.jc-deal-comparison > header p')).toHaveText('合成案件A');
  await page.goBack();
  await expect(changes).toBeVisible();
  expect(new URL(page.url()).searchParams.get('card')).toBe('recent_changes');
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await page.goForward();
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
});
