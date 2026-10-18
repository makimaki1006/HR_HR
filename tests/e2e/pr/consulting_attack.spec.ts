import { test, expect } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { consultingFixture } from './helpers/consulting_layout_fixture';
import { findLayoutProblems } from '../helpers/layout_check';

const template = readFileSync(resolve(__dirname, '../../../templates/tabs/cs_dashboard.html'), 'utf8');
const customers = [
  { houjin: 'sample-a', name: '架空法人あお', sites: 2, active: 1, kigyouzentaininzuu: 1000, monthly_amount_max: 300000, syoudaku_total: 5, focus: true, ltv: 3000000 },
  { houjin: 'sample-b', name: '架空法人みどり', sites: 3, active: 1, kigyouzentaininzuu: 999, monthly_amount_max: 299999, syoudaku_total: 4, focus: false, ltv: 2000000 },
  { houjin: 'sample-c', name: '架空法人しろ', sites: null, active: 0, kigyouzentaininzuu: 2000, monthly_amount_max: 400000, syoudaku_total: 8, focus: true, ltv: 1000000 },
];
const pending = { rows: [
  { date: '2026-10-16', subject: '架空の相談あお', host: '架空担当あお', candidates: '架空案件あお・架空案件しろ', reason: '候補が複数', recording: 'https://example.invalid/recording-a' },
  { date: '2026-10-18', subject: '架空の相談みどり', host: '架空担当みどり', candidates: '', reason: '候補が無い', recording: '' },
] };
async function shot(page: import('@playwright/test').Page, name: string) {
  const dir = process.env.KPI_ATTACK_SCREENSHOTS;
  if (dir) {
    mkdirSync(dir, { recursive: true });
    if (name === '06-pending-recordings') await page.getByText('録画のうち案件に結び付いていないもの 2 件', { exact: true }).evaluate(el => window.scrollTo(0, el.getBoundingClientRect().top + window.scrollY - 80));
    else await page.evaluate(() => window.scrollTo(0, 0));
    await page.screenshot({ path: join(dir, name + '.png') });
  }
  expect(await findLayoutProblems(page, '.wrap')).toEqual({ overlaps: [], spills: [], svgClipped: [], pageOverflow: false });
}
test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route('http://localhost/consulting', r => r.fulfill({ contentType: 'text/html', body: template.replaceAll('{{ user }}', '架空の利用者') }));
});
test('4つの条件ボタン、手で変更、URLから再現', async ({ page }) => {
  await page.route('**/api/consulting/**', r => r.fulfill({ json: { meta: { today: '2026-10-18' }, index: customers, focus: { n_focus: 2, n_display: 3 } } }));
  await page.goto('http://localhost/consulting#research/customer');
  const names = ['本社に提案できそう', '大きいのに拠点が少ない', 'お金を多くかけている', '重点顧客'];
  for (const [i, name] of names.entries()) {
    await page.getByRole('button', { name, exact: true }).click();
    await expect(page.getByRole('button', { name, exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('[data-customer-preset][aria-pressed=true]')).toHaveCount(1);
    await expect(page.locator('#customer-filter-results')).toContainText(`条件に合う ${i === 3 ? 2 : 1} 社`);
    if (i < 3) await expect(page.locator('#customer-filter-table tbody tr')).toHaveCount(1);
    await shot(page, `0${i + 1}-customer-preset`);
    const url = page.url();
    await page.reload();
    await expect(page.getByRole('button', { name, exact: true })).toHaveAttribute('aria-pressed', 'true');
    expect(page.url()).toBe(url);
  }
  await page.getByRole('button', { name: names[1], exact: true }).click();
  await expect(page.getByRole('spinbutton', { name: '拠点数の上限' })).toHaveValue('2');
  await expect(page.locator('#customer-filter-results')).toContainText('拠点数が入っていない 1 社は外れています');
  await page.getByRole('spinbutton', { name: '拠点数の上限' }).fill('1');
  await expect(page.locator('[data-customer-preset][aria-pressed=true]')).toHaveCount(0);
  await expect(page.locator('#customer-filter-results')).toContainText('条件に合う 0 社');
  await page.reload();
  await expect(page.getByRole('spinbutton', { name: '拠点数の上限' })).toHaveValue('1');
});
test('チームの既定と旧URL、録画2件の畳んだ一覧', async ({ page }) => {
  const fixture = consultingFixture(3, 3, false);
  let waiting: unknown = null;
  await page.route('**/api/consulting/**', r => {
    const u = new URL(r.request().url());
    const data = fixture.api(u.pathname, u.searchParams) as any;
    if (u.pathname.endsWith('/team')) {
      data.status.meta.n_active = 9;
      data.status.rows = ['架空担当う', '架空担当あ', '架空担当い'].map((consultant, i) => ({ consultant, n_active: 3, unattended: [2, 1, 2][i] }));
      data.pending_recordings = waiting;
    }
    return r.fulfill({ json: data });
  });
  await page.goto('http://localhost/consulting#deal/team');
  const names = page.locator('#team-tbl tbody tr td:first-child');
  await expect(names).toHaveText(['架空担当い', '架空担当う', '架空担当あ']);
  await expect(page.getByText('録画のうち案件に結び付いていないもの', { exact: false })).toHaveCount(0);
  await shot(page, '05-team-default');
  await page.goto('http://localhost/consulting#deal/team?sort=consultant');
  await expect(names).toHaveText(['架空担当あ', '架空担当い', '架空担当う']);
  for (const value of [null, { rows: [] }, pending]) {
    waiting = value;
    await page.reload();
    await expect(page.locator('#cs-main')).toHaveAttribute('aria-busy', 'false');
    if (value !== pending) await expect(page.getByText('録画のうち案件に結び付いていないもの', { exact: false })).toHaveCount(0);
  }
  await expect(page.getByText('録画のうち案件に結び付いていないもの 2 件', { exact: true })).toBeVisible();
  const details = page.locator('details').filter({ has: page.locator('summary', { hasText: '結び付いていない録画を見る' }) });
  await expect(details).not.toHaveAttribute('open');
  await details.locator('summary').click();
  await expect(details.locator('tbody tr td:first-child')).toHaveText(['2026-10-18', '2026-10-16']);
  await expect(details.getByRole('link', { name: '録画', exact: true })).toHaveAttribute('href', 'https://example.invalid/recording-a');
  await shot(page, '06-pending-recordings');
});
