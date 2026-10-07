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
  await expect(reasons).toContainText('応募2件・記録された理由6件');
  await expect(reasons.getByRole('region', { name: '比較元の記述', exact: true })).toContainText('版との対応は未取得');
  await expect(reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true })).toContainText('表示対象2件');
  await expect(reasons.locator('details[open]')).toHaveCount(0);
  await expect(reasons.locator('blockquote').first()).toBeHidden();
  await reasons.getByText('内部閲覧用の原記録を開く', { exact: true }).nth(1).click();
  await expect(reasons.locator('blockquote').nth(1)).toHaveText('<img src=x onerror="alert(1)">');
  await expect(reasons.locator('img')).toHaveCount(0);
  await expect(reasons).toContainText('匿名化された内容ではありません');
  await reasons.getByLabel('理由の出典', { exact: true }).selectOption('ouboriyuu_hiaringu');
  await expect(reasons.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true })).toContainText('表示対象0件');
  await reasons.getByLabel('理由の出典', { exact: true }).selectOption('all');
  await page.setViewportSize({ width: 375, height: 850 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await selectJobFeature(page, 'report');
  const report = page.getByRole('region', { name: '顧客報告と検証記録', exact: true });
  await report.getByText('内部閲覧用の原記録を開く', { exact: true }).first().click();
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
