import { selectJobFeature } from './job-copy-navigation';
// Synthetic applicant text fixture: this checks React behavior, not production
// OIDC authorization or actual applicant motives. No real record text is logged.
import { test, expect } from '@playwright/test';

function fixture() {
  const capturedAt = '2026-10-05T00:00:00Z';
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{
    id: 'synthetic-reaction', hubspotListingId: '30', title: '合成の応募理由確認用求人', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678', location: '架空市', body: '合成求人本文です。', images: [],
    history: [{ id: 'synthetic-before', capturedAt: '2026-10-04T00:00:00Z', body: '合成の変更前本文です。', images: [] }],
  }] }, results: [{ listing_id: '30', summary: { total: 2, missing_date: 0, by_date: { '2026-10-04': 1, '2026-10-05': 1 }, dimensions: {} }, dated_comparison: null,
    applicant_reasons: { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: capturedAt,
      total_applicants: 2, total_source_values: 6, missing: 4, blank: 0, truncated: false,
      source_counts: { oubodouki: { missing: 0, blank: 0, nonblank: 2 }, ouboriyuu_baitaikisai: { missing: 2, blank: 0, nonblank: 0 }, ouboriyuu_hiaringu: { missing: 2, blank: 0, nonblank: 0 } },
      items: ['合成例：研修の説明を確認しました。', '<img src=x onerror="alert(1)">'].map((text, index) => ({ id: (index ? 'b' : 'a').repeat(64), text, source: 'hubspot', source_property: 'oubodouki', application_date: index ? '2026-10-05' : '2026-10-04', collected_at: null, version_id: null as string | null })),
    },
  }] };
}

test('reason originals stay collapsed and escaped, unknown cohorts separate, source filtering and print exclusion', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'reasons');
  const reasons = page.getByRole('region', { name: '応募理由の記述比較', exact: true });
  await expect(reasons).toContainText('応募2件 · 確認した記録欄3つ（応募ごと）');
  await expect(reasons).toContainText('記録欄の数（応募2件 × 3欄 = 6欄）: 記入あり2欄 · 空欄0欄 · 記録なし4欄');
  await expect(reasons.getByRole('region', { name: '比較元の記述', exact: true })).toContainText('版との対応は未取得');
  await expect(reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true })).toContainText('表示対象2件');
  await expect(reasons.locator('details[open]')).toHaveCount(0);
  // The texts per version (the category summary above them has its own collapsed list).
  const unknown = reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true });
  await expect(reasons.locator('blockquote').first()).toBeHidden();
  await unknown.getByText('記録された文を開く（社内確認用）', { exact: true }).nth(1).click();
  await expect(unknown.locator('blockquote').nth(1)).toHaveText('<img src=x onerror="alert(1)">');
  // The text that matches no keyword is also listed, collapsed and escaped, under 分類できなかった記録.
  await reasons.getByText('分類できなかった記録を開く（1件・社内確認用）', { exact: true }).click();
  await expect(reasons.locator('.ar-unclassified blockquote')).toHaveText('<img src=x onerror="alert(1)">');
  await expect(reasons.locator('img')).toHaveCount(0);
  await expect(reasons).toContainText('それ以外の個人情報が残っていることがあります');
  await reasons.getByLabel('理由の出典', { exact: true }).selectOption('ouboriyuu_hiaringu');
  await expect(reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true })).toContainText('表示対象0件');
  await reasons.getByLabel('理由の出典', { exact: true }).selectOption('all');
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await selectJobFeature(page, 'report');
  const report = page.getByRole('region', { name: '顧客報告と検証記録', exact: true });
  await report.getByText('記録された文を開く（社内確認用）', { exact: true }).first().click();
  await page.emulateMedia({ media: 'print' });
  for (const original of await page.locator('.ar-reasons').all()) await expect(original).toBeHidden();
  await expect(page.getByRole('region', { name: '顧客報告と検証記録', exact: true })).toBeVisible();
});

