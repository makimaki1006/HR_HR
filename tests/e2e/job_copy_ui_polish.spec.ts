import path from 'node:path';
import { test, expect } from '@playwright/test';
import { fixtureHistory, fixtureImage } from './job-copy-listings-fixture.mjs';
import { selectJobFeature } from './job-copy-navigation';
const shots = path.resolve(__dirname, '../../docs/screenshots/job-copy-ui-polish');
test.beforeEach(async ({ page }) => { await page.setViewportSize({ width: 1440, height: 900 }); });
async function wholeCards(page: import('@playwright/test').Page) {
  return page.locator('.jc-listings-panel .jc-job').evaluateAll(cards => cards.filter(card => {
    const box = card.getBoundingClientRect(); const list = card.parentElement!.getBoundingClientRect();
    return box.top >= Math.max(0, list.top) && box.bottom <= Math.min(window.innerHeight, list.bottom);
  }).length);
}
for (const [width, height] of [[1440, 900], [1440, 690], [1920, 900]]) {
  for (const pinned of [true, false]) {
    test(`${width}×${height} ${pinned ? '固定' : 'マウス'}: 選択前後で5枚以上、絞り込みは動かない`, async ({ page }) => {
      if (pinned) await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
      const calls: string[] = [];
      page.on('request', request => { if (request.url().includes('/api/job-copy/')) calls.push(request.url()); });
      await page.route('https://example.invalid/**', route => route.fulfill({ contentType: 'image/svg+xml', body: fixtureImage(false) }));
      await page.route('**/api/job-copy/listings/1/versions', route => route.fulfill({ json: { ...fixtureHistory('1'), company_name: '合成運輸株式会社' } }));
      await page.setViewportSize({ width, height });
      await page.goto('/app/job-copy');
      if (!pinned) await page.getByRole('button', { name: '求人一覧を開く' }).hover();
      await expect(page.getByRole('button', { name: 'HubSpot の求人', exact: true })).toHaveAttribute('aria-pressed', 'true');
      await expect(page.locator('.jc-hubspot-job')).toHaveCount(40);
      await expect.poll(() => wholeCards(page)).toBeGreaterThanOrEqual(5);
      expect(calls.some(url => url.endsWith('/moc'))).toBe(false);
      if (width === 1440 && height === 900 && pinned) await page.screenshot({ path: path.join(shots, 'after-list-1440.png') });
      const filters = page.locator('.jc-hubspot-filter-details');
      await expect(filters).not.toHaveAttribute('open');
      const top = (await filters.boundingBox())!.y;
      await page.locator('.jc-listings-panel .jc-list-scroll').evaluate(element => { element.scrollTop = 200; });
      expect((await filters.boundingBox())!.y).toBe(top);
      await page.locator('.jc-listings-panel .jc-list-scroll').evaluate(element => { element.scrollTop = 0; });
      await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
      await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
      await expect(page.locator('.jc-detail-heading')).toContainText('合成運輸株式会社 · 大分県大分市 · HRハッカー');
      await expect(page.locator('.jc-record-meta')).toContainText('2026/10/02 09:00 JST にHubSpotに記録された本文');
      await expect(page.locator('.jc-main')).not.toContainText(/保存を確認・掲載時刻不明|取得した版 1|取引先名未取得/);
      if (!pinned) await page.getByRole('button', { name: '求人一覧を開く' }).hover();
      await expect.poll(() => wholeCards(page)).toBeGreaterThanOrEqual(5);
      await expect(page.locator('.jc-listings-panel .jc-history-status')).toHaveCount(0);
      if (width === 1440 && height === 900 && pinned) await page.screenshot({ path: path.join(shots, 'after-detail-1440.png') });
    });
  }
}

test('会社名が未取得なら省略し、絞り込みの具体値を送る', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  await page.goto('/app/job-copy');
  await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
  await expect(page.locator('.jc-detail-heading p')).toHaveText('大分県大分市 · HRハッカー');
  await page.locator('.jc-hubspot-filter-details > summary').click();
  const request = page.waitForRequest(request => new URL(request.url()).searchParams.get('prefecture') === '沖縄県');
  await page.getByLabel('都道府県', { exact: true }).selectOption('沖縄県');
  expect(new URL((await request).url()).searchParams.get('prefecture')).toBe('沖縄県');
  await expect(page.locator('.jc-hubspot-job')).toHaveCount(13);
});

test('固定一覧の空項目は区切りごと省略し、近接する給与・本文の印を分ける', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  const capturedAt = '2026-10-09T07:09:00Z';
  const job = { id: 'synthetic-machine', hubspotListingId: '30', title: '特殊機械の組立スタッフ', company: '', location: '', media: 'HRハッカー', mediaJobId: 'synthetic', body: '仕事内容：特殊機械を組み立てます。', images: [], versions: [] };
  // Snapshot parser uses observations as the body history; the exact API shape is asserted in unit tests too.
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [job] }, results: [{ listing_id: '30', summary: { total: 0, missing_date: 0, by_date: {}, dimensions: {} }, dated_comparison: null }] } }));
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-fixed-list .jc-job')).toHaveCount(1);
  await expect(page.locator('.jc-fixed-list .jc-job')).not.toContainText('·');
  await expect(page.locator('.jc-detail-heading p')).toHaveText('HRハッカー');
  await selectJobFeature(page, 'timeline');
  const marker = page.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark');
  await expect(marker).toHaveCount(1);
  const markerBox = (await marker.boundingBox())!;
  const track = (await marker.locator('..').boundingBox())!;
  expect(markerBox.x).toBeGreaterThan(track.x + 20);
  expect(markerBox.x + markerBox.width).toBeLessThan(track.x + track.width - 20);
  await page.screenshot({ path: path.join(shots, 'after-timeline-1440.png') });
});

test('近い3つの日時でも給与「不明」と本文「同じ」が重ならない', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  const history = fixtureHistory('1');
  await page.route('**/api/job-copy/listings/1/versions', route => route.fulfill({ json: { ...history, versions: ['00:00', '00:01', '00:02'].map(time => ({ written_at: `2026-10-09T${time}:00Z`, body: '仕事内容：特殊機械を組み立てます。\n給与：不明', image_urls: [] })) } }));
  await page.goto('/app/job-copy');
  await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
  await selectJobFeature(page, 'timeline');
  const body = page.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark');
  await expect(body).toHaveCount(3);
  expect(await body.allTextContents()).toEqual(['最初', '同じ', '同じ']);
  for (const locator of [body, page.getByRole('group', { name: '給与', exact: true }).locator('.jt-salary-label')]) {
    const boxes = await locator.evaluateAll(elements => elements.map(element => element.getBoundingClientRect().toJSON()));
    expect(boxes).toHaveLength(3);
    for (let i = 1; i < boxes.length; i++) expect(boxes[i].top).toBeGreaterThanOrEqual(boxes[i - 1].bottom);
  }
  await expect(page.getByRole('group', { name: '給与', exact: true })).toContainText('不明');
  await page.screenshot({ path: path.join(shots, 'after-nearby-timeline-1440.png') });
});
