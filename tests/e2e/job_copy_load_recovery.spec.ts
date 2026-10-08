/** Synthetic same-origin frontend recovery fixtures, not production login or live CRM access. */
import { expect, test, type Route } from '@playwright/test';
import { jobFeaturePanel, selectJobFeature } from './job-copy-navigation';

const snapshot = (total = 2) => {
  const capturedAt = '2026-10-06T00:00:00Z';
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-recovery', hubspotListingId: '30', title: `合成の復帰確認${String(total)}`, company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県', body: '再取得した合成の全文です。', images: [] }] }, results: [{ listing_id: '30', summary: { total, missing_date: 0, by_date: { '2026-10-05': total }, dimensions: { gender: { 男性: total } } }, dated_comparison: null }] };
};
const json = (value: unknown, status = 200) => ({ status, contentType: 'application/json', body: JSON.stringify(value) });

test('503 retries into two real-response applications, while 401 keeps the login action', async ({ page }) => {
  let attempts = 0;
  await page.route('**/api/job-copy/moc', route => route.fulfill(++attempts === 1 ? json({ code: 'moc_drive_snapshot_unavailable' }, 503) : json(snapshot())));
  await page.goto('/app/job-copy');
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.locator('.jc-job')).toHaveCount(0);
  await page.getByRole('button', { name: '求人データを再取得', exact: true }).click();
  // 件数は上の帯の「取得した範囲 ⓘ」の中（閉じていても中身は DOM にある。2026-10-08 round 3）
  await expect(page.getByRole('region', { name: '実データの取得範囲', includeHidden: true })).toContainText('2応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
  await selectJobFeature(page, 'body');
  await expect(jobFeaturePanel(page, 'body').locator('.jc-body')).toHaveText('再取得した合成の全文です。');
  await selectJobFeature(page, 'applicants');
  await expect(page.getByRole('region', { name: '求人全体の実応募者構成', exact: true })).toContainText('応募2件');
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toHaveCount(0);
  expect(attempts).toBe(2);
  await page.unroute('**/api/job-copy/moc');
  await page.route('**/api/job-copy/moc', route => route.fulfill(json({ code: 'login_required' }, 401)));
  await page.reload();
  await expect(page.getByRole('link', { name: '再ログインする' })).toHaveAttribute('href', '/login');
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toBeVisible();
  await expect(page.locator('.jc-job')).toHaveCount(0);
});

test('slow snapshot retry preserves the new response when the old server handler finishes', async ({ page }) => {
  let held: Route | undefined;
  let attempts = 0;
  await page.route('**/api/job-copy/moc', async route => {
    if (++attempts === 1) { held = route; return; }
    await route.fulfill(json(snapshot()));
  });
  await page.goto('/app/job-copy');
  await expect.poll(() => attempts).toBe(1);
  await expect(page.locator('.jc-list-heading')).toContainText('取得中');
  await expect(page.locator('.jc-list-heading')).not.toContainText(/0\s*\/\s*0/);
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toBeVisible({ timeout: 8_000 });
  await expect(page.locator('.jc-job')).toHaveCount(0);
  await page.getByRole('button', { name: '求人データを再取得', exact: true }).click();
  const summary = page.getByRole('region', { name: '実データの取得範囲', includeHidden: true });
  await expect(summary).toContainText('2応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
  if (!held) throw new Error('The first synthetic request was not held');
  // Browser abort may reject this delivery. The component unit test additionally
  // models a transport that ignores abort and actually returns the late response.
  await held.fulfill(json(snapshot(9))).catch(() => undefined);
  await expect(summary).toContainText('2応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
  await expect(page.getByRole('heading', { name: '合成の復帰確認9', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toHaveCount(0);
  expect(attempts).toBe(2);
});

test('30-second timeout ends loading without demo data, then retry recovers', async ({ page }) => {
  let attempts = 0;
  const started = Date.now();
  await page.route('**/api/job-copy/moc', async route => {
    if (++attempts > 1) await route.fulfill(json(snapshot()));
  });
  await page.goto('/app/job-copy');
  const loading = page.getByRole('status').filter({ hasText: /求人一覧・本文・応募集計を読み込んでいます|読み込みに時間がかかっています/ });
  await expect(loading).toBeVisible();
  await expect(page.getByRole('alert')).toContainText('時間がかかっています', { timeout: 35_000 });
  expect(Date.now() - started).toBeGreaterThanOrEqual(30_000);
  await expect(loading).toHaveCount(0);
  await expect(page.locator('.jc-job')).toHaveCount(0);
  await page.getByRole('button', { name: '求人データを再取得', exact: true }).click();
  await expect(page.getByRole('region', { name: '実データの取得範囲', includeHidden: true })).toContainText('2応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toHaveCount(0);
});

test('manual media capture cancels pending snapshot and its slow timer without losing chosen body', async ({ page }) => {
  let held: Route | undefined;
  await page.route('**/api/job-copy/moc', route => { held = route; });
  await page.goto('/app/job-copy');
  await expect.poll(() => Boolean(held)).toBe(true);
  await page.getByRole('button', { name: 'データ取込', exact: true }).click();
  await page.getByText('媒体で取得した求人本文・画像を確認', { exact: true }).click();
  const capture = snapshot().capture_bundle;
  const job = capture.jobs[0];
  if (!job) throw new Error('Missing synthetic capture');
  job.title = '合成の手動取込';
  job.body = '手動取込の本文を維持します。';
  await page.getByLabel('媒体取得データを読み込む', { exact: true }).setInputFiles({ name: 'synthetic-capture.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(capture)) });
  await page.getByRole('button', { name: '取得データを表示', exact: true }).click();
  await selectJobFeature(page, 'body');
  const body = jobFeaturePanel(page, 'body').locator('.jc-body');
  await expect(body).toHaveText(job.body);
  if (!held) throw new Error('The synthetic request was not held');
  await held.fulfill(json(snapshot(9))).catch(() => undefined);
  await expect(body).toHaveText(job.body);
  const loading = page.getByRole('status').filter({ hasText: /求人一覧・本文・応募集計を読み込んでいます|読み込みに時間がかかっています/ });
  await expect(loading).toHaveCount(0);
  await page.waitForTimeout(5_100);
  await expect(loading).toHaveCount(0);
  await expect(body).toHaveText(job.body);
  await expect(page.getByRole('region', { name: '実データの取得範囲', includeHidden: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '求人データを再取得', exact: true })).toHaveCount(0);
});
