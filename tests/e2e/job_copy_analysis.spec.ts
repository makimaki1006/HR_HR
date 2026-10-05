import { selectJobFeature } from './job-copy-navigation';
import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
const visuals = resolve('data/job-copy-local/candidate-browser/analysis-visual');
test.beforeAll(() => { mkdirSync(visuals, { recursive: true }); });

function fixture() {
  const capturedAt = '2026-10-05T00:00:00Z';
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{
    id: 'synthetic-analysis', hubspotListingId: '30', title: '合成分析求人', company: '合成取引先', media: 'HRハッカー', mediaJobId: '01234567', location: '大分県大分市', body: '合成求人本文', images: [],
  }] }, results: [{ listing_id: '30', summary: { total: 4, missing_date: 0, by_date: { '2026-09-01': 4 }, dimensions: { gender: { 男性: 2, 女性: 2 }, age: { '20代': 3, '30代': 1 }, prefecture: { 大分県: 3, 福岡県: 1 }, municipality: { 大分市: 3, 福岡市: 1 } }, joint_demographics: { total: 4, cells: [
    { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分市', count: 1 },
    { gender: '女性', age: '20代', prefecture: '大分県', municipality: '大分市', count: 2 },
    { gender: '男性', age: '30代', prefecture: '福岡県', municipality: '福岡市', count: 1 },
  ] } }, dated_comparison: null, hrh_performance: { schema_version: 1, source: 'hrhacker', job_id: '01234567', captured_at: capturedAt, rows: [
    { period_start: '2026-09-01', period_end: '2026-09-10', impressions: 1000, clicks: 50, cost_yen: 10000, applications: 5 },
    { period_start: '2026-09-11', period_end: '2026-09-15', impressions: 500, clicks: 50, cost_yen: 15000, applications: 3 },
  ] } }] };
}

test('joined HR Hacker metrics show exact rates, period delta and meaningful joint reverse search', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'performance');
  const metrics = page.getByRole('region', { name: 'HRハッカー課金・クリック実績', exact: true });
  await expect(metrics).toContainText('01234567');
  const first = metrics.locator('tbody tr').first();
  await expect(first.locator('td').nth(2)).toHaveText('5%');
  await expect(first.locator('td').nth(4)).toHaveText('200円');
  await expect(metrics.getByLabel('実績期間の比較')).toHaveText('クリック率の差：5ポイント · 1日当たりクリック数の差：5件');
  await metrics.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'hrh-desktop.png') });
  await page.setViewportSize({ width: 375, height: 850 });
  await metrics.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  const scroll = metrics.getByRole('region', { name: '期間別実績の横スクロール', exact: true });
  expect(await scroll.evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
  await scroll.focus();
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => scroll.evaluate(element => element.scrollLeft)).toBeGreaterThan(0);
  await scroll.evaluate(element => { element.scrollLeft = 0; });
  await page.screenshot({ path: resolve(visuals, 'hrh-mobile.png') });
  await scroll.evaluate(element => { element.scrollLeft = element.scrollWidth; });
  await page.screenshot({ path: resolve(visuals, 'hrh-mobile-right.png') });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.getByText('応募者の条件から求人を探す（逆検索）', { exact: true }).click();
  await page.getByLabel('応募者の性別', { exact: true }).selectOption('男性');
  await page.getByLabel('応募者の年代', { exact: true }).selectOption('20代');
  await page.getByLabel('応募者の都道府県', { exact: true }).selectOption('大分県');
  await expect(page.getByLabel('逆検索の結果')).toContainText('該当1件 / この求人の全応募4件（25.0%）');
  await page.locator('.jc-reverse-search').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'joint-desktop.png') });
  await page.setViewportSize({ width: 375, height: 850 });
  await page.locator('.jc-reverse-search').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'joint-mobile.png') });
  await page.getByLabel('最低該当人数', { exact: true }).fill('2');
  await expect(page.getByRole('status')).toContainText('条件に一致する求人はありません');
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('market selection keeps period and source separate from applications and hides internal data for print', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.route('**/api/job-copy/market*', route => {
    const url = new URL(route.request().url());
    const selected = url.searchParams.get('title') === '合成職種' && url.searchParams.get('prefecture') === '大分県';
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ source: '合成市場レポート', titles: ['合成職種'], prefectures: ['大分県'], ctk_basis: 'Indeed行動データ。応募数ではありません。', series: selected ? { prefecture: '大分県', months: ['2026-08'], job_count: [100], ctk_count: [300], employer_count: [20], seekers_per_posting: [3] } : null }) });
  });
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'market');
  await page.getByLabel('比較する市場職種').selectOption('合成職種');
  await page.getByLabel('比較する都道府県').selectOption('大分県');
  const market = page.getByRole('region', { name: '市場環境と応募獲得の要因' });
  await expect(page.getByTestId('jc-market-jobs')).toHaveAttribute('data-chart-ready', 'true');
  expect(await page.getByTestId('jc-market-jobs').evaluate(el => {
    const option = window.__echarts_getInstanceByDom?.(el)?.getOption() as { series: { data: number[] }[] };
    return option.series[0]?.data;
  })).toEqual([100]);
  await selectJobFeature(page, 'applications');
  await expect(page.getByTestId('jc-applications-monthly')).toHaveAttribute('data-chart-ready', 'true');
  expect(await page.getByTestId('jc-applications-monthly').evaluate(el => {
    const option = window.__echarts_getInstanceByDom?.(el)?.getOption() as { series: { data: number[] }[] };
    return option.series[0]?.data;
  })).toEqual([4]);
  await selectJobFeature(page, 'market-table');
  await expect(market.getByRole('region', { name: '市場実績の数値表', exact: true }).locator('tbody tr')).toHaveText('2026-08100300203');
  await expect(market).toContainText('応募数ではありません');
  await market.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'market-desktop.png') });
  await page.setViewportSize({ width: 375, height: 850 });
  await market.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(await market.locator('.jc-analysis-table').evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
  await page.screenshot({ path: resolve(visuals, 'market-mobile.png') });
  await page.emulateMedia({ media: 'print' });
  await expect(market).toBeHidden();
});

