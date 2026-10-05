/** Synthetic cross-record fixture E2E only. Different record IDs are kept separate;
 * these checks do not establish random assignment, causal effects or live auth. */
import { createHash } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { test, expect } from '@playwright/test';

const visuals = resolve('data/job-copy-local/candidate-browser/ab-visual');
const capturedAt = '2026-10-05T00:00:00Z';
const imageBytes = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jh1sAAAAASUVORK5CYII=', 'base64');
const imageHash = createHash('sha256').update(imageBytes).digest('hex');
const image = (name: string) => ({ id: `synthetic-image-${name}`, url: `data:image/png;base64,${imageBytes.toString('base64')}`, caption: `合成${name}の掲載画像`, contentHash: imageHash });
const versionId = (name: string) => `capture-synthetic-ab-${name}-${capturedAt}`;
const bodies = { A: '合成A全文\n地域への配送を担当します。\n研修は2週間です。\n月給250,000円。', B: '合成B全文\n地域への配送を担当します。\n研修は4週間です。\n月給270,000円。' };

function reasons(total: number, name: string) {
  return { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: capturedAt,
    total_applicants: total, total_source_values: total * 3, missing: total * 3 - 1, blank: 0, truncated: false,
    source_counts: { oubodouki: { missing: total - 1, blank: 0, nonblank: 1 }, ouboriyuu_baitaikisai: { missing: total, blank: 0, nonblank: 0 }, ouboriyuu_hiaringu: { missing: total, blank: 0, nonblank: 0 } },
    items: [{ id: (name === 'A' ? 'a' : 'b').repeat(64), text: `合成${name}の内部原記録です。`, source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-08', collected_at: null, version_id: null }],
  };
}
function distribution(values: Record<string, number>, total: number) {
  return { denominator: total, categories: Object.entries(values).map(([category, count]) => ({ category, count, percentage: count / total * 100 })) };
}
function fixture() {
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: ['A', 'B'].map(name => ({
    id: `synthetic-ab-${name}`, hubspotListingId: name === 'A' ? '30' : '31', title: `合成配送募集${name}`, company: '合成同一取引先', media: 'HRハッカー', mediaJobId: name === 'A' ? '12345678' : '87654321', location: '大分県大分市', body: bodies[name as keyof typeof bodies], images: [image(name)],
  })) }, results: ['A', 'B'].map(name => {
    const a = name === 'A'; const total = a ? 10 : 6; const assigned = a ? 4 : 2;
    return { listing_id: a ? '30' : '31', summary: { total, missing_date: 0, by_date: { '2026-09-08': total }, dimensions: { gender: a ? { 男性: 6, 女性: 3, 不明: 1 } : { 男性: 2, 女性: 3, 不明: 1 } } },
      dated_comparison: { total, unknown: total - assigned, basis: '合成の日付観測対応', daily_representatives: {}, by_version: { [versionId(name)]: { count: assigned, dimensions: { gender: distribution(a ? { 男性: 1, 女性: 2, 不明: 1 } : { 男性: 2 }, assigned) } } } },
      applicant_reasons: reasons(total, name), hrh_performance: { schema_version: 1, source: 'hrhacker', job_id: a ? '12345678' : '87654321', captured_at: capturedAt, rows: [{ period_start: a ? '2026-09-01' : '2026-09-06', period_end: a ? '2026-09-10' : '2026-09-15', impressions: a ? 1000 : 2000, clicks: a ? 50 : 200, cost_yen: a ? 10000 : 40000, applications: a ? 5 : 20 }] },
    };
  }) };
}

