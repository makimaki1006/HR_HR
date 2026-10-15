import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';

// 合成データ: HubSpot の本文の履歴は 10/09 から、Indeed の週ごとの実績は 6/15 から。
const SHOTS = process.env.JOB_COPY_SHOTS_DIR;
const listing = { id: '10', media: 'hrh', media_job_id: '1234567', account_id: 'synthetic-shop', title: '合成配送求人', prefecture: '大分県', municipality: '大分市', category: 'ドライバー', publication_status: '公開中', last_csv_detected_at: null, application_count: null, deal_ids: [] };
const day = (d: Date) => d.toISOString().slice(0, 10);
const weeks = (count: number) => Array.from({ length: count }, (_, i) => {
  const start = new Date(Date.UTC(2026, 5, 15 + i * 7)); const end = new Date(start.getTime() + 6 * 86_400_000);
  return { period_start: day(start), period_end: day(end), impressions: 800 + i * 10, clicks: 30 + i, cost_yen: i === 0 ? 5000 : i % 5 === 4 ? 0 : 7500, applications: i % 2 };
});
const perf = (count: number) => ({ schema_version: 1, source: 'hrhacker', job_id: '1234567', captured_at: '2026-10-14T09:00:00+09:00', rows: weeks(count) });

const history = { kyuujin_status: [], baitai_genjoukyou_hrhakkaa: [{ timestamp: '2026-06-15T00:00:00Z', value: '公開' }, { timestamp: '2026-08-03T03:00:00Z', value: '非公開' }, { timestamp: '2026-08-17T03:00:00Z', value: '公開' }], baitai_genjoukyou_airwork: [] };
const person = (n: number, date: string) => ({ key: String(n).padStart(64, '0'), date, gender: n % 2 ? '女性' : '男性', age: '30代', prefecture: '大分県', municipality: '大分市' });

async function open(page: Page, application: Record<string, unknown> | 'fail', extra: Record<string, unknown> = {}, weekCount = 17) {
  await page.route('**/api/job-copy/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/versions')) return route.fulfill({ json: { listing, status_history: history, ...extra, indeed_weekly_perf: perf(weekCount), versions: [
      { written_at: '2026-10-09T00:00:00Z', body: '仕事内容\n配送します。\n給与：時給 1,100 円', image_urls: [] },
      { written_at: '2026-10-13T00:00:00Z', body: '仕事内容\n配送します。\n給与：時給 1,200 円', image_urls: [] },
    ], history_counts: { hrh_kyuujinhyou_honbun: 2, hrh_kyuujinhyou_gazou: 0 }, history_may_be_incomplete: false } });
    if (url.pathname.endsWith('/applicants')) return application === 'fail' ? route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } }) : route.fulfill({ json: { listing_id: '10', fetched_at: '2026-10-14T09:00:00+09:00', ...application } });
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { listings: [listing], titles: ['ドライバー'], status: 'ready', total: 1, index_built_at: '2026-10-14T00:00:00Z', offset: 0, next_offset: null, refreshing: false, refresh_failed: false } });
    if (url.pathname.endsWith('/market')) return route.fulfill({ json: { titles: [], prefectures: [], series: null } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/static/app/job-copy-preview.html?demo=1&listing=10');
}

test('掲載開始は課金の最初の週。応募は日別に出て、本文は「記録しはじめた」', async ({ page }) => {
  const byDate: Record<string, number> = {};
  for (let d = 0; d < 120; d += 3) byDate[day(new Date(Date.UTC(2026, 5, 15 + d)))] = 1;
  const rows = Object.keys(byDate).slice(0, 12).map((date, i) => person(i, date));
  const sources = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu', 'genshokumaeshokukaranotenshokuriyuu', 'ouboriyuukategori_hiaringu', 'ouboriyuukategori_baitaikisai'];
  const source_counts = Object.fromEntries(sources.map(name => [name, name === 'ouboriyuu_hiaringu' ? { missing: 0, blank: 39, nonblank: 1 } : { missing: 0, blank: 40, nonblank: 0 }]));
  const reasons = { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: '2026-10-14T09:00:00+09:00', total_applicants: 40, total_source_values: 240, source_counts, items: [{ id: '1'.repeat(64), applicant: rows[0]?.key, text: '家から近いので応募しました', source: 'hubspot', source_property: 'ouboriyuu_hiaringu', application_date: rows[0]?.date, collected_at: null, version_id: null }], selections: [], option_labels: 'unavailable', missing: 0, blank: 239, truncated: false };
  await open(page, { total: 40, missing_date: 0, by_date: byDate, applications: rows, applicant_reasons: reasons });
  const events = page.locator('.lo-events');
  await expect(events).toContainText('掲載を開始');
  await expect(events).toContainText('HubSpot に本文を記録しはじめた');
  await expect(page.getByText('応募を取得した日が分からない')).toHaveCount(0);
  await expect(events).not.toContainText('応募の件数が分かりません');
  await expect(page.getByTestId('lifeline')).toContainText('応募数（Indeedの集計）');
  await expect(events).toContainText('掲載が復活');
  await events.getByRole('button', { name: /男性/ }).last().click();
  await expect(page.getByTestId('lifeline-selected')).toBeVisible();
  await expect(events).toContainText('家から近いので応募しました');
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/events-with-applications.png`, fullPage: true });
});

test('応募が0件の求人は、分からないではなく0件で出す', async ({ page }) => {
  await open(page, { total: 0, missing_date: 0, by_date: {} });
  await expect(page.getByTestId('listing-reasons')).toContainText('応募理由の記録はまだありません');
  await expect(page.getByTestId('lifeline')).toContainText('¥0');
  await expect(page.getByTestId('rail-last-application')).toHaveText('まだ応募なし');
  await expect(page.getByText('応募を取得した日が分からない')).toHaveCount(0);
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/events-zero-applications.png`, fullPage: true });
});

test('応募を取得できなかったときだけ、そう言う', async ({ page }) => {
  await open(page, 'fail');
  await expect(page.locator('.lo-card').first()).toContainText('応募を取得できませんでした');
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/events-applications-failed.png`, fullPage: true });
});

test('30週以上の求人でも出る', async ({ page }) => {
  await open(page, { total: 3, missing_date: 0, by_date: { '2026-07-01': 1, '2026-09-02': 2 } }, {}, 34);
  await expect(page.locator('.lo-events')).toContainText('掲載を開始');
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/events-long.png`, fullPage: true });
});
