import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 競合調査の結果を求人に記録し、案件管理の「競合の推移」に出ることを通しで確かめる。
 * - 競合調査のレポートは本物のサーバーが合成の CSV から作る（Google は使わない）。
 * - HubSpot は架空（この試験の中の入れ物 `stored`）。保存の要求を受けたら、画面が受け取ったレポートから
 *   月ごとの 1 件を写して入れ物に入れ、求人の読み取りがそれを返す。本物の HubSpot には何も送らない。
 * 求人・給与・件数はすべて合成の値。
 */
const listing = { id: '30', media: 'hrh', media_job_id: 'synthetic-hrh-30', account_id: null, title: '顧客A 施設長', prefecture: '大阪府', municipality: '大阪市', category: '施設長', publication_status: '掲載中', last_csv_detected_at: null, application_count: null };
const CSV = 'タイトル,会社名,勤務地,給与,雇用形態\n施設長,A社,大阪府大阪市,月給 25万円 ~ 30万円,正社員\n施設長,B社,大阪府大阪市,月給 35万円 ~ 40万円,正社員\n';

type Report = { meta: { title: string; employment_type: string | null; prefecture: string | null; municipality: string | null; unit: string; total_count: number; top_n_effective: number; salary_parsed_count: number }; excel: { salary_table: { label: string; values: (number | null)[] }[]; salary_counts: number[]; keyword_all: { word: string; count: number }[] }; popularity_split: { popular: number; super_popular: number } | null };
const jstNow = () => new Date(Date.now() + 9 * 3600_000).toISOString();