test('only explicit published-version associations appear in before and after reason groups', async ({ page }) => {
  const data = fixture();
  const result = data.results[0]; const before = result?.applicant_reasons.items[0]; const after = result?.applicant_reasons.items[1];
  if (!before || !after) throw new Error('Missing synthetic reason fixture');
  before.version_id = 'synthetic-before';
  after.version_id = `capture-synthetic-reaction-${data.capturedAt}`;
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) }));
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'reasons');
  const reasons = page.getByRole('region', { name: '応募理由の記述比較', exact: true });
  await expect(reasons.getByRole('region', { name: '比較元の記述', exact: true })).toContainText('表示対象1件');
  await expect(reasons.getByRole('region', { name: '比較先の記述', exact: true })).toContainText('表示対象1件');
  await expect(reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true })).toContainText('表示対象0件');
  await page.getByRole('tabpanel', { name: '応募理由', exact: true }).getByLabel('理由比較先').selectOption('synthetic-before');
  await expect(reasons).toContainText('同じ版を選んでいます');
  await expect(reasons).toContainText('選択した2版以外の表示対象記述: 1件');
});

/** The shape the server sends since 2026-10-08: six sources, applicant keys and category selections (synthetic). */
function currentFixture() {
  const data = fixture();
  const result = data.results[0];
  if (!result) throw new Error('Missing synthetic result');
  const key = (n: number) => String(n).repeat(64);
  const counts = (nonblank: number) => ({ missing: 2 - nonblank, blank: 0, nonblank });
  const reasons = {
    available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: data.capturedAt,
    total_applicants: 2, total_source_values: 12, blank: 0, truncated: false,
    source_counts: { oubodouki: counts(1), ouboriyuu_baitaikisai: counts(0), ouboriyuu_hiaringu: counts(0), genshokumaeshokukaranotenshokuriyuu: counts(0), ouboriyuukategori_hiaringu: counts(1), ouboriyuukategori_baitaikisai: counts(0) },
    missing: 10,
    items: [{ id: 'a'.repeat(64), applicant: key(1), text: '合成例：家から近いため', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-10-04', collected_at: null, version_id: null }],
    selections: [{ applicant: key(2), source_property: 'ouboriyuukategori_hiaringu', value: 'synthetic-salary', label: '給与', application_date: '2026-10-05' }],
  };
  return { ...data, results: [{ ...result, applicant_reasons: reasons }] };
}

test('reason categories: chosen and keyword counts apart, the timeline lane and the overview column', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(currentFixture()) }));
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'reasons');
  const summary = page.getByRole('region', { name: '応募理由の分類', exact: true });
  await expect(summary).toContainText('n=2（応募2件） · 選択済み1件 · キーワードで推定1件 · 分類できない0件');
  const categories = summary.getByRole('region', { name: '応募理由の分類の件数', exact: true });
  await expect(categories.getByRole('row', { name: /^給与/ })).toHaveText('給与1件1件0件n=2のため出しません');
  await expect(categories.getByRole('row', { name: /^勤務地/ })).toHaveText('勤務地1件0件1件n=2のため出しません');
  await expect(summary.getByRole('region', { name: '記録欄ごとの件数', exact: true })).toContainText('今の仕事・前の仕事から転職する理由0件0件2件');
  await expect(summary).not.toContainText('ouboriyuu');
  await selectJobFeature(page, 'timeline');
  await expect(page.getByRole('group', { name: '応募理由', exact: true })).not.toContainText('未取得');
  await expect(page.getByRole('region', { name: '期間ごとの応募理由', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  const overview = page.getByRole('region', { name: '求人の横断比較の表' });
  await expect(overview.getByRole('columnheader', { name: '多い応募理由' })).toBeVisible();
  await expect(overview).toContainText('給与 1件（選択1）・勤務地 1件（推定1）／n=2');
});

test('an old stored file: the overview counts texts, says the choices were not read, and the lane says so too', async ({ page }) => {
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(fixture()) }));
  await page.goto('/app/job-copy');
  await selectJobFeature(page, 'timeline');
  await expect(page.getByText('このデータでは分類の選択を取得していないため、数はすべて文から言葉で推定したものです（選択済みは0件ではなく未取得）')).toBeVisible();
  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  const overview = page.getByRole('region', { name: '求人の横断比較の表' });
  await expect(overview.locator('td.jo-reasons')).toHaveText('その他 1件（推定1）／記述n=2（応募ごとではない）（分類の選択は未取得）');
});
