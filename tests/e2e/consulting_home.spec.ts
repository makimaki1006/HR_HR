import { test, expect } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';

const template = readFileSync(resolve(__dirname, '../../templates/tabs/cs_dashboard.html'), 'utf8');
const meta = { today: '2026-10-19', source_as_of: '2026-10-19 21:30:00', source_age_days: 0,
  n_consultant: 2, n_active: 15, retired_deals: 0, retired_people: 0, unknown_owner: 1,
  not_counted: '担当者の評価ではありません。仕事量を見る一覧です。' };
const row = { consultant: '架空担当あお', n_active: 10, unattended: 4, n_flags2: 5, mtg_critical: 1,
  no_contact: 0, expiring60: 3, nps_low: 1, expiring_week: 2 };
const delta = { n_flags2: 3, mtg_critical: -2, no_contact: 0, expiring60: 1, nps_low: -1, expiring_week: 1 };
function team(withDifference: boolean) {
  return {
    meta, consultants: { rows: [], meta: {}, contact_rule: '' },
    status: { rows: [{ ...row, ...(withDifference ? { delta } : {}) },
      { ...row, consultant: '架空担当みどり', n_active: 4, n_flags2: 1 }],
      meta: { n_active: 15, n_flags2: 7, expiring_week: 4, min_flags: 2, week_days: 7,
        unknown: { n_active: 1, n_flags2: 1 }, flag_labels: {
          no_contact: '接触の記録が無い', expiring60: '満了まで60日以内', nps_low: 'NPSが4以下' } } },
    contact: { meta: {}, month: { periods: [], rows: [], team: [] } },
    handover: { meta: { n: 0, n_active: 0 }, rows: [], reflected_dist: [] },
    ...(withDifference ? { comparison: { data_day: '2026-10-16', totals: { ...delta, n_flags2: 4 } } } : {}),
  };
}
async function shot(page: import('@playwright/test').Page, name: string) {
  const dir = process.env.KPI_HOME_SCREENSHOTS;
  if (!dir) return;
  mkdirSync(dir, { recursive: true });
  await expect(page.locator('#cs-main')).toHaveAttribute('aria-busy', 'false');
  await page.screenshot({ path: join(dir, name + '.png'), animations: 'disabled' });
}

for (const withDifference of [true, false]) {
  test(`ハッシュなしでチームと担当を開く（差${withDifference ? 'あり' : 'なし'}）`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.route('http://localhost/consulting', route => route.fulfill({ contentType: 'text/html', body: template.replaceAll('{{ user }}', '架空の利用者') }));
    await page.route('**/api/consulting/team*', route => route.fulfill({ json: team(withDifference) }));
    await page.route('**/api/consulting/today*', route => route.fulfill({ json: {
      meta: { ...meta, n_hit: 0, n_shown: 0, n_not_started: 0 }, rows: [], candidates: [], consultants: [],
      expiring_this_week: [], started_this_week: [],
    } }));
    await page.goto('http://localhost/consulting');
    await expect(page).toHaveURL(/#deal\/team$/);
    await expect(page.locator('#team-tbl tbody tr')).toHaveCount(2);
    await expect(page.locator('#cs-error')).toBeHidden();
    await expect(page.locator('#cs-side button').nth(0)).toHaveText('チームと担当');
    await expect(page.locator('#cs-side button').nth(1)).toHaveText('今日');
    await expect(page.locator('#cs-side button').nth(2)).toHaveText('案件一覧');
    const cells = page.locator('#team-tbl tbody tr').first().locator('td');
    await expect(cells.nth(3)).toHaveText(withDifference ? '5+3' : '5');
    await expect(cells.nth(4)).toHaveText(withDifference ? '1−2' : '1');
    await expect(cells.nth(5)).toHaveText('0');
    await expect(page.locator('#team-tbl tfoot td').nth(2)).toHaveText(withDifference ? '7+4' : '7');
    if (withDifference) {
      await expect(page.getByText('10/16 のデータとの差', { exact: true })).toHaveCount(1);
      await expect(cells.nth(3).locator('.team-delta')).toHaveCSS('color', 'rgb(171, 47, 38)');
      await expect(cells.nth(4).locator('.team-delta')).toHaveCSS('color', 'rgb(15, 107, 81)');
    } else {
      await expect(page.locator('.team-delta')).toHaveCount(0);
      await expect(page.getByText('のデータとの差')).toHaveCount(0);
    }
    await shot(page, withDifference ? 'team-with-difference' : 'team-without-difference');
    // 従来のURLも週の指定を保って同じ画面を開く。
    await page.goto('http://localhost/consulting#research/team?unit=week');
    await expect(page).toHaveURL(/#deal\/team\?unit=week/);
    await expect(page.locator('#team-tbl')).toBeVisible();
    await page.getByRole('button', { name: '今日', exact: true }).click();
    await expect(page).toHaveURL(/#deal\/today$/);
    await expect(page.locator('#cs-main')).toHaveAttribute('aria-busy', 'false');
    await expect(page.locator('#cs-error')).toBeHidden();
    if (!withDifference) await shot(page, 'today');
  });
}
