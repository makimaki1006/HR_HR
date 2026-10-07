/** Synthetic local frontend responses only; no production OIDC or external image access. */
import { test, expect } from '@playwright/test';
import { selectJobFeature } from './job-copy-navigation';

const imageUrl = '/api/job-copy/snapshot-image?listing_id=30&version=0&slot=1&image_hash=' + 'a'.repeat(64);
const imageBytes = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=', 'base64');
const capturedAt = '2026-10-06T00:00:00Z';
const snapshot = { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-loading', hubspotListingId: '30', title: '合成の画像遅延求人', company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県', body: '画像を待たずに仕事内容を確認できます。', images: [{ id: 'synthetic-image', url: imageUrl, caption: '合成の遅延画像', contentHash: 'a'.repeat(64) }] }] }, results: [{ listing_id: '30', summary: { total: 2, missing_date: 0, by_date: { '2026-10-05': 2 }, dimensions: { gender: { 男性: 1, 不明: 1 } } }, dated_comparison: null }] };

test.beforeEach(async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(snapshot) }));
});

test('30-second image response does not block full body, application counts or tab interaction', async ({ page }) => {
  let requestedAt = 0;
  let settled = false;
  await page.route(`**${imageUrl}`, async route => {
    requestedAt = Date.now();
    await new Promise(resolve => setTimeout(resolve, 30_000));
    await route.fulfill({ status: 200, contentType: 'image/png', body: imageBytes });
    settled = true;
  });
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'body');
  const bodyPanel = page.getByRole('tabpanel', { name: '本文・画像', exact: true });
  const picture = bodyPanel.getByRole('img', { name: '合成の遅延画像', exact: true });
  await expect(bodyPanel.locator('.jc-body')).toHaveText(snapshot.capture_bundle.jobs[0]!.body);
  await picture.scrollIntoViewIfNeeded();
  await expect.poll(() => requestedAt).toBeGreaterThan(0);
  await expect(bodyPanel.getByRole('status')).toContainText('画像を取得中');
  await expect(bodyPanel.getByRole('button', { name: '画像1を拡大: 合成の遅延画像' })).toBeDisabled();
  await selectJobFeature(page, 'applicants');
  const applications = page.getByRole('region', { name: '求人全体の実応募者構成', exact: true });
  await expect(applications).toContainText('応募2件');
  await expect(applications).toContainText('1件 (50.0%)');
  expect(settled).toBe(false);
  expect(Date.now() - requestedAt).toBeLessThan(30_000);
  await selectJobFeature(page, 'body');
  await expect(bodyPanel.locator('.jc-body')).toHaveText(snapshot.capture_bundle.jobs[0]!.body);
  await expect(bodyPanel.getByRole('status')).toContainText('画像を取得中');
  await expect.poll(() => picture.evaluate(image => (image as HTMLImageElement).naturalWidth), { timeout: 35_000 }).toBe(1);
  expect(Date.now() - requestedAt).toBeGreaterThanOrEqual(30_000);
  await expect(bodyPanel.getByRole('status')).toHaveCount(0);
  await expect(bodyPanel.getByRole('button', { name: '画像1を拡大: 合成の遅延画像' })).toBeEnabled();
});

test('image failure stays separate from zero and retry restores the same reference', async ({ page }) => {
  let attempts = 0;
  await page.route(`**${imageUrl}`, route => {
    attempts += 1;
    return attempts === 1 ? route.fulfill({ status: 503, body: '' }) : route.fulfill({ status: 200, contentType: 'image/png', body: imageBytes });
  });
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'body');
  const panel = page.getByRole('tabpanel', { name: '本文・画像', exact: true });
  await panel.locator('.jc-image-section').scrollIntoViewIfNeeded();
  await expect(panel.getByRole('alert')).toContainText('画像なし・削除とは判定していません');
  await expect(panel).not.toContainText('この観測版の画像は0点');
  await panel.getByRole('button', { name: '画像を再読み込み: 合成の遅延画像' }).click();
  await expect.poll(() => panel.getByRole('img', { name: '合成の遅延画像', exact: true }).evaluate(image => (image as HTMLImageElement).naturalWidth)).toBe(1);
  await expect(panel.getByRole('alert')).toHaveCount(0);
  await expect(panel.getByRole('button', { name: '画像1を拡大: 合成の遅延画像' })).toBeEnabled();
  expect(attempts).toBe(2);
});