test('market graphs retain missing months and real zero and hide stale charts during scope changes', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  let release: (() => void) | undefined;
  await page.route('**/api/job-copy/market*', async route => {
    const url = new URL(route.request().url());
    if (url.searchParams.get('prefecture') === '福岡県') await new Promise<void>(resolve => { release = resolve; });
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ source: '合成市場レポート', titles: ['合成職種'], prefectures: ['大分県', '福岡県'], ctk_basis: '応募数ではありません。', series: url.searchParams.get('prefecture') === '大分県' ? { prefecture: '大分県', months: ['2026-07', '2026-09'], job_count: [100, 0], ctk_count: [300, 100], employer_count: [20, null], seekers_per_posting: [3, null] } : null }) });
  });
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'market');
  await page.getByLabel('比較する市場職種').selectOption('合成職種');
  await page.getByLabel('比較する都道府県').selectOption('大分県');
  const chart = page.getByTestId('jc-market-jobs');
  await expect(chart).toHaveAttribute('data-chart-ready', 'true');
  expect(await chart.evaluate(el => {
    const o = window.__echarts_getInstanceByDom?.(el)?.getOption() as { xAxis: { data: string[] }[]; yAxis: { min: number; name: string }[]; series: { data: (number | null)[]; connectNulls: boolean }[] };
    return { months: o.xAxis[0]?.data, values: o.series[0]?.data, connectNulls: o.series[0]?.connectNulls, min: o.yAxis[0]?.min, unit: o.yAxis[0]?.name };
  })).toEqual({ months: ['2026-07', '2026-08', '2026-09'], values: [100, null, 0], connectNulls: false, min: 0, unit: '件' });
  await page.setViewportSize({ width: 375, height: 850 });
  await chart.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: resolve(visuals, 'market-graphs-mobile.png') });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.locator('.jc-market-charts').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'market-graphs-desktop.png') });
  await page.getByLabel('比較する都道府県').selectOption('福岡県');
  await expect(page.getByRole('status')).toContainText('市場データを取得中');
  await expect(page.getByTestId('jc-market-jobs')).toHaveCount(0);
  await expect(page.getByRole('tabpanel', { name: '市場グラフ', exact: true }).getByTestId('jc-applications-monthly')).toHaveCount(0);
  await expect.poll(() => Boolean(release)).toBe(true);
  release?.();
  await expect(page.getByText('選択した職種・県の月次市場データはありません。', { exact: false })).toBeVisible();
});
