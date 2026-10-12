import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

// AirWork images are plain URLs from the Listing field, shown the same way as HRハッカー.
// The image host is answered by the test itself; nothing is fetched from the internet.
const photo = (n: number) => `https://cdn.arwrk.net/images/rct/synthetic/${String(n)}`;
const folder = 'docs/screenshots/job-copy-airwork-image-urls';

test('AirWorkの画像URL: 5枚の順序・画像なし・取得できない', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 742 });
  if (!process.env.JOB_COPY_AWIMG_LOCAL) await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const rows = ['架空の検品スタッフ', '架空の出荷スタッフ', '架空の整理スタッフ'].map((title, i) => ({ id: String(i + 1), media: 'airwork', media_job_id: 'synthetic-private-value', account_id: null, title, prefecture: '東京都', municipality: '江東区', category: null, publication_status: '公開中', last_csv_detected_at: null, application_count: null }));
  await page.route('https://cdn.arwrk.net/**', route => {
    const n = route.request().url().split('/').pop();
    return route.fulfill({ contentType: 'image/svg+xml', body: `<svg xmlns="http://www.w3.org/2000/svg" width="400" height="130"><rect width="400" height="130" fill="#dbeaf3"/><path d="M40 90h320M60 30h70v60H60zM160 30h70v60h-70zM260 30h70v60h-70z" fill="#8cbdcc" stroke="#386578" stroke-width="4"/><text x="100" y="117" fill="#24435a" font-size="16">職場の案内 ${n}（架空素材）</text></svg>` });
  });
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: rows, total: 3, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-12T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) {
      const row = rows.find(item => path.includes(`/${item.id}/`))!;
      const images = row.id === '1' ? { image_urls: [1, 2, 3, 4, 5].map(photo), images_registered: true } : row.id === '2' ? { image_urls: [], images_registered: false } : { image_urls: null };
      return route.fulfill({ json: { listing: row, versions: [{ written_at: '2026-10-11T00:00:00Z', body: '届いた商品を確認して、棚に並べます。\n手順は先輩がご案内します。', ...images }], current_images_registered: row.id === '1' ? true : row.id === '2' ? false : null, current_images: null, history_counts: {}, history_may_be_incomplete: false } });
    }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  const shot = async (name: string) => { await mkdir(folder, { recursive: true }); await page.screenshot({ path: `${folder}/${name}.png` }); };

  await page.goto('/app/job-copy?listing=1');
  const gallery = page.getByRole('region', { name: 'この本文に対応する掲載画像', exact: true });
  await expect(gallery.locator('img')).toHaveCount(5);
  for (let i = 0; i < 5; i += 1) await expect(gallery.locator('img').nth(i)).toHaveAttribute('src', photo(i + 1));
  await gallery.locator('img').nth(4).scrollIntoViewIfNeeded();
  await expect.poll(() => gallery.locator('img').evaluateAll(images => images.filter(image => (image as HTMLImageElement).naturalWidth > 0).length)).toBe(5);
  await shot('01-body-five-images');

  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog').first();
  await dialog.locator('.jc-listing-popup-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.getByRole('img')).toHaveCount(5);
  await expect(dialog.getByRole('img').nth(4)).toHaveAttribute('src', photo(5));
  await expect.poll(() => dialog.locator('img').evaluateAll(images => images.filter(image => (image as HTMLImageElement).naturalWidth > 0).length)).toBe(5);
  await shot('02-popup-five-images');

  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog).toContainText('この求人には画像がありません');
  await dialog.locator('.jc-listing-popup-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  await shot('03-popup-no-images');

  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog).toContainText('画像を取得できませんでした');
  await expect(dialog).not.toContainText('この求人には画像がありません');
  await page.keyboard.press('Escape');

  await page.goto('/app/job-copy?listing=2');
  await expect(page.getByText('この求人には画像がありません', { exact: true })).toBeVisible();
  await shot('04-body-no-images');
  await page.goto('/app/job-copy?listing=3');
  await expect(page.getByText('画像を取得できませんでした', { exact: true })).toBeVisible();
});
