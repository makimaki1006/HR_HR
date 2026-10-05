/** Synthetic navigation fixtures: local frontend behavior, not production auth. */
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { test, expect, type Page } from '@playwright/test';
import { selectJobFeature } from './job-copy-navigation';

const visuals = resolve('data/job-copy-local/candidate-browser/navigation-visual');
const capturedAt = '2026-10-05T00:00:00Z';
function snapshot() {
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: ['A', 'B'].map(name => ({
    id: `synthetic-navigation-${name}`, hubspotListingId: name === 'A' ? '30' : '31', title: `合成タブ確認求人${name}`, company: '合成取引先', media: 'HRハッカー', mediaJobId: name === 'A' ? '12345678' : '87654321', location: '大分県大分市', body: `合成${name}の求人本文です。`, images: [],
    history: [{ id: `synthetic-navigation-${name}-previous`, capturedAt: '2026-10-01T00:00:00Z', body: `合成${name}の前回本文です。`, images: [] }],
  })) }, results: ['A', 'B'].map(name => ({ listing_id: name === 'A' ? '30' : '31', summary: { total: 2, missing_date: 0, by_date: { '2026-09-01': 1, '2026-10-01': 1 }, dimensions: { gender: { 男性: 1, 不明: 1 } } }, dated_comparison: null,
    applicant_reasons: { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: capturedAt, total_applicants: 2, total_source_values: 6, missing: 5, blank: 0, truncated: false,
      source_counts: { oubodouki: { missing: 1, blank: 0, nonblank: 1 }, ouboriyuu_baitaikisai: { missing: 2, blank: 0, nonblank: 0 }, ouboriyuu_hiaringu: { missing: 2, blank: 0, nonblank: 0 } },
      items: [{ id: (name === 'A' ? 'a' : 'b').repeat(64), text: '合成の内部原記録です。', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-01', collected_at: null, version_id: null }] },
  })) };
}
async function setup(page: Page, longBody = false) {
  const data = snapshot();
  if (longBody) for (const job of data.capture_bundle.jobs) job.body += '\n合成の長い仕事内容を確認するための段落です。'.repeat(200);
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) }));
  await page.route('**/api/job-copy/market*', route => {
    const url = new URL(route.request().url());
    const selected = url.searchParams.get('title') === '合成職種' && url.searchParams.get('prefecture') === '大分県';
    const months = Array.from({ length: 18 }, (_, index) => `${2025 + Math.floor(index / 12)}-${String(index % 12 + 1).padStart(2, '0')}`);
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ source: '合成市場レポート', titles: ['合成職種'], prefectures: ['大分県'], ctk_basis: '市場指標です。応募数ではありません。', series: selected ? { prefecture: '大分県', months, job_count: months.map((_, i) => 100 + i), ctk_count: months.map((_, i) => 300 + i), employer_count: months.map(() => 20), seekers_per_posting: months.map(() => 3) } : null }) });
  });
  await page.goto('/app/job-copy');
  await expect(page.locator('.jc-job')).toHaveCount(2);
}
test.beforeAll(() => { mkdirSync(visuals, { recursive: true }); });

test('reading actions remain reachable, restore tab focus and return to the filtered mobile list', async ({ page }) => {
  await setup(page, true);
  const actions = page.getByRole('navigation', { name: '求人の閲覧操作', exact: true });
  const jump = page.getByRole('button', { name: 'この版を前の版と比較する →', exact: true });
  await jump.scrollIntoViewIfNeeded();
  await expect(actions.getByRole('button', { name: '機能を切り替える', exact: true })).toBeInViewport();
  await actions.getByRole('button', { name: '機能を切り替える', exact: true }).click();
  await expect(page.getByRole('tab', { name: '本文・画像', exact: true })).toBeFocused();
  await expect(page.getByRole('tablist', { name: '求人管理の機能', exact: true })).toBeInViewport();
  await page.getByRole('searchbox').fill('NO_MATCH_AUDIT_839201');
  await expect(page.locator('.jc-detail h1')).toHaveText('一致する求人はありません');
  await expect(page.locator('.jc-job')).toHaveCount(0);
  await page.getByRole('button', { name: '検索条件をリセット', exact: true }).click();
  await expect(page.locator('.jc-job')).toHaveCount(2);
  await expect(page.getByRole('searchbox')).toHaveValue('');
  await expect(page.getByRole('searchbox')).toBeFocused();
  await page.setViewportSize({ width: 375, height: 850 });
  await page.getByRole('searchbox').fill('合成タブ確認求人A');
  await page.locator('.jc-job').first().click();
  await actions.getByRole('button', { name: '求人一覧に戻る', exact: true }).click();
  await expect(page.getByRole('heading', { name: '求人レコード', exact: true })).toBeFocused();
  await expect(page.getByRole('heading', { name: '求人レコード', exact: true })).toBeInViewport();
  await expect(page.getByRole('searchbox')).toHaveValue('合成タブ確認求人A');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  await page.setViewportSize({ width: 640, height: 850 });
  await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.evaluate(() => { document.documentElement.style.zoom = ''; });
  await page.setViewportSize({ width: 667, height: 375 });
  expect(await actions.evaluate(element => getComputedStyle(element).position)).toBe('static');
  await page.emulateMedia({ media: 'print' });
  await expect(actions).toBeHidden();
});

