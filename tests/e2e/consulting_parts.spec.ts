import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// 共通部品と移動の確認。外部サービスを呼ばず、架空の案件を表示する。
const row = {
  deal_id: 'sample-1', name: '架空の採用支援', consultant: '担当サンプル',
  flags: ['採用目標の半分に届いていない', '満了まで60日以内', 'MTGが60日以上空いている'],
  n_flags: 3, amount: 1200000, days_left: 30, is_active: true,
  start: '2026-04-01', expiration: '2026-10-31', renewal_no: 1,
};
const meta = {
  today: '2026-10-01', n_active: 1, flag_counts: row.flags.map(label => ({ label, n: 1 })),
  source_as_of: '2026-09-21 21:33:00', source_age_days: 10,
};

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route('**/consulting', route => route.fulfill({
    contentType: 'text/html',
    body: readFileSync(resolve(__dirname, '../../templates/tabs/cs_dashboard.html'), 'utf8')
      .replaceAll('{{ user }}', '画面確認用'),
  }));
  await page.route('**/api/consulting/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/deal-detail')) {
      const id = url.searchParams.get('id') || url.searchParams.get('deal_id');
      return route.fulfill({ json: id ? {
        meta: { ...meta, deal_id: row.deal_id, found: true, summary_sheet: 'ok' },
        deal: row, counts: {}, chain: { rows: [] }, events: [],
      } : { meta: { ...meta, deal_id: null }, search: { q: '', rows: [], n_match: 0 } } });
    }
    return route.fulfill({ json: { meta, rows: [row] } });
  });
});

test('サイドバーは6項目で、非表示の2画面もハッシュから開ける', async ({ page }) => {
  await page.goto('/consulting#deal/byowner');
  await expect(page.locator('#owner-tbl')).toContainText('担当サンプル');
  await expect(page.locator('#cs-side button')).toHaveCount(6);
  await expect(page.locator('#cs-side')).not.toContainText('担当者ごとの案件');
  await expect(page.locator('#cs-side')).not.toContainText('案件の詳細');
  await page.locator('#owner-tbl a.drill').click();
  await expect(page.locator('#board-tbl')).toContainText(row.name);
  await page.goto('/consulting#deal/detail');
  await expect(page.locator('#cs-main')).toContainText('案件名・拠点名の一部で探せます');
  await expect(page).toHaveTitle('案件の詳細 — コンサルダッシュボード');
});

test('案件名から詳細へ移動でき、注意と鮮度に条件と全文を残す', async ({ page }) => {
  await page.goto('/consulting#deal/board');
  const table = page.locator('#board-tbl');
  await expect(table).toContainText(row.name);
  await expect(page.locator('#cs-main h2').first()).toContainText('稼働中の案件（1行が取引1件）');
  await expect(page.locator('#cs-main h2').first().locator('xpath=following-sibling::*[1]')).not.toHaveClass(/sec-hint/);
  const tags = table.locator('tbody tr').first().locator('.tag');
  await expect(tags).toHaveCount(3);
  await expect(tags.nth(0)).toHaveText('MTG60日以上空いている');
  await expect(tags.nth(1)).toHaveText('満了60日以内');
  await expect(tags.nth(2)).toHaveText('他1');
  await expect(tags.nth(2)).toHaveCSS('font-size', '12px');
  await expect(tags.nth(2)).toHaveCSS('line-height', '18px');
  await expect(tags.nth(2)).toHaveCSS('color', 'rgb(91, 97, 104)');
  await expect(tags.nth(2)).toHaveCSS('background-color', 'rgb(250, 251, 252)');
  for (const flag of row.flags) await expect(tags.nth(2)).toHaveAttribute('title', new RegExp(flag));
  await expect(page.locator('#cs-fresh > b')).toHaveText('10日前のデータです（2026-09-21 21:33）。');
  await expect(page.locator('#cs-fresh details')).not.toHaveAttribute('open');
  await page.locator('#cs-fresh summary').click();
  await expect(page.locator('#cs-fresh')).toContainText('元データを落としたのは 2026-09-21 21:33:00。');
  await table.getByRole('link', { name: row.name, exact: true }).click();
  await expect(page).toHaveURL(/#deal\/detail\?id=sample-1/);
  await expect(page.locator('#cs-main h2').first()).toContainText(row.name);
  await expect(page.locator('#cs-error')).toBeHidden();
});

test('見出しの補足とカード全文を残し、注意書きは畳まない', async ({ page }) => {
  await page.goto('/consulting#deal/board');
  await expect(page.locator('#board-tbl')).toBeVisible();
  await page.evaluate(`document.getElementById('cs-main').innerHTML =
    sec('問い', '接触を見る（90日以上）', 'sample-heading') +
    kpi('件数', '40', '<b>対象40件</b><br>n&lt;30') +
    note('info', '読み方', '<b>90日以内の動きを見ます。比べ方を説明します。</b>') +
    note('info', '注意', '集計の説明です。対象40件から3件を外しています。担当者の評価ではありません。');`);
  await expect(page.locator('#sample-heading')).toHaveText('接触を見る');
  await expect(page.locator('#sample-heading + .sec-hint')).toHaveText('（90日以上）');
  await expect(page.locator('.sec-hint')).toHaveCSS('font-size', '13px');
  await expect(page.locator('.sec-hint')).toHaveCSS('color', 'rgb(91, 97, 104)');
  await expect(page.locator('.kpi .fine')).toHaveAttribute('title', '対象40件 n<30');
  const fold = page.locator('#cs-main details');
  await expect(fold).toHaveCount(1);
  await expect(fold.locator('summary .keep')).toHaveText('90日以内の動きを見ます。');
  await expect(fold.locator('.note')).toBeHidden();
  const visibleNote = page.locator('#cs-main > .note');
  await expect(visibleNote).toContainText('対象40件から3件を外しています。担当者の評価ではありません。');
  await expect(visibleNote).toBeVisible();
  await fold.locator('summary').click();
  await expect(fold.locator('p b')).toHaveText('比べ方を説明します。');
});

test('古い注意のリンクを開いても案件を絞り込める', async ({ page }) => {
  await page.goto('/consulting#deal/board?flag=' + encodeURIComponent('MTGが90日以上途絶'));
  await expect(page.locator('#board-tbl')).toContainText(row.name);
  await expect(page.locator('#bf-flag')).toHaveValue('MTGが60日以上空いている');
  await expect(page).toHaveURL(new RegExp(encodeURIComponent('MTGが60日以上空いている')));
  await expect(page.locator('#cs-main')).not.toContainText('途絶');
  await expect(page.locator('#cs-main h2.mincho .no')).toHaveCount(0);
});
