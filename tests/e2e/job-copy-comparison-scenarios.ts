import { test, expect, type Page } from '@playwright/test';
import { selectJobFeature } from './job-copy-navigation';
import { demoHome } from '../../frontend/src/screens/job-copy/demoHome';
import type { HubSpotListing } from '../../frontend/src/screens/job-copy/hubspotListings';

const captured = '2026-10-16T03:00:00Z';
const image = 'https://example.invalid/job-comparison/fictional.svg';
const shift = (date: string, days: number) => new Date(Date.parse(date) + days * 86_400_000).toISOString().slice(0, 10);
const listing = (id: number, same = true): HubSpotListing => ({ id: String(id), media: 'hrh', media_job_id: String(12345000 + id), account_id: same ? 'fictional-account-a' : 'fictional-account-b', title: `架空の配送募集${String(id)}`, prefecture: '大分県', municipality: '大分市', category: null, indeed_title_state: 'unset', publication_status: id === 2 ? '掲載終了' : '掲載中', last_csv_detected_at: captured, application_count: 90 - id, listing_started_on: id === 1 ? '2026-09-01' : '2026-09-08', deal_ids: id <= 2 ? ['901'] : ['902'], summary: { occupation: '配送ドライバー', location: '大分県大分市', salary_kind: '月給', salary_min: id === 1 ? '250000' : '270000', salary_max: '300000', hours: '8:00〜17:00', holidays: '土日', body_changed_at: captured } });
const rows = [listing(1), listing(2), listing(3), ...Array.from({ length: 22 }, (_, i) => listing(i + 4)), listing(30, false), listing(31, false)];
function versions(row: HubSpotListing) {
  const id = Number(row.id);
  const body = `案件名：${row.title}\n仕事内容：地域の店舗に日用品を届けます。${id === 1 ? '同乗研修は2週間です。' : '同乗研修は4週間です。'}\nIndeed表示職種名：配送ドライバー\n給与形態：月給\n基本給与 最小：${id === 1 ? '250000' : '270000'}\n基本給与 最大：300000\n勤務時間：8:00〜17:00\n休日：土日`;
  return { listing: row, listing_started_on: row.listing_started_on, current_fields: Object.fromEntries(body.split('\n').map(line => line.split('：'))), versions: [{ written_at: captured, body, image_urls: [image] }], history_counts: {}, history_may_be_incomplete: false,
    indeed_weekly_perf: { schema_version: 1, source: 'hrhacker', job_id: row.media_job_id, captured_at: captured, rows: Array.from({ length: 8 }, (_, i) => ({ period_start: shift('2026-08-31', i * 7), period_end: shift('2026-09-06', i * 7), impressions: 1000, clicks: 50, cost_yen: id * 1000, applications: id })) } };
}
async function setup(page: Page, authenticate: (page: Page) => Promise<void>) {
  await page.clock.setFixedTime(new Date(captured));
  const requests: URL[] = []; const details: string[] = [];
  const state = { failDetail: false, failApplicants: false };
  await page.route('https://example.invalid/job-comparison/**', route => route.fulfill({ contentType: 'image/svg+xml', body: '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="280"><rect width="480" height="280" fill="#dbeafe"/><rect x="80" y="90" width="260" height="100" fill="#2563eb"/><circle cx="140" cy="205" r="24" fill="#334155"/><circle cx="300" cy="205" r="24" fill="#334155"/><text x="30" y="45" font-size="24" fill="#1e3a8a">架空の配送求人・掲載画像</text></svg>' }));
  await page.route('**/api/job-copy/**', async route => {
    const url = new URL(route.request().url()); const path = url.pathname;
    if (path.endsWith('/listings')) {
      requests.push(url); const q = url.searchParams;
      let candidates = q.has('account_id') ? rows.filter(r => r.account_id === q.get('account_id') && r.id !== q.get('exclude_listing')).sort((a, b) => Number(a.id) - Number(b.id)) : q.has('exclude_account_id') ? rows.filter(r => r.account_id !== q.get('exclude_account_id')).sort((a, b) => Number(b.id) - Number(a.id)) : [rows[0]];
      if (q.get('occupation')) candidates = candidates.filter(r => r?.summary?.occupation === q.get('occupation'));
      if (q.get('prefecture')) candidates = candidates.filter(r => r?.prefecture === q.get('prefecture'));
      if (q.get('q')) candidates = candidates.filter(r => r?.title?.includes(q.get('q')!));
      if (q.get('media')) candidates = candidates.filter(r => r?.media === q.get('media'));
      const offset = Number(q.get('offset') ?? 0); const size = Number(q.get('page_size') ?? 50);
      return route.fulfill({ json: { status: 'ready', total: candidates.length, listings: candidates.slice(offset, offset + size), offset, next_offset: offset + size < candidates.length ? offset + size : null, titles: [], deals: [], index_built_at: captured, refreshing: false, refresh_failed: false } });
    }
    const match = /\/listings\/(\d+)\/(versions|company|applicants|applications)/.exec(path);
    if (match) {
      const row = rows.find(r => r.id === match[1]); if (!row) return route.fulfill({ status: 404, json: {} });
      if (match[2] === 'versions') { details.push(row.id); if (row.id === '2' && state.failDetail) { await new Promise(resolve => setTimeout(resolve, 300)); return route.fulfill({ status: 503, json: {} }); } return route.fulfill({ json: versions(row) }); }
      if (match[2] === 'company') return route.fulfill({ json: { company_name: row.account_id === 'fictional-account-a' ? '架空配送A' : '架空配送B' } });
      if (match[2] === 'applications') return route.fulfill({ json: { listing_id: row.id, application_count: 3 } });
      if (state.failApplicants && row.id === '2') return route.fulfill({ status: 503, json: {} });
      return route.fulfill({ json: { listing_id: row.id, total: 3, missing_date: 0, by_date: { '2026-09-20': 3 }, fetched_at: captured, applications: [1, 2, 3].map(i => ({ key: String(i), date: '2026-09-20', gender: i === 1 ? '女性' : '男性', age: '32', prefecture: '大分県', municipality: '大分市' })) } });
    }
    if (path.endsWith('/home')) return route.fulfill({ json: demoHome });
    if (path.endsWith('/companies')) return route.fulfill({ json: { companies: Object.fromEntries(rows.map(r => [r.id, r.account_id === 'fictional-account-a' ? '架空配送A' : '架空配送B'])) } });
    if (path.endsWith('/moc')) return route.fulfill({ json: { schemaVersion: 1, capturedAt: captured, capture_bundle: { schemaVersion: 1, capturedAt: captured, jobs: [] }, results: [] } });
    return route.fulfill({ json: { status: 'ready', dates: {}, report: { count: 0, listings: [] }, jobs: [] } });
  });
  await authenticate(page);
  await page.goto('/app/job-copy?listing=1');
  await selectJobFeature(page, 'ab');
  const comparison = page.getByRole('region', { name: '求人比較', exact: true });
  await expect(comparison.getByRole('button', { name: '架空の配送募集2を比較に追加' })).toBeVisible();
  return { comparison, requests, details, state };
}
export function registerComparisonTests(authenticate: (page: Page) => Promise<void> = async () => {}) {
  test('same customer searches unopened jobs and pages twenty at a time', async ({ page }, info) => {
    await page.setViewportSize({ width: 1440, height: 1100 });
    const { comparison, requests, details } = await setup(page, authenticate);
    expect(requests.filter(u => u.searchParams.has('account_id')).at(-1)?.searchParams.get('sort')).toBe('same_account');
    expect(requests.filter(u => u.searchParams.has('account_id')).at(-1)?.searchParams.has('owner')).toBe(false);
    expect(details).not.toContain('2');
    await expect(comparison.locator('.ab-candidates > li')).toHaveCount(20);
    await page.getByRole('dialog', { name: 'ほかの分析', exact: true }).screenshot({ path: info.outputPath('same-customer.png') });
    await comparison.getByRole('button', { name: '次の20件' }).click();
    await expect(comparison.locator('.ab-candidates > li')).toHaveCount(4);
    expect(requests.at(-1)?.searchParams.get('offset')).toBe('20');
    await comparison.getByRole('button', { name: '架空の配送募集22を比較に追加' }).click();
    await expect(comparison.getByRole('region', { name: '成果の横並び比較' })).toContainText('110件');
    expect(details).toContain('22');
  });
  test('other customers start with body occupation and prefecture, and filters can be cleared', async ({ page }, info) => {
    await page.setViewportSize({ width: 1440, height: 1100 });
    const { comparison, requests } = await setup(page, authenticate);
    await comparison.getByRole('tab', { name: 'ほかの顧客', exact: true }).click();
    await expect(comparison.locator('.ab-candidates strong')).toHaveText(['架空の配送募集31', '架空の配送募集30']);
    const query = requests.at(-1)?.searchParams;
    expect(query?.get('occupation')).toBe('配送ドライバー'); expect(query?.has('title')).toBe(false);
    expect(query?.get('prefecture')).toBe('大分県'); expect(query?.get('exclude_account_id')).toBe('fictional-account-a');
    expect(query?.get('sort')).toBe('recent_applications');
    await expect(comparison.getByLabel('本文の職種')).toHaveValue('配送ドライバー');
    await page.getByRole('dialog', { name: 'ほかの分析', exact: true }).screenshot({ path: info.outputPath('other-customers.png') });
    await comparison.getByRole('button', { name: '絞り込みを外す' }).click();
    await expect.poll(() => requests.at(-1)?.searchParams.has('occupation')).toBe(false);
    expect(requests.at(-1)?.searchParams.has('prefecture')).toBe(false);
    await comparison.getByLabel('求人名の検索').fill('募集30');
    await expect(comparison.locator('.ab-candidates strong')).toHaveText(['架空の配送募集30']);
  });
  test('three columns share periods, retain content differences and collapsed applicant details', async ({ page }, info) => {
    await page.setViewportSize({ width: 1440, height: 1100 });
    const { comparison } = await setup(page, authenticate);
    await comparison.getByRole('button', { name: '架空の配送募集2を比較に追加' }).click();
    await comparison.getByRole('button', { name: '架空の配送募集3を比較に追加' }).click();
    const table = comparison.getByRole('region', { name: '成果の横並び比較' });
    await expect(table.getByRole('columnheader')).toHaveCount(4);
    await expect(table.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '応募数', exact: true }) }).getByRole('cell')).toHaveText(['5件', '10件', '15件']);
    await expect(table.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '表示数', exact: true }) }).getByRole('cell')).toHaveText(['5,000回', '5,000回', '5,000回']);
    await expect(table.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '応募単価', exact: true }) }).getByRole('cell')).toHaveText(['1,000円', '1,000円', '1,000円']);
    await expect(comparison.locator('.ab-difference-mark').first()).toHaveText('今の求人と違う');
    const composition = comparison.getByText('応募者構成（性別・年代・地域）', { exact: true }).locator('..');
    await expect(composition).not.toHaveAttribute('open');
    await comparison.getByText('比較する相手を探す', { exact: true }).click();
    await page.setViewportSize({ width: 1440, height: 1700 });
    await page.getByRole('dialog', { name: 'ほかの分析', exact: true }).screenshot({ path: info.outputPath('three-columns.png') });
    await info.attach('求人比較・3列', { path: info.outputPath('three-columns.png'), contentType: 'image/png' });
    await comparison.getByText(/^同じ項目・未取得/).click();
    const same = comparison.getByRole('region', { name: '同じ項目・未取得の横並び比較', exact: true });
    await same.scrollIntoViewIfNeeded();
    await expect(same.locator('img')).toHaveCount(3);
    await expect.poll(() => same.locator('img').evaluateAll(images => images.every(image => image instanceof HTMLImageElement && image.complete && image.naturalWidth === 480))).toBe(true);
    await page.getByRole('dialog', { name: 'ほかの分析', exact: true }).screenshot({ path: info.outputPath('three-columns-content.png') });
    await comparison.getByLabel('比べる期間').selectOption('recent');
    await expect(table.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '比べる期間', exact: true }) }).getByRole('cell')).toHaveText(['2026/09/19〜2026/10/16', '2026/09/19〜2026/10/16', '2026/09/19〜2026/10/16']);
    await comparison.getByText('応募者構成（性別・年代・地域）', { exact: true }).click();
    await composition.getByText('性別を比較', { exact: true }).click();
    await expect(composition.getByRole('table').first().getByRole('columnheader')).toHaveCount(4);
    await expect(composition.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '女性', exact: true }) })).toContainText('33.33%');
    await page.setViewportSize({ width: 390, height: 850 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
    expect(await table.evaluate(el => el.scrollWidth > el.clientWidth)).toBe(true);
    await table.screenshot({ path: info.outputPath('three-columns-mobile.png') });
    await table.getByRole('button', { name: '架空の配送募集2を外す' }).click();
    await expect(table.getByRole('columnheader')).toHaveCount(3);
  });
  test('fixed imported jobs use local candidates even when their data came from HubSpot', async ({ page }) => {
    const { requests } = await setup(page, authenticate);
    const fixed = rows.slice(0, 3);
    await page.route('**/api/job-copy/moc', route => route.fulfill({ json: {
      schemaVersion: 1, capturedAt: captured,
      capture_bundle: { schemaVersion: 1, capturedAt: captured, jobs: fixed.map(row => ({ id: `fixed-${row.id}`, hubspotListingId: row.id, shopId: row.account_id, title: row.title, company: '架空配送A', media: 'HRハッカー', mediaJobId: row.media_job_id, location: '大分県大分市', body: `架空の配送の本文${row.id}`, images: [] })) },
      results: fixed.map(row => ({ listing_id: row.id, summary: { total: 3, missing_date: 0, by_date: { '2026-09-20': 3 }, dimensions: {} }, dated_comparison: null })),
    } }));
    requests.length = 0;
    await page.goto('/app/job-copy?data=actual');
    await selectJobFeature(page, 'ab');
    const comparison = page.getByRole('region', { name: '求人比較', exact: true });
    await expect(comparison.locator('.ab-candidates strong')).toHaveText(['架空の配送募集2', '架空の配送募集3']);
    await comparison.getByRole('button', { name: '架空の配送募集2を比較に追加' }).click();
    await comparison.getByRole('button', { name: '架空の配送募集3を比較に追加' }).click();
    const table = comparison.getByRole('region', { name: '成果の横並び比較' });
    await expect(table.getByRole('columnheader')).toHaveCount(4);
    await expect(table.getByText('掲載開始日が分かりません', { exact: true })).toHaveCount(3);
    expect(requests).toHaveLength(0);
  });
  test('detail and applicant failures have loading and retry controls', async ({ page }) => {
    const { comparison, state } = await setup(page, authenticate);
    state.failDetail = true;
    await comparison.getByRole('button', { name: '架空の配送募集2を比較に追加' }).click();
    const table = comparison.getByRole('region', { name: '成果の横並び比較' });
    await expect(table).toContainText('読み込んでいます');
    await expect(table).toContainText('求人の内容を読み込めませんでした');
    state.failDetail = false; state.failApplicants = true;
    await table.getByRole('button', { name: '再取得', exact: true }).click();
    await expect(table).toContainText('応募者構成・理由を読み込めませんでした');
    state.failApplicants = false;
    await table.getByRole('button', { name: '再取得', exact: true }).click();
    await expect(table).not.toContainText('応募者構成・理由を読み込めませんでした');
  });
}
