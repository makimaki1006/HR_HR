import { test, expect } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';

// 顧客情報・外部サービスを使わず、架空の法人で実際の画面を確認する。
const index = [
  { houjin: 'sample-a', name: '架空法人あお', deals: 6, sites: 3, active: 2, ltv: 6000000,
    kigyouzentaininzuu: 1200, monthly_amount_max: 300000, max_renewal_no: 3,
    oubo_total: 80, mensetu_total: 20, syoudaku_total: 5, focus: true },
  { houjin: 'sample-b', name: '架空法人みどり', deals: 4, sites: 2, active: 0, ltv: 8000000,
    kigyouzentaininzuu: 1800, monthly_amount_max: 299999, max_renewal_no: 2,
    oubo_total: 100, mensetu_total: 30, syoudaku_total: 8, focus: true },
  { houjin: 'sample-c', name: '架空法人しろ', deals: 2, sites: 4, active: 1, ltv: 1000000,
    kigyouzentaininzuu: null, monthly_amount_max: null, max_renewal_no: null,
    oubo_total: null, mensetu_total: null, syoudaku_total: null, focus: false },
];
const focus = { n_focus: 2, n_display: 3 };
const meta = { today: '2026-10-18', source_as_of: '2026-10-18 09:00:00', source_age_days: 0 };

test('顧客を手元で絞り込み、URLで復元し、結果から法人を開ける', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  let requests = 0;
  await page.route('http://localhost/consulting', route => route.fulfill({
    contentType: 'text/html',
    body: readFileSync(resolve(__dirname, '../../templates/tabs/cs_dashboard.html'), 'utf8')
      .replaceAll('{{ user }}', '架空の利用者'),
  }));
  await page.route('**/api/consulting/**', route => {
    requests++;
    const houjin = new URL(route.request().url()).searchParams.get('houjin');
    if (!houjin) return route.fulfill({ json: { meta, index, focus, default_houjin: "sample-b", default_reason: "架空の表示例です" } });
    return route.fulfill({ json: {
      meta: { ...meta, found: true, houjin }, customer: index.find(r => r.houjin === houjin),
      focus, deals: [], mtgs: [], cpa: [], events: [],
    } });
  });
  const results = page.locator('#customer-filter-results');
  const table = page.locator('#customer-filter-table');
  const employee = page.getByRole('spinbutton', { name: '従業員数の下限', exact: true });
  const sites = page.getByRole('spinbutton', { name: '拠点数の下限', exact: true });
  const acceptances = page.getByRole('spinbutton', { name: '承諾の下限', exact: true });
  const shot = async (name: string) => {
    const dir = process.env.KPI_FILTER_SCREENSHOTS;
    if (dir) {
      mkdirSync(dir, { recursive: true });
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.screenshot({ path: join(dir, name + '.png') });
    }
  };
  await page.goto('http://localhost/consulting#research/customer');
  await expect(results).toContainText('条件に合う 3 社（全 3 社のうち）');
  await expect(page.locator("#cs-houjin")).toHaveValue("sample-b");
  await expect(table).toHaveCount(0);
  await expect(page.locator('#cs-houjin option')).toHaveCount(4);
  await shot('01-no-conditions');
  const initialRequests = requests;
  await employee.fill('1000');
  await expect(employee).toBeFocused();
  await sites.fill('3');
  await expect(table.locator('tbody tr')).toHaveCount(1);
  await expect(table).toContainText('架空法人あお');
  await expect(results).toContainText('従業員数が入っていない 1 社は外れています');
  await expect(page.locator('#cs-houjin option')).toHaveCount(2);
  await expect(page).toHaveURL(/employees=1000.*sites=3/);
  expect(requests).toBe(initialRequests);
  await shot('02-employees-and-sites');
  await page.reload();
  await expect(employee).toHaveValue('1000');
  await expect(sites).toHaveValue('3');
  await expect(table.locator('tbody tr')).toHaveCount(1);
  await employee.fill('');
  await sites.fill('');
  await acceptances.fill('5');
  await page.getByLabel('稼働中だけ', { exact: true }).check();
  await expect(table.locator('tbody tr')).toHaveCount(1);
  await expect(table).toContainText('架空法人あお');
  await expect(results).toContainText('承諾が入っていない 1 社は外れています');
  await shot('03-acceptances-and-active');
  const beforeSelect = requests;
  await table.getByRole('link', { name: '架空法人あお' }).click();
  await expect(page).toHaveURL(/houjin=sample-a/);
  await expect(page.locator('#cs-main')).toHaveAttribute('aria-busy', 'false');
  await expect(page.locator('#cs-houjin')).toHaveValue('sample-a');
  expect(requests).toBe(beforeSelect + 1);
  await page.reload();
  await expect(acceptances).toHaveValue('5');
  await expect(page.getByLabel('稼働中だけ', { exact: true })).toBeChecked();
  await expect(table).toContainText('架空法人あお');
  await expect(page.locator('#cs-houjin')).toHaveValue('sample-a');
  await expect(page.locator('#cs-error')).toBeHidden();
});
