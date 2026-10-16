import { test, expect } from '@playwright/test';
import { openOtherAnalysis, closeOtherAnalysis, selectJobFeature } from './job-copy-navigation';

test('求人選択・内容ポップアップ・分析を戻る／進むで復元し、再読込と二重pushをしない', async ({ page }) => {
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  await page.goto('/app/job-copy?demo=1');
  const start = page.url();
  const rows = page.locator('.jc-job');
  const initialCount = await page.evaluate(() => history.length);
  await rows.nth(1).click();
  const second = page.url();
  await expect(rows.nth(1)).toHaveAttribute('aria-pressed', 'true');
  await rows.nth(1).click();
  expect(await page.evaluate(() => history.length)).toBe(initialCount + 1);
  await page.evaluate(() => { (window as unknown as { historyTestMarker: string }).historyTestMarker = '保持'; });
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const popup = page.url();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.goBack();
  await expect(page).toHaveURL(second);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.goBack();
  await expect(page).toHaveURL(start);
  await expect(rows.nth(0)).toHaveAttribute('aria-pressed', 'true');
  await page.goForward();
  await expect(rows.nth(1)).toHaveAttribute('aria-pressed', 'true');
  await page.goForward();
  await expect(page).toHaveURL(popup);
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.goBack();
  await page.getByRole('region', { name: 'ほかの分析', exact: true }).getByRole('button', { name: 'タイムライン', exact: true }).click();
  const timeline = page.url();
  await page.getByRole('tab', { name: '求人内容', exact: true }).click();
  const body = page.url();
  await page.goBack();
  await expect(page).toHaveURL(timeline);
  await expect(page.getByRole('tab', { name: 'タイムライン', exact: true })).toHaveAttribute('aria-selected', 'true');
  await page.goForward();
  await expect(page).toHaveURL(body);
  await expect(page.getByRole('tab', { name: '求人内容', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect(await page.evaluate(() => (window as unknown as { historyTestMarker: string }).historyTestMarker)).toBe('保持');
});

test('再読み込みで復元した分析を操作でき、閉じるとURLから消えて再び開かない', async ({ page }) => {
  await page.goto('/app/job-copy?demo=1&analysis=1&feature=body');
  const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
  await expect(dialog.getByRole('tab', { name: '求人内容', exact: true })).toHaveAttribute('aria-selected', 'true');
  const length = await page.evaluate(() => history.length);
  await openOtherAnalysis(page, '変更差分');
  await expect(dialog.getByRole('tab', { name: '変更差分', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect(await page.evaluate(() => history.length)).toBe(length + 1);
  await page.reload();
  // 読込が始まった直後に呼んでも、背後ではなく復元済みドロワーのタブを使う。
  await selectJobFeature(page, 'diff');
  await expect(dialog.getByRole('tab', { name: '変更差分', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect(await page.evaluate(() => history.length)).toBe(length + 1);
  await closeOtherAnalysis(page);
  expect(new URL(page.url()).searchParams.has('analysis')).toBe(false);
  expect(new URL(page.url()).searchParams.has('drawer')).toBe(false);
  expect(new URL(page.url()).searchParams.has('drawerEvent')).toBe(false);
  await page.reload();
  await expect(page.getByRole('region', { name: 'ほかの分析', exact: true })).toBeVisible();
  await expect(dialog).toHaveCount(0);
});

test('ホーム→会社→案件→求人と案件内ポップアップを履歴で往復する', async ({ page }) => {
  const { demoHome } = await import('../../frontend/src/screens/job-copy/demoHome');
  const customer = { ...demoHome.home.customers[0]!, company_id: '11' };
  demoHome.home.customers = [customer];
  let homeReads = 0; let versionReads = 0;
  const row = { id: '1', media: 'hrh', media_job_id: 'synthetic-1', account_id: null, title: '合成の履歴確認求人', prefecture: null, municipality: null, category: null, publication_status: '公開', last_csv_detected_at: null, application_count: 2 };
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/home')) { homeReads++; return route.fulfill({ json: demoHome }); }
    if (path.endsWith('/company-deals')) return route.fulfill({ json: { company: { id: customer.company_id, name: customer.name }, deals: [{ id: '20', name: '合成の履歴確認案件', pipeline_id: '1', pipeline_label: '納品管理', stage_label: '契約中', active: true, current: true, created_at: null, contract_start: null, contract_end: null, listings_total: 1, listings_live: 1, recent_apps: 2, recent_cost: 0 }] } });
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [row], total: 1, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) { versionReads++; return route.fulfill({ json: { listing: row, versions: [{ written_at: '2026-10-11T00:00:00Z', body: '合成の求人内容です。', image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } }); }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy');
  const home = page.url();
  await expect(page.getByRole('button', { name: `${customer.name}を開く` })).toBeVisible();
  const initialHomeReads = homeReads;
  await page.getByRole('button', { name: `${customer.name}を開く` }).click();
  const company = page.url();
  await page.getByRole('button', { name: '合成の履歴確認案件 ›', exact: true }).click();
  const deal = page.url();
  await page.getByRole('table', { name: '取引の求人一覧表' }).getByRole('button', { name: '内容を見る', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('合成の求人内容です。');
  const popup = page.url();
  await page.goBack();
  await expect(page).toHaveURL(deal);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.goForward();
  await expect(page).toHaveURL(popup);
  await expect(page.getByRole('dialog')).toContainText('合成の求人内容です。');
  await page.goBack();
  await page.getByRole('table', { name: '取引の求人一覧表' }).getByRole('button', { name: row.title, exact: true }).click();
  await expect(page.getByRole('button', { name: '求人の内容を確認する' })).toBeVisible();
  const listing = page.url();
  const reads = versionReads;
  for (const url of [deal, company, home]) { await page.goBack(); await expect(page).toHaveURL(url); }
  await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
  expect(homeReads).toBe(initialHomeReads);
  for (const url of [company, deal, listing]) { await page.goForward(); await expect(page).toHaveURL(url); }
  await expect(page.getByRole('button', { name: '求人の内容を確認する' })).toBeVisible();
  expect(versionReads).toBe(reads);
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button', { name: '案件管理', exact: true }).click();
  await expect(page).toHaveURL(home);
  await page.goBack();
  await expect(page).toHaveURL(listing);
  await expect(page.getByRole('button', { name: '求人の内容を確認する' })).toBeVisible();
  // 会社・案件を含む求人URLを直接開いても、案件のパンくずから戻れる。
  await page.goto(listing);
  await expect(page.getByRole('button', { name: '求人の内容を確認する' })).toBeVisible();
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button').nth(2).click();
  await expect(page).toHaveURL(deal);
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
});
