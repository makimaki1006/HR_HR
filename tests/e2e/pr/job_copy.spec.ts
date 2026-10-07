import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 求人文面管理 (/app/job-copy) を Rust が配る React 画面で確かめる。
 * 求人と応募の取得 (/api/job-copy/moc) は Drive の保存データを読むので、PR の fixture には無い。
 * ここでは合成データを page.route で返し、市場 (/api/job-copy/market) も合成の月次データを返す。
 * 確かめること: 最初のタブがタイムライン、7 レーンの値、2 つの ECharts の描画完了、期間比較表の具体値、
 * 課金CSV を読み込むと課金レーンと期間比較表に入ること、取込と逆検索が主作業の外にあること。
 */
const capturedAt = '2026-08-20T00:00:00Z';
const months = Array.from({ length: 14 }, (_, index) => `${2025 + Math.floor((index + 6) / 12)}-${String((index + 6) % 12 + 1).padStart(2, '0')}`);

function snapshot() {
  return {
    schemaVersion: 1, capturedAt,
    capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{
      id: 'synthetic-pr-timeline', hubspotListingId: '30', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678',
      location: '大分県大分市', body: '仕事内容：合成の配送業務です。\n給与：月給250,000円〜280,000円\n休日：土日', images: [],
      history: [{ id: 'synthetic-pr-timeline-previous', capturedAt: '2026-07-01T00:00:00Z', body: '仕事内容：合成の配送業務です。\n給与：月給230,000円〜260,000円\n休日：土日', images: [] }],
    }] },
    results: [{ listing_id: '30', summary: { total: 6, missing_date: 1, by_date: { '2026-07-10': 2, '2026-07-25': 1, '2026-08-20': 2 }, dimensions: { gender: { 男性: 4, 不明: 2 } } }, dated_comparison: null }],
  };
}

async function open(page: Page) {
  const calls: string[] = [];
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(snapshot()) }));
  await page.route('**/api/job-copy/market*', route => {
    const url = new URL(route.request().url());
    calls.push(url.search);
    const selected = url.searchParams.get('title') === '配送ドライバー' && url.searchParams.get('prefecture') === '大分県';
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
      source: '合成の市場データ', titles: ['配送ドライバー', '倉庫作業'], prefectures: ['大分県', '福岡県'], ctk_basis: '合成の閲覧者指標です。応募数ではありません。',
      series: selected ? { prefecture: '大分県', months, job_count: months.map((_, index) => 100 + index * 10), ctk_count: months.map((_, index) => 600 + index), employer_count: months.map(() => 30), seekers_per_posting: months.map(() => 5) } : null,
    }) });
  });
  await login(page);
  await page.goto('/app/job-copy');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  return calls;
}

async function seriesLengths(page: Page, testId: string) {
  const chart = page.locator(`[data-testid="${testId}"][data-chart-ready="true"]`);
  await expect(chart).toHaveCount(1);
  return chart.evaluate(element => {
    const option = window.__echarts_getInstanceByDom?.(element as HTMLElement)?.getOption() as { series?: { data?: unknown[] }[] } | undefined;
    return option?.series?.map(series => series.data?.length ?? 0) ?? [];
  });
}

test.describe('求人文面管理のタイムライン', () => {
  test('最初のタブがタイムラインで、7 レーン・グラフ・期間比較表に具体値が出る', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    const calls = await open(page);
    const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
    await expect(primary.getByRole('tab')).toHaveText(['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']);
    await expect(primary.getByRole('tab', { name: 'タイムライン', exact: true })).toHaveAttribute('aria-selected', 'true');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    for (const lane of ['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場']) await expect(timeline.getByRole('group', { name: lane, exact: true })).toBeVisible();
    const salary = timeline.getByRole('group', { name: '給与', exact: true });
    await expect(salary).toContainText('月給23万〜26万円');
    await expect(salary).toContainText('月給25万〜28万円');
    // HRハッカー実績も課金CSVも無いので、課金は「未接続」で 0円 にしない
    await expect(timeline.getByRole('group', { name: '課金', exact: true })).toContainText('未接続');
    await expect(timeline.getByRole('group', { name: '課金', exact: true })).not.toContainText('0円');
    await expect(timeline).toContainText('応募日が分からない応募 1件 はグラフに含めていません');
    // 市場は求人タイトルと勤務地から自動で選び、選んだ値を見せる
    await expect(timeline.getByLabel('職種')).toHaveValue('配送ドライバー');
    await expect(timeline.getByLabel('都道府県')).toHaveValue('大分県');
    expect(calls.some(search => search.includes('title=%E9%85%8D%E9%80%81%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC'))).toBe(true);
    const applications = await seriesLengths(page, 'jt-applications');
    expect(applications.length).toBeGreaterThan(0);
    expect(applications[0]).toBeGreaterThan(0);
    const market = await seriesLengths(page, 'jt-market');
    expect(market.length).toBe(2);
    expect(market[0]).toBeGreaterThan(0);
    // 期間比較表: 取得日 07-01 と 08-20 で区切った 2 期間（応募は HubSpot 記録分）
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0).locator('td').nth(0)).toHaveText('50日');
    await expect(rows.nth(0).locator('td').nth(1)).toHaveText('3件');
    await expect(rows.nth(0).locator('td').nth(2)).toHaveText('0.06件/日');
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('未接続');
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('+4.5%（2026-07 220件 → 2026-08 230件）');
    await expect(rows.nth(1).locator('td')).toHaveText(['1日', '2件', '2.00件/日', '未接続', '同じ月の中（2026-08 230件）']);
    // 主作業の外: データ取込は閉じていて、逆検索は押すまで出ない。旧来の操作バーも無い。
    await expect(page.locator('.jc-data-import')).toBeHidden();
    await expect(page.locator('.jc-reverse-search')).toHaveCount(0);
    await expect(page.getByRole('button', { name: '求人一覧に戻る', exact: true })).toHaveCount(0);
    const firstView = await page.evaluate(() => ({ inner: window.innerHeight, list: document.querySelector('.jc-list')?.getBoundingClientRect().bottom ?? 0 }));
    expect(firstView.list).toBeGreaterThanOrEqual(firstView.inner - 2);
    await expect(page.locator('body')).not.toContainText(/効果|確実に|必ず|100%/u);
  });

  test('課金CSVを読み込むと課金レーンと期間比較表に入り、再読み込みで消えることを示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    const csv = '媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,12345678,2026-07-01,2026-07-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing')).toHaveText(['3.1万円']);
    await expect(timeline.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。', { exact: true })).toBeVisible();
    // 07-01〜07-31 の 31 日分 31,000円 のうち、1 つ目の期間（07-01〜08-19 の 50 日）に入るのは全額
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('3.1万円');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('この期間の課金データなし');
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
  });
});