test('functional tabs isolate applicants, reasons, application trends, market graphs and tables while retaining same-job selections', async ({ page }) => {
  await setup(page);
  const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
  await expect(primary.getByRole('tab')).toHaveText(['求人内容', '応募分析', '市場分析', '比較・報告', 'データ取込']);
  await expect(primary.getByRole('tab', { name: '求人内容', exact: true })).toHaveAttribute('aria-selected', 'true');
  const panel = (name: string) => page.getByRole('tabpanel', { name, exact: true });
  await expect(panel('本文・画像').locator('.jc-body')).toHaveText('合成Aの求人本文です。');
  await selectJobFeature(page, 'applicants');
  await expect(panel('応募者構成').getByRole('region', { name: '求人全体の実応募者構成', exact: true })).toBeVisible();
  await expect(panel('応募者構成').getByRole('region', { name: '応募理由の記述比較', exact: true })).toHaveCount(0);
  await selectJobFeature(page, 'reasons');
  await expect(panel('応募理由').getByRole('region', { name: '応募理由の記述比較', exact: true })).toBeVisible();
  await panel('応募理由').getByLabel('理由の出典', { exact: true }).selectOption('ouboriyuu_hiaringu');
  await panel('応募理由').getByLabel('理由比較先').selectOption('synthetic-navigation-A-previous');
  await selectJobFeature(page, 'applications');
  await expect(panel('応募推移').getByRole('region', { name: 'この求人の応募推移', exact: true })).toBeVisible();
  await expect(panel('応募推移').getByTestId('jc-applications-monthly')).toHaveAttribute('data-chart-ready', 'true');
  await expect(panel('応募推移').getByTestId('jc-market-jobs')).toHaveCount(0);
  await selectJobFeature(page, 'market');
  await expect(panel('市場グラフ').locator('table')).toHaveCount(0);
  await expect(panel('市場グラフ').getByRole('region', { name: 'この求人の応募推移', exact: true })).toHaveCount(0);
  await page.getByLabel('比較する市場職種').selectOption('合成職種');
  await page.getByLabel('比較する都道府県').selectOption('大分県');
  const chart = panel('市場グラフ').getByTestId('jc-market-jobs');
  await expect(chart).toHaveAttribute('data-chart-ready', 'true');
  expect(await chart.evaluate(element => {
    const option = window.__echarts_getInstanceByDom?.(element)?.getOption() as { dataZoom?: unknown[] };
    return option.dataZoom?.length ?? 0;
  })).toBe(0);
  await page.getByLabel('市場グラフの表示期間').selectOption('12');
  await expect.poll(() => chart.evaluate(element => {
    const option = window.__echarts_getInstanceByDom?.(element)?.getOption() as { xAxis?: { data?: string[] }[] };
    return option.xAxis?.[0]?.data?.length;
  })).toBe(12);
  await selectJobFeature(page, 'market-table');
  await expect(panel('市場データ').locator('tbody tr')).toHaveCount(12);
  await expect(panel('市場データ').locator('tbody tr').first()).toContainText('2025-07');
  await expect(panel('市場データ').getByTestId('jc-market-jobs')).toHaveCount(0);
  await primary.getByRole('tab', { name: '求人内容', exact: true }).click();
  await primary.getByRole('tab', { name: '市場分析', exact: true }).click();
  await expect(page.getByRole('tablist', { name: '市場分析の表示', exact: true }).getByRole('tab', { name: '市場データ', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(panel('市場データ').locator('tbody tr')).toHaveCount(12);
  await selectJobFeature(page, 'market');
  await expect(page.getByLabel('比較する市場職種')).toHaveValue('合成職種');
  await expect(page.getByLabel('比較する都道府県')).toHaveValue('大分県');
  await expect(page.getByLabel('市場グラフの表示期間')).toHaveValue('12');
  await selectJobFeature(page, 'reasons');
  await expect(panel('応募理由').getByLabel('理由の出典', { exact: true })).toHaveValue('ouboriyuu_hiaringu');
  await expect(panel('応募理由').getByLabel('理由比較先')).toHaveValue('synthetic-navigation-A-previous');
  await selectJobFeature(page, 'market-table');
  await primary.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'navigation-desktop.png') });
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await panel('市場データ').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'market-data-mobile.png') });
  await page.locator('.jc-job').nth(1).click();
  await selectJobFeature(page, 'market');
  await expect(page.getByLabel('比較する市場職種')).toHaveValue('');
  await expect(page.getByLabel('比較する都道府県')).toHaveValue('');
});

