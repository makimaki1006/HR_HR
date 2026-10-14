import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';
import { openOtherAnalysis, closeOtherAnalysis } from '../job-copy-navigation';

for (const width of [1440, 1920]) test(`AirWork保存画像5枚と取得不可の表示 ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 742 });
  if (!process.env.JOB_COPY_AWIMG_LOCAL) await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const rows = ['架空の検品スタッフ', '架空の出荷スタッフ', '架空の整理スタッフ'].map((title, i) => ({ id: String(i + 1), media: 'airwork', media_job_id: 'synthetic-private-value', account_id: null, title, prefecture: '東京都', municipality: '江東区', category: null, publication_status: i === 1 ? '公開終了' : '公開中', last_csv_detected_at: null, application_count: null }));
  const externalMedia: string[] = [];
  page.on('request', request => { if (/arwrk\.net|googleapis\.com/.test(request.url())) externalMedia.push(request.url()); });
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/job-copy/image') {
      const slot = new URL(route.request().url()).searchParams.get('slot');
      return route.fulfill({ contentType: 'image/svg+xml', body: `<svg xmlns="http://www.w3.org/2000/svg" width="400" height="130"><rect width="400" height="130" fill="#dbeaf3"/><path d="M40 90h320M60 30h70v60H60zM160 30h70v60h-70zM260 30h70v60h-70z" fill="#8cbdcc" stroke="#386578" stroke-width="4"/><text x="100" y="117" fill="#24435a" font-size="16">職場の案内 ${slot}（架空素材）</text></svg>` });
    }
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: rows, total: 3, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-12T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) {
      const row = rows.find(item => path.includes(`/${item.id}/`))!;
      return route.fulfill({ json: { listing: row, versions: [{ written_at: '2026-10-11T00:00:00Z', body: '届いた商品を確認して、棚に並べます。\n手順は先輩がご案内します。', image_urls: null }], current_images: row.id === '1' ? { observed_at: '2026-10-12T00:00:00Z', image_urls: Array.from({ length: 5 }, (_, i) => `/api/job-copy/image?company_id=10&listing_id=1&manifest_id=synthetic_manifest_001&slot=${String(i + 1)}`) } : null, history_counts: {}, history_may_be_incomplete: false } });
    }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?listing=1');
  await openOtherAnalysis(page, '本文・画像');
  const gallery = page.getByRole('region', { name: '現在取得できる掲載画像', exact: true });
  await expect(gallery.locator('img')).toHaveCount(5);
  await expect(gallery.locator('img').nth(4)).toHaveAttribute('src', /slot=5$/);
  await gallery.locator('img').nth(4).scrollIntoViewIfNeeded();
  await expect.poll(() => gallery.locator('img').evaluateAll(images => images.filter(image => (image as HTMLImageElement).naturalWidth > 0).length)).toBe(5);
  const shot = async (name: string) => { if (width === 1440) { await mkdir('docs/screenshots/job-copy-airwork-images', { recursive: true }); await page.screenshot({ path: `docs/screenshots/job-copy-airwork-images/${name}.png` }); } };
  await shot('01-body-five-images');
  await closeOtherAnalysis(page);
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog').first();
  await dialog.locator('.jc-listing-popup-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.getByRole('img')).toHaveCount(5);
  await expect(dialog.getByRole('img').nth(4)).toHaveAttribute('src', /slot=5$/);
  await expect.poll(() => dialog.locator('img').evaluateAll(images => images.filter(image => (image as HTMLImageElement).naturalWidth > 0).length)).toBe(5);
  await shot('02-popup-five-images');
  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog).toContainText('掲載が終わっているため画像を取得できません');
  await dialog.locator('.jc-listing-popup-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  await shot('03-ended-unavailable');
  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog).toContainText('画像を取得できませんでした');
  await page.keyboard.press('Escape');
  await page.goto('/app/job-copy?listing=3');
  await openOtherAnalysis(page, '本文・画像');
  await expect(page.getByText('画像を取得できませんでした', { exact: true })).toBeVisible();
  expect(externalMedia).toEqual([]);
});
