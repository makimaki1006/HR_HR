import path from 'node:path';
import fs from 'node:fs/promises';
import { test, expect } from '@playwright/test';
const shots = path.resolve('docs/screenshots/rikurozi-draft');
const csv = '\ufeff求人id,店舗id,画像1\r\n1234567,4081,https://example.invalid/one.jpg\r\n';
const result = {
  review_required: true, threshold: 0.35, csv,
  comparisons: [
    { title: '配送ドライバー・大分1', media: 'HRハッカー', publication: '基準', ratio: 0.22, too_similar: false },
    { title: '日用品の配送', media: 'AirWork', publication: '掲載中', ratio: 0.35, too_similar: true },
    { title: '商品の仕分け', media: 'AirWork', publication: '掲載中か不明', ratio: null, too_similar: false },
  ],
  copied: [
    { label: '給与', value: '月給 280,000円〜320,000円', review: false, reason: null },
    { label: '勤務地', value: '大分県大分市', review: false, reason: null },
    { label: '勤務時間', value: '8:00〜17:00（休憩1時間）', review: false, reason: null },
    { label: '雇用形態', value: '正社員', review: false, reason: null },
    { label: '休日・休暇', value: '週休2日。年間休日120日', review: false, reason: null },
  ],
  generated: [
    { label: '求人名', value: '日用品を届ける配送スタッフ', review: false, reason: null },
    { label: '仕事内容', value: null, review: true, reason: '原本の条件と数字が一致しないため空欄にしました。' },
    { label: '紹介文', value: '身近な商品を、いつもの地域へ', review: false, reason: null },
    { label: '仕事の魅力', value: '配送の手順を研修で学びます。', review: false, reason: null },
    { label: '職種', value: '配送スタッフ', review: false, reason: null },
  ],
};
export function registerRikuroziDraftTests() {
for (const width of [1440, 1920]) {
  test(`${width}px: selected HRハッカー creates a draft, checks unknown/similar and downloads`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => {
      expect(route.request().method()).toBe('POST'); return route.fulfill({ json: result });
    });
    await page.goto('/app/job-copy?demo=1');
    await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
    await page.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
    await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
    const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
    await expect(draft.getByText('要確認の案', { exact: true })).toBeVisible();
    await expect(draft).toContainText('掲載中 ／ 重なり：35.0％ ／ 似すぎ（要確認）');
    await expect(draft).toContainText('掲載中か不明 ／ 重なり：未取得（要確認）');
    await expect(draft).toContainText('月給 280,000円〜320,000円');
    await expect(draft).toContainText('原本の条件と数字が一致しないため空欄にしました。');
    await expect(draft).not.toContainText(/1234567|4081|求人id|店舗id|基本給与 最小|3-gram|LLM/);
    const download = page.waitForEvent('download');
    await draft.getByRole('button', { name: '確認用の案をダウンロード' }).click();
    const file = await download;
    expect(await fs.readFile((await file.path())!, 'utf8')).toBe(csv);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    if (width === 1440) {
      await fs.mkdir(shots, { recursive: true });
      await page.locator('.jc-main').evaluate(element => { element.scrollTop = 0; });
      await page.screenshot({ path: path.join(shots, 'review-1440.png') });
      await draft.getByRole('button', { name: '確認用の案をダウンロード' }).scrollIntoViewIfNeeded();
      await page.screenshot({ path: path.join(shots, 'checks-1440.png') });
    }
    // AirWork selections must not offer generation; a new HRハッカー starts with no old result.
    await page.getByRole('button', { name: '看護スタッフ・大分31の版を見る' }).click();
    await expect(page.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toHaveCount(0);
    await page.getByRole('button', { name: '配送ドライバー・沖縄2の版を見る' }).click();
    await expect(page.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toBeVisible();
    await expect(page.getByRole('button', { name: '確認用の案をダウンロード' })).toHaveCount(0);
  });
}
test('1440px: failure is readable and allows retry', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => route.fulfill({ status: 422, json: { code: 'customer_relation_unknown' } }));
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await page.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
  await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
  const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
  await expect(draft.getByRole('alert')).toContainText('取引先との関連');
  await expect(draft.getByRole('button', { name: 'リクロジメディア向けの案を作る' })).toBeEnabled();
  await expect(draft.getByRole('button', { name: '確認用の案をダウンロード' })).toHaveCount(0);
});

}