test('tab keyboard Arrow/Home/End movement activates valid linked panels and inactive controls stay hidden', async ({ page }) => {
  await setup(page, true);
  const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
  const jump = page.getByRole('button', { name: 'この版を前の版と比較する →', exact: true });
  await jump.scrollIntoViewIfNeeded();
  expect(await primary.evaluate(element => { const bounds = element.getBoundingClientRect(); return bounds.bottom < 0 || bounds.top > window.innerHeight; })).toBe(true);
  await jump.click();
  const difference = page.getByRole('tablist', { name: '比較・報告の表示', exact: true }).getByRole('tab', { name: '変更差分', exact: true });
  await expect(difference).toBeFocused();
  await expect(difference).toBeInViewport();
  await expect(page.getByRole('tabpanel', { name: '変更差分', exact: true })).toBeVisible();
  await selectJobFeature(page, 'body');
  const content = primary.getByRole('tab', { name: '求人内容', exact: true });
  await content.focus();
  await page.keyboard.press('ArrowRight');
  const applications = primary.getByRole('tab', { name: '応募分析', exact: true });
  await expect(applications).toBeFocused();
  await expect(applications).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tabpanel', { name: '応募推移', exact: true })).toBeVisible();
  const secondary = page.getByRole('tablist', { name: '応募分析の表示', exact: true });
  await secondary.getByRole('tab', { name: '応募推移', exact: true }).focus();
  await page.keyboard.press('End');
  await expect(secondary.getByRole('tab', { name: '課金・クリック', exact: true })).toBeFocused();
  await expect(page.getByRole('tabpanel', { name: '課金・クリック', exact: true })).toBeVisible();
  await page.keyboard.press('Home');
  await expect(secondary.getByRole('tab', { name: '応募推移', exact: true })).toBeFocused();
  await applications.focus();
  await page.keyboard.press('End');
  await expect(primary.getByRole('tab', { name: 'データ取込', exact: true })).toBeFocused();
  await expect(page.getByRole('tabpanel', { name: '外部文面を確認', exact: true })).toBeVisible();
  await expect(page.getByLabel('受け取った文面')).toBeVisible();
  await page.keyboard.press('Home');
  await expect(content).toBeFocused();
  await expect(page.getByLabel('受け取った文面')).toBeHidden();
  await expect(page.getByRole('tabpanel', { name: '本文・画像', exact: true })).toBeVisible();
  for (let step = 0; step < 15; step++) {
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => {
      const pane = document.activeElement?.closest('[role="tabpanel"]');
      return !pane || (!pane.hasAttribute('hidden') && pane.getAttribute('aria-hidden') !== 'true' && getComputedStyle(pane).display !== 'none');
    })).toBe(true);
  }
  for (const tab of await primary.getByRole('tab').all()) {
    await tab.click();
    const activeSecondary = page.getByRole('tablist', { name: /の表示$/ }).getByRole('tab', { selected: true });
    const target = await activeSecondary.getAttribute('aria-controls');
    expect(target !== null).toBe(true);
    const linked = page.locator(`[id="${target}"]`);
    await expect(linked).toHaveAttribute('role', 'tabpanel');
    await expect(linked).toBeVisible();
    expect(await linked.getAttribute('aria-labelledby')).toBe(await activeSecondary.getAttribute('id'));
  }
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await primary.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'tabs-mobile.png') });
  await selectJobFeature(page, 'market');
  await page.getByLabel('比較する市場職種').selectOption('合成職種');
  await page.getByLabel('比較する都道府県').selectOption('大分県');
  await expect(page.getByTestId('jc-market-jobs')).toHaveAttribute('data-chart-ready', 'true');
  await selectJobFeature(page, 'report');
  await page.emulateMedia({ media: 'print' });
  await expect(primary).toBeHidden();
  await expect(page.getByRole('region', { name: '顧客報告と検証記録', exact: true })).toBeVisible();
  await expect(page.getByTestId('jc-market-jobs')).toBeHidden();
  await expect(page.getByLabel('受け取った文面')).toBeHidden();
});