async function mockHubSpot(page: Page) {
  const stored: Record<string, unknown>[] = [];
  const saves: Record<string, unknown>[] = [];
  let report: { report_id: string; report: Report } | null = null;
  page.on('response', async response => {
    if (response.url().endsWith('/api/competitor/report') && response.ok()) report = await response.json() as { report_id: string; report: Report };
  });
  await page.route('**/api/job-copy/market*', route => route.fulfill({ json: { titles: [], prefectures: [], series: null } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', listings: [listing], titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, total: 1, index_built_at: '2026-11-16T00:00:00Z' } }));
  await page.route('**/api/job-copy/listings/30/versions*', route => route.fulfill({ json: {
    listing, versions: [{ written_at: '2026-10-01T00:00:00Z', body: '給与形態：月給\n基本給与 最小：280000\n基本給与 最大：320000', image_urls: null }],
    history_counts: { hrh_kyuujinhyou_honbun: 1 }, history_may_be_incomplete: false, competitor_monthly: stored,
  } }));
  await page.route('**/api/job-copy/listings/30/competitor-record', route => {
    const body = route.request().postDataJSON() as { report_id: string };
    if (route.request().method() !== 'PUT' || route.request().headers()['x-requested-with'] !== 'fetch' || !report || body.report_id !== report.report_id) {
      return route.fulfill({ status: 404, json: { code: 'competitor_report_not_found' } });
    }
    saves.push(body);
    const { meta, excel, popularity_split: split } = report.report;
    const row = (label: string) => excel.salary_table.find(item => item.label === label)?.values ?? [];
    const stats = (min: number, max: number) => ({ min_mean: row('平均値')[min] ?? null, min_median: row('中央値')[min] ?? null, max_mean: row('平均値')[max] ?? null, max_median: row('中央値')[max] ?? null });
    const now = jstNow();
    const month = now.slice(0, 7);
    const replaced = stored.some(item => item.month === month);
    const entry = {
      month, run_at: `${now.slice(0, 19)}+09:00`, by: 'e2e@f-a-c.co.jp',
      conditions: { title: meta.title, job_name: '施設長', employment_type: meta.employment_type, prefecture: meta.prefecture, municipality: meta.municipality, region_specified: true, top_n: meta.top_n_effective, source: 'Indeed SP の一覧CSV', salary_converted: false },
      count: { total: meta.total_count, salary_parsed: meta.salary_parsed_count, salary_median_n: excel.salary_counts[0] ?? null, popular: split?.popular ?? null, super_popular: split?.super_popular ?? null },
      salary: { unit: meta.unit, all: stats(0, 1), popular: stats(2, 3) },
      own_salary: { min: 28, max: 32 }, keywords_top5: excel.keyword_all.slice(0, 5).map(item => [item.word, item.count]), csv_url: null,
    };
    stored.splice(0, stored.length, ...stored.filter(item => item.month !== month), entry);
    return route.fulfill({ json: { listing_id: '30', listing_name: listing.title, month, replaced, months: stored.length } });
  });
  return { stored, saves };
}

test('競合調査を求人に記録すると、案件管理の「競合の推移」にその月の行が出る', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const hubspot = await mockHubSpot(page);
  await login(page);
  const monthNumber = String(Number(jstNow().slice(5, 7)));
  const monthJa = `${jstNow().slice(0, 4)}年${monthNumber}月`;

  // 1. 案件管理の求人のページ: まだ記録が無い。入口は求人の番号と地域を付けた競合調査の URL。
  await page.goto('/app/job-copy?listing=30');
  const card = page.getByTestId('competitor-history');
  await expect(card.getByRole('status')).toHaveText('まだ競合調査の記録はありません');
  const entry = card.getByRole('link', { name: /この求人で競合調査をする/ });
  const href = await entry.getAttribute('href');
  expect(href).toBe(`/app/competitor?${new URLSearchParams({ listing: '30', listing_name: '顧客A 施設長', prefecture: '大阪府', municipality: '大阪市' }).toString()}`);
  await expect(entry).toHaveAttribute('target', '_blank');

  // 2. 競合調査: 求人の地域が入った状態で開く。CSV から結果を作る。
  await page.goto(href ?? '');
  await expect(page.getByTestId('listing-target')).toContainText('求人「顧客A 施設長」の競合調査として開いています');
  await expect(page.locator('#prefecture')).toHaveValue('大阪府');
  await page.locator('#job-name').fill('施設長');
  await page.locator('input[name="include_google"]').uncheck();
  await page.locator('#csv-file').setInputFiles({ name: 'competitors.csv', mimeType: 'text/csv', buffer: Buffer.from(CSV) });
  await page.getByRole('button', { name: 'レポートを作成' }).click();
  await expect(page.getByTestId('result-summary')).toContainText('CSV重複排除後 2 件');

  // 3. 「この求人に記録する」: 保存先と月を確かめてから保存する。
  await page.getByRole('button', { name: 'この求人に記録する' }).click();
  await expect(page.getByTestId('record-confirm')).toContainText(`HubSpot の求人「顧客A 施設長」（番号 30）に、${monthNumber}月の競合調査として保存します。同じ月の記録は置き換わります。`);
  expect(hubspot.saves).toHaveLength(0);
  await page.getByRole('button', { name: '保存する' }).click();
  await expect(page.getByTestId('record-status')).toContainText(`求人「顧客A 施設長」に、${monthNumber}月の競合調査として保存しました。`);
  expect(hubspot.saves).toHaveLength(1);
  // 送るのはレポートの ID だけ（月・媒体はサーバーがレポートから決める）。
  expect(Object.keys(hubspot.saves[0] ?? {})).toEqual(['report_id']);
  expect(hubspot.stored).toHaveLength(1);

  // 4. 案件管理に戻ると、その月の行に件数・給与の中央値（下限）・自社の給与が出る。
  //    CSV は月給 25〜30 万円と 35〜40 万円の 2 件なので、下限の中央値は 30 万円。
  await page.goto('/app/job-copy?listing=30');
  const row = page.getByTestId(`competitor-row-${jstNow().slice(0, 7)}`);
  await expect(row.locator('th,td')).toHaveText([monthJa, '2件', '30万円（2件から）', '28万円', /.*/, '最初の記録']);
  await expect(page.getByTestId('competitor-history')).not.toContainText('まだ競合調査の記録はありません');
  // 出来事にも「競合調査」が出て、開くとその月の記録が見える。
  await page.getByRole('button', { name: /競合調査.*競合 2件・給与の中央値（下限） 30万円/ }).click();
  await expect(page.getByTestId('competitor-month')).toContainText('中央値 30万円（2件から） ／ 平均 30万円');
  await expect(page.getByTestId('competitor-month')).toContainText('地域：大阪府');
});
