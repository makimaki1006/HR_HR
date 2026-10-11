import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
export function registerRealdataFindingsTests() {
for (const width of [1440, 1920]) test(`実応答の形を使った表・横並び・全文・本文変更日 ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  const description = '架空の商品を棚から集め、出荷場所にまとめます。\n'.repeat(24);
  const baseSummary = { occupation: '倉庫作業', location: '東京都江東区', salary_kind: '月給', salary_min: null, salary_max: null, hours: '9:00〜18:00', holidays: null, body_changed_at: null };
  const jobs = [
    { id: '1', title: '架空の品物集めスタッフ', media: 'hrh', media_job_id: 'synthetic-hrh', account_id: null, prefecture: '東京都', municipality: '江東区', category: null, publication_status: '公開', application_count: 2, summary: { ...baseSummary, salary_min: '213000', salary_max: '249000', holidays: '土日休み' } },
    { id: '2', title: '架空の仕分けスタッフ', media: 'airwork', media_job_id: 'synthetic-aw', account_id: null, prefecture: '東京都', municipality: '江東区', category: null, publication_status: '掲載中', application_count: null, summary: baseSummary },
  ];
  const historyIds: string[][] = [];
  await page.route('**/api/job-copy/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: jobs, total: 2, offset: 0, next_offset: null, titles: [], deals: [{ id: '901', name: '架空の物流契約', count: 2 }], no_deal_count: 0, index_built_at: '2026-10-11T00:00:00Z' } });
    if (url.pathname.endsWith('/listing-body-dates')) {
      historyIds.push((url.searchParams.get('ids') ?? '').split(','));
      return route.fulfill({ json: { dates: { '1': '2026-10-09T00:00:00Z', '2': '2026-10-10T00:00:00Z' } } });
    }
    if (url.pathname.endsWith('/listing-comparison')) return route.fulfill({ json: { jobs: [
      { listing: jobs[0], body: `仕事内容：${description}`, fields: { 仕事内容: description, 給与形態: '月給', '基本給与 最小': '213000', '基本給与 最大': '249000', 勤務地: '東京都江東区', 勤務時間: '9:00〜18:00', '休日・休暇': '土日休み' } },
      { listing: jobs[1], body: '出荷先ごとに箱を分ける業務です。', fields: { 仕事内容: '出荷先ごとに箱を分ける業務です。', 給与形態: '月給', 勤務地: '東京都江東区', 勤務時間: '9:00〜18:00' } },
    ] } });
    return route.fulfill({ status: 503, json: {} });
  });
  await page.goto('/app/job-copy');
  const panel = page.getByRole('region', { name: 'HubSpot の求人' });
  await panel.getByText('絞り込み・並び順', { exact: true }).click();
  await panel.getByLabel('取引', { exact: true }).selectOption('901');
  await panel.getByRole('button', { name: 'この取引の求人', exact: true }).click();
  const table = page.getByRole('table', { name: '取引の求人一覧表' });
  await expect(table.locator('tbody tr').first()).toContainText('月給 213,000円〜249,000円');
  await expect(table.locator('tbody tr').last()).toContainText('月給 金額は未取得');
  await expect(table.locator('tbody tr').first()).toContainText('2026/10/09');
  await expect(table.locator('tbody tr').last()).toContainText('2026/10/10');
  expect(historyIds).toEqual([['1', '2']]);
  const media = table.locator('tbody [data-column="media"]').first();
  expect(await media.evaluate(element => getComputedStyle(element).whiteSpace)).toBe('nowrap');
  if (width === 1440) { await mkdir('docs/screenshots/job-copy-realdata-findings', { recursive: true }); await page.screenshot({ path: 'docs/screenshots/job-copy-realdata-findings/01-deal-table-1440.png' }); }
  for (const job of jobs) await table.getByLabel(`${job.title}を比較に選ぶ`).check();
  await page.getByRole('button', { name: '横に並べて比べる' }).click();
  const comparison = page.getByRole('region', { name: '選んだ求人の横並び比較' });
  await expect(comparison).toContainText('出荷先ごとに箱を分ける業務です。');
  const descriptionRow = comparison.locator('tbody tr').filter({ has: page.getByRole('rowheader', { name: '仕事内容', exact: true }) });
  const preview = descriptionRow.locator('.jc-comparison-preview').first();
  expect((await preview.boundingBox())!.height).toBeLessThan(90);
  await expect(preview.locator('mark').first()).toBeAttached();
  await comparison.scrollIntoViewIfNeeded();
  if (width === 1440) await page.screenshot({ path: 'docs/screenshots/job-copy-realdata-findings/02-parallel-preview-1440.png' });
  await descriptionRow.getByRole('button', { name: '全文を見る' }).click();
  const full = descriptionRow.locator('.jc-comparison-full');
  expect(await full.textContent()).toBe(description);
  await expect(full.locator('mark').first()).toBeAttached();
  expect((await full.boundingBox())!.height).toBeLessThanOrEqual(420);
  if (width === 1440) await page.screenshot({ path: 'docs/screenshots/job-copy-realdata-findings/03-parallel-full-1440.png' });
  await descriptionRow.getByRole('button', { name: '閉じる' }).click();
  await expect(full).toHaveCount(0);
  await expect(comparison).not.toContainText(/synthetic-hrh|synthetic-aw|基本給与 最小|求人id|LLM/);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
test('作り直し回数と写した条件の表示を1440×900で確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  await page.route('**/api/job-copy/listings/1/rikurozi-draft', route => route.fulfill({ json: {
    review_required: true, regeneration_count: 2, threshold: 0.2, min_sentence_chars: 8, csv: '',
    comparisons: [{ media: 'HRハッカー', title: '架空の基準求人', publication: '基準', ratio: 0.25, too_similar: true, whole_match: false, matched_sentences: ['品物を棚から集める業務を担当します'] }],
    copied: [{ label: '給与', value: '月給 213,000円〜249,000円\n試用・研修：確認が必要\n研修中の給与：確認が必要 180,000円〜未取得', review: true, reason: null, missing_chunks: [] }, { label: '休日・休暇', value: '週休2日・夏季休暇', review: false, reason: null, missing_chunks: [] }],
    generated: [{ label: '仕事内容', value: '品物を棚から集める業務を担当します。', review: false, reason: null, missing_chunks: [] }],
  } }));
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await page.getByRole('button', { name: '配送ドライバー・大分1の本文を見る' }).click();
  await page.getByRole('button', { name: 'リクロジメディア向けの案を作る' }).click();
  const draft = page.getByRole('region', { name: 'リクロジメディア向けの案' });
  await expect(draft).toContainText('自動で作り直した回数：2回（最大2回）');
  await expect(draft).toContainText('月給 213,000円〜249,000円');
  await expect(draft).toContainText('週休2日・夏季休暇');
  await expect(draft).toContainText('要確認の案');
  await draft.scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'docs/screenshots/job-copy-realdata-findings/04-regeneration-1440.png' });
  await draft.getByText('原本から写した条件', { exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'docs/screenshots/job-copy-realdata-findings/05-copied-conditions-1440.png' });
});

}
