import { test, expect } from '@playwright/test';
import { selectJobFeature, jobFeaturePanel } from './job-copy-navigation';

test('HubSpotの求人を絞って履歴を比較し、固定一覧へ戻る', async ({ page }) => {
  const listing = { id: '10', media: 'hrh', media_job_id: 'HR-10', account_id: 'synthetic-shop', title: '合成配送求人', prefecture: '大分県', municipality: '大分市', category: 'ドライバー', publication_status: '公開中', last_csv_detected_at: null, application_count: 4 };
  await page.route('**/api/job-copy/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/versions')) return route.fulfill({ json: { listing, versions: [
      { written_at: '2026-10-01T00:00:00Z', body: '合成の旧本文', image_urls: [] },
      { written_at: '2026-10-02T00:00:00Z', body: '合成の新本文', image_urls: [] },
    ], history_counts: { hrh_kyuujinhyou_honbun: 2, hrh_kyuujinhyou_gazou: 1 }, history_may_be_incomplete: false } });
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { listings: [listing], titles: ['ドライバー'], status: 'ready', total: 1, index_built_at: '2026-10-10T00:00:00Z', offset: 0, next_offset: null, refreshing: false, refresh_failed: false } });
    if (url.pathname.endsWith('/market')) return route.fulfill({ json: { titles: [], prefectures: [], series: null } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/static/app/job-copy-preview.html?demo=1');
  const original = await page.locator('.jc-fixed-list .jc-job').count();
  expect(original).toBeGreaterThan(0);
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await panel.getByLabel('都道府県', { exact: true }).selectOption('大分県');
  await panel.getByLabel('媒体', { exact: true }).selectOption('hrh');
  await panel.getByRole('button', { name: '求人を取得' }).click();
  await expect(panel.getByText('応募 4件', { exact: true })).toBeVisible();
  await panel.getByLabel('職種の分類').selectOption('ドライバー');
  const request = page.waitForRequest(req => req.url().includes('/listings?') && new URL(req.url()).searchParams.get('title') === 'ドライバー');
  await panel.getByRole('button', { name: '求人を取得' }).click();
  expect(new URL((await request).url()).searchParams.get('prefecture')).toBe('大分県');
  await panel.getByRole('button', { name: '合成配送求人の版を見る' }).click();
  await expect(page.locator('.jc-fixed-list .jc-job')).toHaveCount(1);
  await expect(panel.getByRole('status')).toContainText('文面の版は2件');
  await expect(panel.getByRole('status')).not.toContainText('20件までの可能性');
  await selectJobFeature(page, 'body');
  await expect(jobFeaturePanel(page, 'body').getByText('合成の新本文', { exact: true })).toBeVisible();
  await selectJobFeature(page, 'diff');
  await expect(jobFeaturePanel(page, 'diff')).toContainText('合成の旧本文');
  await expect(jobFeaturePanel(page, 'diff')).toContainText('合成の新本文');
  await page.getByRole('button', { name: '固定一覧を表示' }).click();
  await expect(page.locator('.jc-fixed-list .jc-job')).toHaveCount(original);
});
