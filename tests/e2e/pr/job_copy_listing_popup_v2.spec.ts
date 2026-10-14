import { test, expect } from '@playwright/test';
import { readFile, mkdir } from 'node:fs/promises';
import { login } from './helpers/login';
import { openOtherAnalysis, closeOtherAnalysis, findListings } from '../job-copy-navigation';
const shots = 'docs/screenshots/job-copy-listing-popup-v2';
for (const width of [1440, 1920]) test(`求人内容の前回比較・空欄・画像・矢印で切り替え ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 742 });
  if (!process.env.JOB_COPY_POPUP_LOCAL) await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const columns = JSON.parse(await readFile('frontend/src/generated/jobgen/columns.json', 'utf8')) as string[];
  const rows = [
    { id: '1', media: 'hrh', title: '日用品の検品スタッフ' },
    { id: '2', media: 'airwork', title: '出荷準備スタッフ' },
    { id: '3', media: 'hrh', title: '倉庫内の整理スタッフ' },
  ].map(row => ({ ...row, media_job_id: 'hidden-media-id', account_id: null, prefecture: '東京都', municipality: '江東区', category: null, publication_status: '公開', last_csv_detected_at: null, application_count: Number(row.id) * 12 }));
  const fields = { 案件名: rows[0]!.title, 仕事内容: '日用品を確認し、伝票に沿って梱包します。\n先輩が手順をご案内します。', 給与形態: '月給', '基本給与 最小': '213000', '基本給与 最大': '249000', 勤務時間: '9:00〜18:00', 応募資格: '', 自由項目1のタイトル: '休日・休暇', 自由項目1の内容: '土日休み', 自由項目2のタイトル: '福利厚生', 自由項目2の内容: '社会保険・交通費支給', 画像1: 'warehouse.jpg' };
  const body = (values: Record<string, string>) => columns.map(key => `${key}：${values[key] ?? ''}`).join('\n');
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: rows, total: 3, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) {
      const row = rows.find(item => path.includes(`/${item.id}/`))!;
      const aw = row.media === 'airwork';
      return route.fulfill({ json: { listing: row, current_fields: { 勤務地: '東京都江東区' }, current_images_registered: row.id === '3' ? false : true,
        versions: row.id === '3' ? [{ written_at: '2026-10-11T00:00:00Z', body: body({ 案件名: row.title, 仕事内容: '倉庫内を整理します。' }), image_urls: [], images_registered: false }] : [
          { written_at: '2026-10-01T00:00:00Z', body: aw ? '荷物を受け取ります。' : body({ ...fields, 仕事内容: '日用品を確認し、伝票に沿って出荷します。\n先輩が手順をご案内します。', 勤務時間: '8:00〜17:00' }), image_urls: [], images_registered: true },
          { written_at: '2026-10-11T00:00:00Z', body: aw ? '出荷する荷物を確認します。' : body(fields), image_urls: [] },
        ], current_images: row.id === '3' ? null : { observed_at: '2026-10-11T01:00:00Z', image_urls: ['/popup-workplace.svg'] }, history_counts: {}, history_may_be_incomplete: false } });
    }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.route('**/popup-workplace.svg', route => route.fulfill({ contentType: 'image/svg+xml', body: '<svg xmlns="http://www.w3.org/2000/svg" width="520" height="180"><rect width="520" height="180" fill="#eaf5f7"/><path d="M50 120h420M80 50h80v70H80zM200 50h80v70h-80zM320 50h80v70h-80z" stroke="#357d87" stroke-width="8" fill="#c2e0e5"/><text x="165" y="158" font-size="18" fill="#25394c">職場の案内画像（架空）</text></svg>' }));
  await page.goto('/app/job-copy?listing=1');
  await findListings(page);
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('1 / 3件目');
  await expect(dialog.getByRole('button', { name: '‹ 前の求人' })).toBeDisabled();
  await expect(dialog.locator('.jc-listing-two-columns')).toHaveCount(1);
  await expect(dialog.locator('mark').first()).toHaveText('梱包');
  const bounds = (await dialog.boundingBox())!;
  expect(bounds.width).toBe(width * 0.95); expect(Math.abs(bounds.height / 742 - 0.9)).toBeLessThan(0.01);
  const pair = dialog.locator('.jc-listing-comparison-row').first().locator('section');
  expect((await pair.nth(0).boundingBox())!.y).toBe((await pair.nth(1).boundingBox())!.y);
  const shot = async (name: string) => { if (width === 1440) { await mkdir(shots, { recursive: true }); await page.screenshot({ path: `${shots}/${name}.png` }); } };
  await shot('v2-01-two-columns');
  await dialog.getByRole('checkbox', { name: /変わった項目だけ表示/ }).check();
  await expect(dialog.locator('.jc-listing-comparison-row')).toHaveCount(2);
  await shot('v2-02-changes-only');
  await dialog.getByRole('checkbox', { name: /変わった項目だけ表示/ }).uncheck();
  const scroller = dialog.locator('.jc-listing-popup-scroll');
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.getByRole('img', { name: '現在の画像1', exact: true })).toBeVisible();
  await shot('v2-03-images-and-closed-optional');
  const missing = dialog.locator('.jc-listing-popup-missing').first();
  await expect(missing).not.toHaveAttribute('open', '');
  await missing.locator('summary').click();
  await expect(missing.locator('li')).toHaveCount(66);
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await shot('v2-04-open-optional');
  await page.keyboard.press('ArrowRight');
  await expect(dialog.getByRole('heading', { name: '出荷準備スタッフ', exact: true })).toBeVisible();
  await expect(dialog).toContainText('出荷する荷物を確認します。');
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(dialog.getByRole('img', { name: '現在の画像1', exact: true })).toBeVisible();
  await shot('v2-08-airwork-images');
  await page.keyboard.press('ArrowLeft');
  await expect(dialog.getByRole('heading', { name: rows[0]!.title, exact: true })).toBeVisible();
  await expect(scroller).toHaveJSProperty('scrollTop', 0);
  await shot('v2-05-arrow-left');
  const beforeScroll = await scroller.evaluate(element => element.scrollTop);
  await page.keyboard.press('ArrowDown');
  await expect.poll(() => scroller.evaluate(element => element.scrollTop)).toBeGreaterThan(beforeScroll);
  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog).toContainText('2 / 3件目');
  await dialog.getByRole('button', { name: '次の求人 ›' }).click();
  await expect(dialog.getByRole('heading', { name: rows[2]!.title, exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '次の求人 ›' })).toBeDisabled();
  await expect(dialog.locator('.jc-listing-two-columns')).toHaveCount(0);
  await expect(dialog).toContainText('この求人には画像が登録されていません');
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await shot('v2-06-no-images');
  await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
  await page.getByRole('button', { name: `${rows[0]!.title}の本文を見る` }).click();
  await openOtherAnalysis(page, '本文・画像');
  await page.getByRole('button', { name: /2026\/10\/01 09:00 JST にHubSpotに記録された本文/ }).click();
  await closeOtherAnalysis(page);
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  await expect(dialog.locator('.jc-listing-two-columns')).toHaveCount(0);
  await expect(dialog).toContainText('画像はありますが、取得できていません');
  await scroller.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await shot('v2-07-images-unavailable');
  await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await panel.getByText('絞り込み・並び順', { exact: true }).click();
  await panel.getByLabel('並び順', { exact: true }).selectOption('applications');
  await expect(panel.locator('.jc-hubspot-job').first()).toContainText('倉庫内の整理スタッフ');
  await panel.getByRole('button', { name: '出荷準備スタッフの本文を見る' }).click();
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  await expect(dialog).toContainText('2 / 3件目');
  await page.keyboard.press('ArrowLeft');
  await expect(dialog.getByRole('heading', { name: '倉庫内の整理スタッフ', exact: true })).toBeVisible();
  await expect(dialog).toContainText('1 / 3件目');
  await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
});

for (const width of [1440, 1920]) test(`取得不能の給与・任意項目と短文・600文字の色付け ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 742 });
  if (!process.env.JOB_COPY_POPUP_LOCAL) await login(page);
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const listing = { id: '31', media: 'airwork', media_job_id: 'hidden-aw-id', account_id: null, title: '架空の作業スタッフ', prefecture: '東京都', municipality: '江東区', category: null, publication_status: '公開', last_csv_detected_at: null, application_count: null };
  let before = '検品', after = '梱包';
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [listing], total: 1, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) return route.fulfill({ json: { listing, current_fields: { 給与形態: '' }, versions: [
      { written_at: '2026-10-01T00:00:00Z', body: before, image_urls: null },
      { written_at: '2026-10-11T00:00:00Z', body: after, image_urls: null },
    ], history_counts: {}, history_may_be_incomplete: false } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  for (const scenario of [
    { before: '検品', after: '梱包', removed: '検品', added: '梱包', shot: 'review-01-short-replacement' },
    { before: '前'.repeat(299) + '検品' + '後'.repeat(299), after: '前'.repeat(299) + '梱包' + '後'.repeat(299), removed: '検品', added: '梱包', shot: 'review-02-600-character-change' },
    { before: '検'.repeat(600), after: '梱'.repeat(600), removed: '検'.repeat(600), added: '梱'.repeat(600), shot: 'review-03-600-character-replacement' },
  ]) {
    before = scenario.before; after = scenario.after;
    await page.goto('/app/job-copy?listing=31');
    await page.getByRole('button', { name: '求人の内容を確認する' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.jc-listing-popup-summary > div').filter({ has: page.locator('dt', { hasText: '給与' }) }).locator('dd')).toHaveText('取得できていません');
    const salary = dialog.locator('.jc-listing-comparison-row').filter({ has: page.getByRole('heading', { name: '給与', exact: true }) });
    await expect(salary.locator('p')).toHaveText(['取得できていません', '取得できていません']);
    await expect(dialog).not.toContainText('記載のない任意項目（0件）');
    await expect(dialog.getByText('任意項目の記載状況を取得できていません。', { exact: true })).toHaveCount(2);
    const row = dialog.locator('.jc-listing-comparison-row').first();
    await expect(row.locator('p')).toHaveText([after, before]);
    await expect(row.locator('mark')).toHaveText([scenario.added, scenario.removed]);
    expect(await row.locator('mark').first().evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(143, 209, 158)');
    if (width === 1440) {
      await mkdir(shots, { recursive: true });
      await row.locator('mark').first().scrollIntoViewIfNeeded();
      await page.screenshot({ path: `${shots}/${scenario.shot}.png` });
      if (scenario.after === '梱包') {
        await dialog.locator('.jc-listing-popup-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
        await page.screenshot({ path: `${shots}/review-04-optional-unavailable.png` });
      }
    }
    await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
  }
});
