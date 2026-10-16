import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

test('顧客の掲載中・過去の全求人、案件リンクと求人の往復を確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  const rows = Array.from({ length: 26 }, (_, i) => ({ id: String(i + 1), title: `合成求人${String(i + 1).padStart(2, '0')}`, media: i % 2 ? 'airwork' : 'hrh', media_job_id: String(i + 1), account_id: null, prefecture: null, municipality: null, category: null, publication_status: i < 3 ? '掲載中' : '掲載終了', listing_started_on: i === 2 ? null : i < 3 ? '2026-09-01' : '2026-04-01', listing_planned_end_on: '2027-08-31', listing_ended_on: i < 3 ? null : '2026-09-30', application_count: null, last_csv_detected_at: null, deal_ids: i === 0 ? ['10', '20'] : ['10'], summary: { body_changed_at: i === 1 ? '2026-10-15T00:00:00Z' : '2026-10-10T00:00:00Z' } }));
  let versions = 0;
  let companyReads = 0;
  const indexOffsets: number[] = [];
  await page.route('**/api/job-copy/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/company-deals')) { companyReads++; return route.fulfill({ json: { company: { id: '100', name: '合成事業所' }, deals: ['10', '20'].map((id, i) => ({ id, name: `合成案件${i ? 'B' : 'A'}`, stage_label: i ? '契約終了' : '契約中', current: false })) } }); }
    if (url.pathname.endsWith('/listings')) {
      const all = url.searchParams.get('deal') === '10,20';
      const offset = Number(url.searchParams.get('offset') ?? 0);
      if (all) indexOffsets.push(offset);
      const selected = url.searchParams.get('deal') === '20' ? rows.slice(0, 1) : rows;
      return route.fulfill({ json: { status: 'ready', listings: all ? selected.slice(offset, offset + 15) : selected, total: selected.length, offset, next_offset: all && offset === 0 ? 15 : null, titles: [], deals: [], index_built_at: '2026-10-16T00:00:00Z', refreshing: false, refresh_failed: false } });
    }
    if (url.pathname.endsWith('/deal-applications')) return route.fulfill({ json: { shared_applications: 0, listings: rows.map(row => ({ listing_id: row.id, by_date: { '2026-10-13': 7 }, fetched_at: '2026-10-16T00:00:00Z' })) } });
    if (url.pathname.endsWith('/listing-body-dates')) return route.fulfill({ json: { dates: {} } });
    if (url.pathname.endsWith('/versions')) { versions++; return route.fulfill({ json: { listing: rows.find(row => row.id === url.pathname.split('/').at(-2)), versions: [{ written_at: '2026-10-10T00:00:00Z', body: '仕事内容：合成の作業です。', image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } }); }
    return route.fulfill({ status: 503, json: {} });
  });
  await login(page);
  await page.goto('/app/job-copy?company=100');
  const main = page.locator('#job-details');
  const live = page.getByRole('table', { name: '掲載中の求人', exact: true });
  await expect(page.getByRole('heading', { name: '掲載中 3件' })).toBeVisible();
  await expect(live.locator('tbody tr')).toHaveCount(3);
  await expect(live.locator('tbody tr').first()).toContainText('合成求人02');
  await expect(live.locator('tbody tr').nth(1)).toContainText('合成求人01合成案件A · 契約中合成案件B · 契約終了');
  await expect(live.locator('tbody tr').first()).toContainText('7件');
  await expect(live.locator('tbody tr').first()).toContainText('2026/09/01 〜 今日');
  await expect(live.locator('tbody tr').last()).toContainText('開始日不明 〜 今日');
  await expect(live.locator('tbody tr').first()).toContainText('本文を変更2026/10/15 09:00');
  await expect(main.locator('summary')).toHaveText('過去（掲載終了） 23件');
  const past = page.getByRole('table', { name: '過去の求人', exact: true });
  await expect(past).toBeHidden();
  expect(indexOffsets).toEqual([0, 15]);
  expect(versions).toBe(0);
  expect(companyReads).toBe(1);
  await main.screenshot({ path: 'artifacts/company-live-past-collapsed.png' });
  await main.locator('summary').click();
  await expect(past.locator('tbody tr')).toHaveCount(20);
  await expect(past.locator('tbody tr').first()).toContainText('2026/04/01 〜 2026/09/30');
  await page.getByRole('navigation', { name: '過去のページ' }).getByRole('button', { name: '次の20件' }).click();
  await expect(past.locator('tbody tr')).toHaveCount(3);
  await expect(past.locator('tbody tr').first()).toContainText('合成求人24');
  await page.setViewportSize({ width: 1440, height: 1500 });
  await main.screenshot({ path: 'artifacts/company-live-past-expanded.png' });
  await live.getByRole('button', { name: '合成案件B' }).click();
  await expect(page).toHaveURL(/company=100&deal=20/);
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
  await page.goBack();
  await expect(live).toBeVisible();
  await live.locator('tbody tr').first().getByRole('cell').first().click();
  await expect(page).toHaveURL(/company=100&listing=2/);
  await page.goBack();
  await expect(page).toHaveURL(/company=100$/);
  await expect(live.locator('tbody tr')).toHaveCount(3);
});
