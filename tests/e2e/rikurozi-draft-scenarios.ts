import path from 'node:path';
import fs from 'node:fs/promises';
import { test, expect } from '@playwright/test';
import { fixtureJobs, fixtureHistory } from './job-copy-listings-fixture.mjs';
import { findListings, openWork } from './job-copy-navigation';
const shots = path.resolve('docs/screenshots/rikurozi-draft');
const csv = '\ufeff求人id,店舗id,画像1,画像2,画像3\r\n,4081,https://example.invalid/one.jpg,,https://example.invalid/three.png\r\n';
const result = {
  generated_values: { 案件名: '日用品を届ける配送スタッフ', 仕事内容: '' },
  images: ['https://example.invalid/one.jpg', '', 'https://example.invalid/three.png'],
  review_required: true, threshold: 0.2, min_sentence_chars: 8, csv,
  comparisons: [
    { title: '配送ドライバー・大分1', media: 'HRハッカー', publication: '基準', ratio: 0.1, too_similar: false, whole_match: false, matched_sentences: [] },
    { title: '日用品の配送', media: 'AirWork', publication: '掲載中', ratio: 0.2, too_similar: true, whole_match: false, matched_sentences: ['配送の手順を研修で学びます'] },
    { title: '商品の仕分け', media: 'AirWork', publication: '掲載中か不明', ratio: null, too_similar: false, whole_match: false, matched_sentences: [] },
  ],
  copied: [
    { label: '給与', value: '月給 280,000円〜320,000円', review: false, reason: null, missing_chunks: [] },
    { label: '勤務地', value: '大分県大分市', review: false, reason: null, missing_chunks: [] },
    { label: '勤務時間', value: '8:00〜17:00（休憩1時間）', review: false, reason: null, missing_chunks: [] },
    { label: '雇用形態', value: '正社員', review: false, reason: null, missing_chunks: [] },
    { label: '休日・休暇', value: '週休2日。年間休日120日', review: false, reason: null, missing_chunks: [] },
  ],
  generated: [
    { label: '求人名', value: '日用品を届ける配送スタッフ', review: false, reason: null, missing_chunks: [] },
    { label: '仕事内容', value: null, review: true, reason: '原文にない数字の表記があるため空欄にしました。', missing_chunks: ['日給1100円'] },
    { label: '紹介文', value: '身近な商品を、いつもの地域へ', review: false, reason: null, missing_chunks: [] },
    { label: '仕事の魅力', value: '配送の手順を研修で学びます。', review: false, reason: null, missing_chunks: [] },
    { label: '職種', value: '配送スタッフ', review: false, reason: null, missing_chunks: [] },
  ],
};
export function registerRikuroziDraftTests() {
test.beforeEach(async ({ page }) => {
  await page.route('**/api/job-copy/market*', route => route.fulfill({ json: { titles: [], prefectures: [], series: null } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: fixtureJobs.length, index_built_at: '2026-10-10T00:00:00Z', listings: fixtureJobs, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false } }));
  await page.route('**/api/job-copy/listings/*/versions', route => route.fulfill({ json: fixtureHistory(new URL(route.request().url()).pathname.split('/').at(-2)) }));
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  // リクロジ向けに作った求人（IDはまだ空）が1件ある状態。保存の中身は各テストで確かめる。
  await page.route('**/api/job-copy/listings/*/rikurozi-listing', route => route.request().method() === 'POST'
    ? route.fulfill({ json: { status: 'created', listing_id: '900', linked_deals: 2, unlinked_deals: 0 } })
    : route.fulfill({ json: { listings: [{ listing_id: '900', name: '日用品を届ける配送スタッフ', reclogi_id: null }] } }));
});
for (const width of [1440, 1920]) {
  test(`${width}px: selected HRハッカー creates a draft, checks unknown/similar and downloads`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => {
      expect(route.request().method()).toBe('POST'); return route.fulfill({ json: result });
    });
    await page.goto('/app/job-copy?demo=1');
    await findListings(page);
    await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
    await findListings(page);
    await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
    await openWork(page);
    await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
    const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
    await expect(draft.getByText('要確認の案', { exact: true })).toBeVisible();
    await expect(draft).toContainText('掲載中 ／ 文の一致率：20.0％ ／ 似すぎ・不合格（要確認）');
    await expect(draft).toContainText('掲載中か不明 ／ 文の一致率：未取得（要確認）');
    await expect(draft).toContainText('月給 280,000円〜320,000円');
    await expect(draft).toContainText('原文にない数字の表記があるため空欄にしました。');
    await expect(draft).toContainText('日給1100円');
    await expect(draft).toContainText('一致した文');
    await expect(draft.locator('ol > li')).toHaveText(['画像1：https://example.invalid/one.jpg', '画像2：空欄', '画像3：https://example.invalid/three.png']);
    await expect(draft).toContainText('原本の条件');
    await expect(draft).toContainText('新しい求人として取り込むCSV');
    await expect(draft).toContainText('正しい数字を違う文脈で使った誤りは検知できません');
    await expect(draft).not.toContainText(/1234567|4081|求人id|店舗id|基本給与 最小|3-gram|LLM/);
    await expect(draft.getByLabel(/リクロジの求人ID（リクロジに取り込んだあとで入力します）/)).toBeVisible();
    const download = page.waitForEvent('download');
    await draft.getByRole('button', { name: 'この案でCSVを出す' }).click();
    const file = await download;
    expect(await fs.readFile((await file.path())!, 'utf8')).toBe(csv);
    await expect(draft).toContainText('HubSpotに新しい求人（公開前）を作りました。2件の取引に関連付けました。');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    if (width === 1440) {
      await fs.mkdir(shots, { recursive: true });
      await page.locator('.jc-main').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({ path: path.join(shots, 'review-1440.png') });
      await draft.getByRole('region', { name: 'リクロジ向けに作った求人' }).scrollIntoViewIfNeeded();
      await page.screenshot({ path: path.join(shots, 'reclogi-id-1440.png') });
      await draft.getByRole('button', { name: 'この案でCSVを出す' }).scrollIntoViewIfNeeded();
      await page.screenshot({ path: path.join(shots, 'checks-1440.png') });
    }
    // AirWork selections must not offer generation; a new HRハッカー starts with no old result.
    await page.getByRole('button', { name: '看護スタッフ・大分31の本文を見る' }).click();
    await expect(page.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toHaveCount(0);
    await page.getByRole('button', { name: '配送ドライバー・沖縄2の本文を見る' }).click();
    await openWork(page);
    await expect(page.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'この案でCSVを出す' })).toHaveCount(0);
  });
}
test('1440px: failure is readable and allows retry', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => route.fulfill({ status: 422, json: { code: 'customer_relation_unknown' } }));
  await page.goto('/app/job-copy?demo=1');
  await findListings(page);
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await findListings(page);
  await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
  await openWork(page);
  await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
  const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
  await expect(draft.getByRole('alert')).toContainText('取引先との関連');
  await expect(draft.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toBeEnabled();
  await expect(draft.getByRole('button', { name: 'この案でCSVを出す' })).toHaveCount(0);
});

test('1440px: copied condition mismatch stops CSV output', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => route.fulfill({ status: 422, json: { code: 'draft_copy_mismatch' } }));
  await page.goto('/app/job-copy?demo=1');
  await findListings(page);
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await findListings(page);
  await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
  await openWork(page);
  await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
  const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
  await expect(draft.getByRole('alert')).toContainText('出力を止めました（要確認）');
  await expect(draft.getByRole('button', { name: 'この案でCSVを出す' })).toHaveCount(0);
});

}
