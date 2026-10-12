import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';
const shots = 'docs/screenshots/job-copy-status';
for (const width of [1440, 1920]) test(`公開状況・空白だけの本文比較・任意項目 ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 742 });
  if (!process.env.JOB_COPY_STATUS_LOCAL) await login(page);
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  const rows = ['掲載中', '掲載終了', '掲載前', '不明'].map((status, index) => ({ id: String(index + 1), media: index === 0 ? 'hrh' : 'airwork', media_job_id: 'hidden-media-key', account_id: null, title: index === 0 ? '架空のピッキング作業スタッフ' : `架空の出荷スタッフ${index}`, prefecture: '東京都', municipality: '江東区', category: null, publication_status: status, last_csv_detected_at: null, application_count: null }));
  const body = (holiday: string) => `案件名：${rows[0]!.title}\n仕事内容：注文の商品を棚から集めます。\n給与形態：時給\n基本給与 最小：1200\n勤務時間：9:00〜18:00\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：${holiday}`;
  let single = false;
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/listings')) {
      const status = url.searchParams.get('publication');
      const listings = rows.filter(row => !status || row.publication_status === (status === 'unknown' ? '不明' : status));
      return route.fulfill({ json: { status: 'ready', listings, total: listings.length, offset: 0, next_offset: null, titles: [], publication_statuses: ['掲載中', '掲載終了', '掲載前', '不明'], index_built_at: '2026-10-12T00:00:00Z', refreshing: false, refresh_failed: false } });
    }
    if (url.pathname.endsWith('/versions')) return route.fulfill({ json: { listing: rows[0], current_fields: { 勤務地: '東京都江東区' }, versions: [
      ...(!single ? [{ written_at: '2026-10-01T00:00:00Z', body: body('土日休み。\n夏季休暇あり。'), image_urls: null }] : []),
      { written_at: '2026-10-12T00:00:00Z', body: body('\n 土日休み。　\r\n夏季休暇あり。\n'), image_urls: null },
    ], history_counts: {}, history_may_be_incomplete: false } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  const shot = async (name: string) => { if (width === 1440) { await mkdir(shots, { recursive: true }); await page.screenshot({ path: `${shots}/${name}.png` }); } };
  await page.goto('/app/job-copy?listing=1');
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await panel.getByText('絞り込み・並び順', { exact: true }).click();
  const selector = panel.getByLabel('公開状況', { exact: true });
  await expect(selector.locator('option')).toHaveText(['すべて', '不明', '掲載中', '掲載終了', '掲載前']);
  await expect(panel.locator('.jc-publication')).toHaveText(['掲載中', '掲載終了', '掲載前', '不明']);
  await panel.getByText('絞り込み・並び順', { exact: true }).click();
  await shot('01-list-statuses');
  await panel.getByText('絞り込み・並び順', { exact: true }).click();
  for (const status of ['掲載中', '掲載終了', '掲載前', 'unknown']) {
    await selector.selectOption(status);
    await expect(panel.locator('.jc-hubspot-job')).toHaveCount(1);
    await expect(panel.locator('.jc-publication')).toHaveText(status === 'unknown' ? '不明' : status);
    if (status === '掲載前') await shot('01-filter-before-publication');
  }
  await selector.selectOption('');
  await expect(panel.locator('.jc-hubspot-job')).toHaveCount(4);
  await expect(page.locator('.jc-detail-heading')).toContainText('公開状況：掲載中');
  await expect(page.locator('.jc-detail-heading')).toContainText('本文：前の本文と同じ');
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.locator('.jc-listing-popup-summary > div').filter({ has: page.locator('dt', { hasText: '公開状況' }) }).locator('dd')).toHaveText('掲載中');
  await expect(dialog.getByRole('checkbox', { name: '変わった項目だけ表示（0項目）' })).toBeVisible();
  await expect(dialog.locator('.jc-listing-change-dot')).toHaveCount(0);
  await shot('02-popup-summary');
  const holiday = dialog.locator('.jc-listing-comparison-row').filter({ has: page.getByRole('heading', { name: '休日・休暇', exact: true }) }).first();
  await holiday.scrollIntoViewIfNeeded();
  await expect(holiday).toHaveAttribute('data-changed', 'false');
  await expect(holiday.locator('mark')).toHaveCount(0);
  await shot('03-whitespace-no-change');
  const missing = dialog.locator('.jc-listing-popup-missing');
  await expect(missing.locator('summary')).toHaveText(['記載のない任意項目（68件）', '記載のない任意項目（68件）']);
  await expect(dialog).not.toContainText('任意項目の記載状況を取得できていません');
  await missing.first().locator('summary').scrollIntoViewIfNeeded();
  await shot('04-hrh-missing-optional');
  await missing.first().locator('summary').click();
  await expect(missing.first().locator('li')).toHaveCount(68);

  await page.keyboard.press('Escape');
  single = true;
  await page.reload();
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  await expect(dialog.locator('.jc-listing-two-columns')).toHaveCount(0);
  await expect(dialog).toContainText('本文以外の補足条件・公開状況は現在の取得値です');
  await expect(dialog).not.toContainText('左の本文');
  await shot('05-single-column-note');
});

for (const width of [1440, 1920]) test(`AirWorkの改行コードと空白だけの違いは上部とポップアップで同じ判定 ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 742 });
  if (!process.env.JOB_COPY_STATUS_LOCAL) await login(page);
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  const listing = { id: '1', media: 'airwork', media_job_id: 'hidden-media-key', account_id: null, title: '架空の出荷準備スタッフ', prefecture: '東京都', municipality: '江東区', category: null, publication_status: '掲載中', last_csv_detected_at: null, application_count: null };
  const before = '棚から集めます。\r\n箱に詰めます。';
  let after = '棚から集めます。\n箱に詰めます。';
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [listing], total: 1, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-12T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) return route.fulfill({ json: { listing, versions: [
      { written_at: '2026-10-01T00:00:00Z', body: before, image_urls: null },
      { written_at: '2026-10-12T00:00:00Z', body: after, image_urls: null },
    ], history_counts: {}, history_may_be_incomplete: false } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  for (const text of ['棚から集めます。\n箱に詰めます。', '\n 棚から集めます。　\r\n箱に詰めます。\n']) {
    after = text;
    await page.goto('/app/job-copy?listing=1');
    await expect(page.locator('.jc-detail-heading .jc-badge')).toHaveText('本文：前の本文と同じ');
    if (width === 1440 && text === '棚から集めます。\n箱に詰めます。') {
      await mkdir(shots, { recursive: true });
      await page.screenshot({ path: `${shots}/06-airwork-body-summary.png` });
    }
    await page.getByRole('button', { name: '求人の内容を確認する' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('checkbox', { name: '変わった項目だけ表示（0項目）' })).toBeVisible();
    await expect(dialog.locator('.jc-listing-change-dot, mark')).toHaveCount(0);
    expect(await dialog.locator('.jc-listing-comparison-row').first().locator('p').first().textContent()).toBe(after);
    if (width === 1440 && text === '棚から集めます。\n箱に詰めます。') {
      await mkdir(shots, { recursive: true });
      await page.screenshot({ path: `${shots}/07-airwork-line-endings-popup.png` });
    }
    await page.keyboard.press('Escape');
  }
  after = '棚から集めます。\n箱に詰め、伝票を確認します。';
  await page.goto('/app/job-copy?listing=1');
  await expect(page.locator('.jc-detail-heading .jc-badge')).toHaveText('本文：前の本文から内容が変わっています');
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('checkbox', { name: '変わった項目だけ表示（1項目）' })).toBeVisible();
  await expect(dialog.locator('.jc-listing-change-dot')).toHaveCount(2);
});