test.beforeAll(() => { mkdirSync(visuals, { recursive: true }); });
test('cross-record A/B preserves bodies, images, independent denominators, scope and overlapping metric periods', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.goto('/app/job-copy');
  await page.getByRole('button', { name: '2求人のA/B比較', exact: true }).click();
  const comparison = page.getByRole('region', { name: '2求人のA/B比較', exact: true });
  await expect(comparison).toContainText('B求人を選ぶと');
  await comparison.getByLabel('Bとして比較する求人').selectOption('synthetic-ab-B');
  await expect(comparison).toContainText('組み合わせ未確認');
  await comparison.getByRole('checkbox', { name: '同じ募集として比較する組み合わせを確認した' }).check();
  await expect(comparison).not.toContainText('組み合わせ未確認');
  const a = comparison.getByRole('region', { name: 'A求人の比較内容', exact: true });
  const b = comparison.getByRole('region', { name: 'B求人の比較内容', exact: true });
  await expect(a).toContainText('求人レコード全体）：10件');
  await expect(b).toContainText('求人レコード全体）：6件');
  await a.getByText('Aの求人本文を全文確認', { exact: true }).click();
  await b.getByText('Bの求人本文を全文確認', { exact: true }).click();
  await expect(a.locator('.jc-body')).toHaveText(bodies.A);
  await expect(b.locator('.jc-body')).toHaveText(bodies.B);
  await expect.poll(() => comparison.locator('img').evaluateAll(images => images.length === 2 && images.every(image => image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0))).toBe(true);
  await a.getByLabel('Aの課金実績期間').selectOption('2026-09-01');
  await b.getByLabel('Bの課金実績期間').selectOption('2026-09-06');
  const period = comparison.getByRole('region', { name: 'A/B実績期間の比較', exact: true });
  await expect(period).toContainText('（10日）');
  await expect(period).toContainText('重なる日数：5日');
  await expect(period).toContainText('CTR差（B−A）：5ポイント');
  await expect(period).toContainText('A 10%・B 10%');
  await expect(b.locator('.ab-values')).toContainText('媒体応募数20件');
  await expect(b).toContainText('求人レコード全体）：6件');
  await comparison.getByText('性別のA/B比較', { exact: true }).click();
  const male = comparison.locator('tbody tr').filter({ has: page.getByRole('rowheader', { name: '男性', exact: true }) });
  await expect(male.locator('td')).toHaveText(['6', '60%', '2', '33.33%', '-26.67ポイント']);
  await a.getByText('Aの応募理由（内部閲覧）', { exact: true }).click();
  await a.getByText(/原記録を開く/).click();
  await expect(a.locator('blockquote')).toHaveText('合成Aの内部原記録です。');
  await comparison.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'ab-record-desktop.png'), fullPage: true });
  await comparison.getByLabel('応募の比較範囲').selectOption('version');
  await expect(a).toContainText('選択版の確定＋推定対応）：4件');
  await expect(b).toContainText('選択版の確定＋推定対応）：2件');
  await expect(a).toContainText('求人全体の版対応不明：6件');
  await expect(b).toContainText('求人全体の版対応不明：4件');
  await expect(male.locator('td')).toHaveText(['1', '25%', '2', '100%', '75ポイント']);
  await expect(a.locator('blockquote')).toHaveCount(0);
  await expect(a).toContainText('版対応不明の表示対象：1記述');
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await a.getByRole('heading', { name: 'A：合成配送募集A', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'ab-a-mobile-viewport.png') });
  await b.getByRole('heading', { name: 'B：合成配送募集B', exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'ab-b-mobile-viewport.png') });
  const table = comparison.getByRole('region', { name: '性別比較表の横スクロール', exact: true });
  expect(await table.evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
  await table.scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(visuals, 'ab-table-mobile-viewport.png') });
  await page.screenshot({ path: resolve(visuals, 'ab-version-mobile.png'), fullPage: true });
  await comparison.getByLabel('応募の比較範囲').selectOption('record');
  await a.getByText(/原記録を開く/).click();
  await page.emulateMedia({ media: 'print' });
  await expect(a.locator('blockquote')).toBeHidden();
  await expect(a.locator('.jc-body')).toBeVisible();
  await expect(b.locator('.jc-body')).toBeVisible();
});

test('B candidates include loaded records outside list filters and changing the pair requires confirmation again', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.goto('/app/job-copy');
  await page.locator('.jc-list input[type=search]').fill('合成配送募集A');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  await page.getByRole('button', { name: '2求人のA/B比較', exact: true }).click();
  const comparison = page.getByRole('region', { name: '2求人のA/B比較', exact: true });
  const chooser = comparison.getByLabel('Bとして比較する求人');
  await expect(chooser.locator('option[value="synthetic-ab-A"]')).toHaveCount(0);
  await expect(chooser.locator('option[value="synthetic-ab-B"]')).toHaveCount(1);
  await chooser.selectOption('synthetic-ab-B');
  const confirm = comparison.getByRole('checkbox', { name: '同じ募集として比較する組み合わせを確認した' });
  await confirm.check();
  await expect(confirm).toBeChecked();
  await comparison.getByLabel('比較グループ名').fill('合成配送の比較設定');
  await comparison.getByLabel('検証したい仮説').fill('合成例：研修期間の説明を比較する');
  await page.getByRole('button', { name: '本文・履歴', exact: true }).click();
  await expect(comparison).toBeHidden();
  await page.getByRole('button', { name: '2求人のA/B比較', exact: true }).click();
  await expect(confirm).toBeChecked();
  await expect(chooser).toHaveValue('synthetic-ab-B');
  await expect(comparison.getByLabel('比較グループ名')).toHaveValue('合成配送の比較設定');
  await expect(comparison.getByLabel('検証したい仮説')).toHaveValue('合成例：研修期間の説明を比較する');
  await chooser.selectOption('');
  await expect(confirm).not.toBeChecked();
  await expect(confirm).toBeDisabled();
  await chooser.selectOption('synthetic-ab-B');
  await expect(comparison).toContainText('組み合わせ未確認');
  await expect(comparison.getByRole('region', { name: 'A/B実績期間の比較', exact: true })).toContainText('両求人の実績期間を選ぶと比較できます');
  await expect(comparison.getByRole('region', { name: 'A求人の比較内容', exact: true })).toContainText('課金・クリック実績は未接続、または期間未選択');
});
