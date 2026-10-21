import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';
import { selectJobFeature } from '../job-copy-navigation';

/**
 * 案件管理の記録欄 3 つ (検証記録・要因メモ・受け取った文面) が、求人ごとにサーバーへ保存され、
 * 開き直すと読み込まれることを確かめる。求人と応募は合成データ、記録の保存先 (/api/job-copy/listings/30/notes) は
 * 試験の中の偽の応答 (保存された内容を覚える) で、本物の監査 DB には書かない。
 */
const snapshot = {
  schemaVersion: 1, capturedAt: '2026-08-20T00:00:00Z',
  capture_bundle: { schemaVersion: 1, capturedAt: '2026-08-20T00:00:00Z', jobs: [{
    id: 'synthetic-pr-notes', hubspotListingId: '30', shopId: '0042', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678',
    location: '大分県大分市', body: '仕事内容：合成の配送業務です。\n給与：月給250,000円〜280,000円', images: [], history: [],
  }] },
  results: [{ listing_id: '30', summary: { total: 2, missing_date: 0, by_date: { '2026-08-10': 2 }, dimensions: { gender: { 男性: 2 } } }, dated_comparison: null }],
};

type Stored = Record<string, { value: string; updated_by: string; updated_at: string }>;

async function open(page: Page, store: Stored, behaviour: { failPut: boolean }) {
  const puts: { field: string; value: string }[] = [];
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(snapshot) }));
  await page.route('**/api/job-copy/market*', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ source: '合成', titles: [], prefectures: [], ctk_basis: '', series: null }) }));
  await page.route('**/api/job-copy/listings/30/notes', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ notes: store }) }));
  await page.route('**/api/job-copy/listings/30/notes/*', route => {
    const field = route.request().url().split('/').pop() ?? '';
    const { value } = route.request().postDataJSON() as { value: string };
    if (behaviour.failPut) return route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ code: 'notes_unavailable' }) });
    puts.push({ field, value });
    store[field] = { value, updated_by: 'writer@example.test', updated_at: '2026-10-21T03:00:00+00:00' };
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ field, ...store[field] }) });
  });
  await login(page);
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  return puts;
}

test.describe('案件管理の記録欄の保存', () => {
  test('検証記録を保存すると、保存した人と時刻が出て、開き直しても残っている', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const store: Stored = {};
    const puts = await open(page, store, { failPut: false });
    await selectJobFeature(page, 'report');
    const bar = page.getByRole('group', { name: '記録の保存', exact: true });
    await expect(bar).toContainText('まだ保存されていません');
    await page.getByLabel('仮説', { exact: true }).fill('給与を上げると応募が増える');
    await expect(bar).toContainText('保存していない変更があります');
    await bar.getByRole('button', { name: '保存', exact: true }).click();
    await expect(bar).toContainText('最後に保存：writer@example.test 2026/10/21 12:00');
    expect(puts).toHaveLength(1);
    expect(puts[0]?.field).toBe('review');
    expect(JSON.parse(puts[0]?.value ?? '{}')).toEqual({ stage: 'plan', target: 'both', fields: { hypothesis: '給与を上げると応募が増える' } });
    // 開き直すと保存済みの内容が入っている
    await page.reload();
    await expect(page.locator('.jc-job')).toHaveCount(1);
    await selectJobFeature(page, 'report');
    await expect(page.getByLabel('仮説', { exact: true })).toHaveValue('給与を上げると応募が増える');
  });

  test('保存できないときは 1 行だけ理由を出し、入力は消えない', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await open(page, {}, { failPut: true });
    await selectJobFeature(page, 'report');
    await page.getByLabel('結果と根拠', { exact: true }).fill('応募は12件から9件');
    await page.getByRole('group', { name: '記録の保存', exact: true }).getByRole('button', { name: '保存', exact: true }).click();
    await expect(page.getByRole('alert').filter({ hasText: 'いまは保存できません。入力はこの画面に残っています。' })).toHaveCount(1);
    await expect(page.getByLabel('結果と根拠', { exact: true })).toHaveValue('応募は12件から9件');
  });
});
