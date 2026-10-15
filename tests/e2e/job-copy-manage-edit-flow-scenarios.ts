import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import columns from '../../frontend/src/generated/jobgen/columns.json';
import { selectJobFeature, openOtherAnalysis, closeOtherAnalysis, findListings } from './job-copy-navigation';
import { composeFixtureBody } from './job-copy-listings-fixture.mjs';
import type { DraftSnapshot } from '../../frontend/src/generated/DraftSnapshot';
const shots = path.resolve('docs/screenshots/job-copy-manage-edit-flow');
const row = { ...Object.fromEntries(columns.map(key => [key, ''])), '求人id': 'secret-job', '店舗id': 'secret-shop', '職種id': 'secret-role', '案件名': '架空配送スタッフ', '仕事内容': '日用品を配送します。', '給与形態': '月給', '基本給与 最小': '250000', '基本給与 最大': '300000', '雇用形態': '正社員', '勤務時間': '9:00〜18:00', '自由項目1のタイトル': '休日・休暇', '自由項目1の内容': '土日休み', '公開': '非公開' };
const body = composeFixtureBody(row);
export async function mockFlow(page: Page, allowed = true, failedSave = false) {
  const drafts: DraftSnapshot[] = [{ schema_version: 1, draft_id: '10000000-0000-4000-8000-000000000001', created_at: '2026-10-10T00:00:00Z', source_kind: 'free_text', review_status: 'pending', row: { ...row, '仕事内容': 'チームで日用品を配送します。' }, facts: {} }];
  const airworkDrafts: DraftSnapshot[] = [];
  const requests: { method: string; body: any }[] = [];
  let revision = 'a'.repeat(64);
  const listing = { id: '30', media: 'hrh', media_job_id: 'secret-job', account_id: null, title: '架空配送スタッフ', prefecture: '大分県', municipality: '大分市', category: 'ドライバー', publication_status: '掲載終了', last_csv_detected_at: null, application_count: null };
  const airwork = { ...listing, id: '31', media: 'airwork', title: '架空看護スタッフ', media_job_id: 'secret-aw', account_id: 'secret-account', category: '看護師', publication_status: '掲載中' };
  await page.route('**/api/job-copy/listings*', route => {
    const url = new URL(route.request().url());
    const entries = [{ ...listing, draft_status: drafts.at(-1)?.review_status }, { ...airwork, draft_status: airworkDrafts.at(-1)?.review_status ?? null }].filter(value => ['prefecture', 'media', 'draft_status'].every(key => !url.searchParams.get(key) || value[key as keyof typeof value] === url.searchParams.get(key)) && (!url.searchParams.get('title') || value.category === url.searchParams.get('title')) && (!url.searchParams.get('publication') || value.publication_status === url.searchParams.get('publication')));
    return route.fulfill({ json: { status: 'ready', listings: entries, titles: ['ドライバー', '看護師'], publication_statuses: ['掲載終了', '掲載中'], total: entries.length, index_built_at: '2026-10-11T00:00:00Z', offset: 0, next_offset: null, refreshing: false, refresh_failed: false } });
  });
  await page.route('**/api/job-copy/listings/30/versions', route => route.fulfill({ json: { listing, current_row: row, versions: [{ written_at: '2026-10-01T00:00:00Z', body, image_urls: null }], history_counts: {}, history_may_be_incomplete: false, drafts, draft_revision: revision, can_write_drafts: allowed } }));
  await page.route('**/api/job-copy/listings/31/versions', route => route.fulfill({ json: { listing: airwork, versions: [{ written_at: '2026-10-01T00:00:00Z', body: '健康を支える看護のお仕事です。時給1,800円。9時〜18時、週休2日。', image_urls: null }], history_counts: {}, history_may_be_incomplete: false, drafts: airworkDrafts, draft_revision: revision, can_write_drafts: allowed } }));
  for (const endpoint of ['manual-draft', 'draft']) await page.route(`**/api/job-copy/listings/*/${endpoint}`, route => {
    const req = route.request().postDataJSON(); const method = route.request().method(); requests.push({ method, body: req });
    if (!allowed || failedSave) return route.fulfill({ status: 403, json: { code: 'draft_forbidden' } });
    const targetDrafts = new URL(route.request().url()).pathname.includes('/31/') ? airworkDrafts : drafts;
    if (method === 'POST') targetDrafts.push({ schema_version: 1, draft_id: req.operation_id, created_at: '2026-10-11T00:00:00Z', source_kind: 'free_text', review_status: 'pending', row: req.row, facts: {}, manual_edit: { base_draft_id: req.base_draft_id, published_hash: req.base_body_hash, reason: req.reason, changed_conditions: ['基本給与 最小'] } });
    else { const last = targetDrafts.at(-1)!; last.review_status = req.status; last.review_events = [...(last.review_events ?? []), { actor: 'private-reviewer@example.invalid', at: '2026-10-11T01:00:00Z', status: req.status }]; }
    revision = method === 'POST' ? 'b'.repeat(64) : 'c'.repeat(64);
    return route.fulfill({ json: { status: 'saved', operation_id: req.operation_id, draft: targetDrafts.at(-1), revision, code: '' } });
  });
  return { requests, drafts };
}
async function shot(page: Page, width: number, name: string) {
  if (width === 1440) { await fs.mkdir(shots, { recursive: true }); await page.screenshot({ path: path.join(shots, `${name}-1440.png`) }); }
}
function parseCsv(text: string): string[][] {
  const rows: string[][] = []; let fields: string[] = []; let value = ''; let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') { if (quoted && text[i + 1] === '"') { value += '"'; i++; } else quoted = !quoted; }
    else if (c === ',' && !quoted) { fields.push(value); value = ''; }
    else if (c === '\r' && text[i + 1] === '\n' && !quoted) { fields.push(value); rows.push(fields); fields = []; value = ''; i++; }
    else value += c;
  }
  return rows;
}
/** 本文は「ほかの分析」の横画面にある。開いて確かめ、ページを操作できるよう閉じる。 */
async function expectBodyContains(page: Page, text: string) {
  await openOtherAnalysis(page, '本文・画像');
  await expect(page.getByLabel('求人票')).toContainText(text);
  await closeOtherAnalysis(page);
}
export function manageEditScenarios(login?: (page: Page) => Promise<void>) {
  test.beforeEach(async ({ page }) => { await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true')); if (login) await login(page); });
  for (const width of [1440, 1920]) test.describe(`${width}px 管理→修正→比較→表示`, () => {
    test.use({ viewport: { width, height: 1000 } });
    test('求人を切り替えても前の本文を再取得せず選んだ求人を保持する', async ({ page }) => {
      await mockFlow(page);
      const fetched: string[] = [];
      page.on('request', request => { if (/\/listings\/\d+\/versions$/.test(request.url())) fetched.push(new URL(request.url()).pathname); });
      await page.goto('/app/job-copy?listing=30&demo=1&status=all'); // 掲載状況の既定は掲載中。掲載終了の求人も一覧に出すので「すべて」にする
      await findListings(page);
      await expectBodyContains(page, '日用品を配送します。');
      const list = page.getByRole('region', { name: 'HubSpot の求人' });
      await list.getByRole('button', { name: '架空看護スタッフの本文を見る' }).click();
      await expectBodyContains(page, '健康を支える看護');
      await expect(page).toHaveURL(/listing=31/);
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      await expect(page.getByLabel('仕事の内容', { exact: true })).toHaveValue('健康を支える看護のお仕事です。時給1,800円。9時〜18時、週休2日。');
      await page.getByRole('button', { name: '手直しを閉じる' }).click();
      expect(fetched).toEqual(['/api/job-copy/listings/30/versions', '/api/job-copy/listings/31/versions']);
      await list.getByRole('button', { name: '架空配送スタッフの本文を見る' }).click();
      await expectBodyContains(page, '日用品を配送します。');
      await expect(page).toHaveURL(/listing=30/);
    });
    test('AirWorkの冒頭と給与を手直しから保存・表示まで保持する', async ({ page }) => {
      const { requests } = await mockFlow(page);
      const body = '訪問看護師募集。時給1800円。\n勤務時間：9〜18時';
      await page.route('**/api/job-copy/listings/31/versions', route => route.fulfill({ json: { listing: { id: '31', media: 'airwork', media_job_id: 'secret-aw', title: '架空看護スタッフ' }, versions: [{ written_at: '2026-10-01T00:00:00Z', body, image_urls: null }], drafts: [], draft_revision: 'a'.repeat(64), can_write_drafts: true, history_counts: {}, history_may_be_incomplete: false } }));
      await page.goto('/app/job-copy?listing=31&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      await expect(page.getByLabel('仕事の内容', { exact: true })).toHaveValue(body);
      await page.getByLabel('変更した項目と内容を確認しました').check();
      await page.getByRole('button', { name: '確認待ちの案として保存して比較' }).click();
      expect(requests[0].body.row['仕事内容']).toBe(body);
      expect(requests[0].body.row['基本給与 最小']).toBe('');
      await page.locator('.jc-draft-review').getByRole('combobox', { name: '案の確認状態', exact: true }).selectOption('adopted');
      await page.getByRole('button', { name: '確認状態を保存' }).click();
      await page.getByRole('button', { name: 'この案を求人票で見る', exact: true }).click();
      await expect(page.getByLabel('求人票')).toContainText('訪問看護師募集。時給1800円。');
      await expect(page.getByLabel('求人票')).toContainText('勤務時間：9〜18時');
      await shot(page, width, '05-airwork-preserved');
    });
    for (const [before, after] of [['土日休みの配送', '日曜休みの配送'], ['東京都の配送', '大阪府の配送'], ['月給25万円の配送', '月給30万円の配送']]) test(`見出しの${before}→${after}は一覧で確認し意味で分類しない`, async ({ page }) => {
      const { requests } = await mockFlow(page);
      const current = { ...row, '案件名': before };
      await page.route('**/api/job-copy/listings/30/versions', route => route.fulfill({ json: { listing: { id: '30', media: 'hrh', title: '架空配送スタッフ' }, current_row: current, versions: [{ written_at: '2026-10-01T00:00:00Z', body: composeFixtureBody(current), image_urls: null }], drafts: [], draft_revision: 'a'.repeat(64), can_write_drafts: true, history_counts: {}, history_may_be_incomplete: false } }));
      await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      await editor.getByLabel('求人の見出し').fill(after!);
      await expect(editor.getByRole('alert')).toHaveCount(0);
      await expect(editor.getByLabel('条件を変更する理由')).toHaveCount(0);
      const preview = editor.getByRole('region', { name: '保存前の変更確認' });
      await expect(preview).toContainText(before!); await expect(preview).toContainText(after!);
      await expect(preview.locator('ins')).toBeVisible(); await expect(preview.locator('del')).toBeVisible();
      const save = editor.getByRole('button', { name: '確認待ちの案として保存して比較' });
      await expect(save).toBeDisabled(); expect(requests).toHaveLength(0);
      await preview.getByRole('checkbox').check();
      await editor.getByLabel('求人の見出し').fill(`${after!} `);
      await expect(preview.getByRole('checkbox')).not.toBeChecked(); await expect(save).toBeDisabled();
      await preview.getByRole('checkbox').check();
      if (before!.startsWith('土日')) { await preview.scrollIntoViewIfNeeded(); await shot(page, width, '07-heading-change-confirmation'); }
      await save.click();
      expect(requests[0].body.row['案件名']).toBe(`${after!} `); expect(requests[0].body.reason).toBe('');
    });
    for (const includeCurrentRow of [true, false]) test(`原文の末尾空白を保持し削除時の理由を求める（項目データ${includeCurrentRow ? 'あり' : 'なし'}）`, async ({ page }) => {
      const { requests, drafts } = await mockFlow(page);
      const originalRow = { ...row, '勤務時間': '9〜18時 ' };
      await page.route('**/api/job-copy/listings/30/versions', route => route.fulfill({ json: {
        listing: { id: '30', media: 'hrh', media_job_id: 'secret-job', account_id: null, title: '架空配送スタッフ', prefecture: null, municipality: null, category: null, publication_status: null, last_csv_detected_at: null, application_count: null },
        current_row: includeCurrentRow ? originalRow : null,
        versions: [{ written_at: '2026-10-01T00:00:00Z', body: composeFixtureBody(originalRow), image_urls: null }],
        history_counts: {}, history_may_be_incomplete: false, drafts, draft_revision: 'a'.repeat(64), can_write_drafts: true,
      } }));
      await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      const input = editor.getByLabel('働く時間・休憩', { exact: true });
      await expect(input).toHaveValue('9〜18時 ');
      await expect(editor.getByRole('alert')).toHaveCount(0);
      const preview = editor.getByRole('region', { name: '保存前の変更確認' });
      await expect(preview).toContainText('変更なし');
      await preview.getByRole('checkbox').check();
      const save = editor.getByRole('button', { name: '確認待ちの案として保存して比較' });
      await expect(save).toBeEnabled();
      await input.fill('9〜18時');
      await expect(preview.getByRole('checkbox')).not.toBeChecked();
      await expect(editor.getByRole('alert')).toContainText('働く時間・休憩');
      const changed = preview.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '働く時間・休憩', exact: true }) });
      expect(await changed.locator('pre').allTextContents()).toEqual(['9〜18時 ', '9〜18時']);
      await expect(changed.locator('del')).toHaveText(' ');
      await preview.getByRole('checkbox').check();
      await expect(save).toBeDisabled(); expect(requests).toHaveLength(0);
      if (!includeCurrentRow) { await editor.getByRole('alert').scrollIntoViewIfNeeded(); await shot(page, width, '08-whitespace-condition-warning'); }
      await editor.getByLabel('条件を変更する理由').fill('勤務時間末尾の空白を削除したため');
      await save.click(); await expect(page.locator('.jc-comparison')).toBeVisible();
      expect(requests[0].body.row['勤務時間']).toBe('9〜18時');
      expect(requests[0].body.reason).toBe('勤務時間末尾の空白を削除したため');
      expect(requests[0].body.row['基本給与 最小']).toBe('250000');
    });
    test('給与変更と理由、保存状態、二つの案の比較、採用CSVの84列と具体値', async ({ page }) => {
      const { requests } = await mockFlow(page);
      await page.goto('/app/job-copy?demo=1');
      await findListings(page);
      await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
      await findListings(page);
      const list = page.getByRole('region', { name: 'HubSpot の求人' });
      await list.getByText('絞り込み・並び順', { exact: true }).click();
      await list.getByLabel('都道府県', { exact: true }).selectOption('大分県');
      await list.getByLabel('職種の分類', { exact: true }).selectOption('ドライバー');
      await list.getByLabel('媒体', { exact: true }).selectOption('hrh');
      await list.getByRole('group', { name: '掲載状況で絞り込む' }).getByRole('button', { name: /^掲載終了/ }).click();
      await list.getByLabel('案の確認状態で絞り込む', { exact: true }).selectOption('pending');
      await expect(list.getByRole('button', { name: /の本文を見る/ })).toHaveCount(1);
      await shot(page, width, '01-manage');
      await list.getByRole('button', { name: '架空配送スタッフの本文を見る' }).click();
      await page.getByRole('button', { name: '現在の文面を手直しする', exact: true }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      await expect(editor.locator('.jc-manual-fields textarea')).toHaveCount(81);
      await expect(editor.locator('.jc-manual-fields input')).toHaveCount(0);
      await expect(editor.locator('.jc-management-fields')).not.toHaveAttribute('open');
      await expect(editor).not.toContainText(/基本給与|求人id|店舗id|職種id|secret-/);
      await editor.getByLabel('給与の下限', { exact: true }).fill('270000');
      await editor.getByLabel('仕事の内容', { exact: true }).fill('チームで配送し、確認"済み"の品を届けます。\n荷物は、日用品です。');
      const save = editor.getByRole('button', { name: '確認待ちの案として保存して比較' });
      await expect(editor.getByRole('alert')).toContainText('募集条件に関わる変更');
      await expect(save).toBeDisabled(); expect(requests).toHaveLength(0);
      await editor.getByLabel('条件を変更する理由').fill('募集担当者に給与の下限の変更を確認したため');
      await editor.getByLabel('変更した項目と内容を確認しました').check();
      await expect(save).toBeEnabled();
      await editor.getByRole('alert').scrollIntoViewIfNeeded(); await shot(page, width, '02-edit-warning');
      await save.click(); await expect(page.locator('.jc-comparison')).toBeVisible();
      expect(requests).toHaveLength(1); expect(requests[0].body.row['基本給与 最小']).toBe('270000');
      expect(Object.keys(requests[0].body.row)).toHaveLength(84); expect(requests[0].body.row['求人id']).toBe('secret-job');
      await expect(list).toContainText('案：確認待ち');
      const table = page.locator('.jc-column-comparison');
      const salary = table.getByRole('row').filter({ has: page.getByRole('rowheader', { name: '給与の下限', exact: true }) });
      await expect(salary).toContainText('月給 250,000円'); await expect(salary).toContainText('月給 270,000円');
      await expect(salary.locator('ins')).toBeVisible(); await expect(salary.locator('del')).toBeVisible();
      await page.getByText('元データを見る', { exact: true }).click(); await expect(page.locator('.jc-draft-source')).not.toContainText(/secret-|基本給与|求人id|店舗id/);
      await table.scrollIntoViewIfNeeded(); await shot(page, width, '03-compare');
      await page.getByLabel('比較元', { exact: true }).selectOption('hubspot-draft-10000000-0000-4000-8000-000000000001');
      await expect(salary).toContainText('月給 250,000円'); await expect(salary).toContainText('月給 270,000円');
      await page.getByLabel('比較元', { exact: true }).selectOption('hubspot-history-30-0');
      await page.locator('.jc-draft-review').getByRole('combobox', { name: '案の確認状態', exact: true }).selectOption('adopted');
      await page.getByRole('button', { name: '確認状態を保存' }).click();
      await expect(page.locator('.jc-draft-review h3')).toHaveText('採用の案');
      expect(requests).toHaveLength(2); expect(requests[1].body.base_revision).toBe('b'.repeat(64));
      if (!await list.getByLabel('案の確認状態で絞り込む', { exact: true }).isVisible()) await list.getByText('絞り込み・並び順', { exact: true }).click();
      await list.getByLabel('案の確認状態で絞り込む', { exact: true }).selectOption('adopted');
      await expect(list.getByRole('button', { name: /の本文を見る/ })).toHaveCount(1); await expect(list).toContainText('案：採用');
      await page.getByRole('button', { name: 'この案を求人票で見る', exact: true }).click();
      await expect(page.getByLabel('求人票')).toContainText('270,000円');
      await expect(page.getByLabel('求人票')).toContainText('荷物は、日用品です。');
      await page.getByLabel('求人票').scrollIntoViewIfNeeded(); await shot(page, width, '04-adopted');
      const download = page.waitForEvent('download');
      await page.getByRole('button', { name: '採用した案をHRハッカーのCSVで保存' }).click();
      const bytes = await fs.readFile((await (await download).path())!);
      expect([...bytes.subarray(0, 3)]).toEqual([239, 187, 191]);
      const csv = parseCsv(bytes.toString('utf8').slice(1)); expect(csv).toHaveLength(2); expect(csv[0]).toEqual(columns); expect(csv[1]).toHaveLength(84);
      expect(csv[1][columns.indexOf('基本給与 最小')]).toBe('270000');
      expect(csv[1][columns.indexOf('仕事内容')]).toBe(requests[0].body.row['仕事内容']);
      expect(csv[1][columns.indexOf('求人id')]).toBe('secret-job');
      expect(csv[1][columns.indexOf('公開')]).toBe('非公開');
      await page.reload(); await openOtherAnalysis(page, '変更差分'); await expect(page.locator('.jc-draft-review h3')).toHaveText('採用の案');
      expect(await page.locator('.jc-app').innerText()).not.toMatch(/secret-|private-reviewer@|基本給与|求人id|店舗id/);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
    });
    test('保存済みAI案から手直しでき、変更前の値も維持する', async ({ page }) => {
      const { requests } = await mockFlow(page); await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '保存済みの案を手直しする' }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      await expect(editor.getByLabel('仕事の内容', { exact: true })).toHaveValue('チームで日用品を配送します。');
      await editor.getByLabel('求人の見出し').fill('手直しした配送スタッフ');
      await expect(editor.getByLabel('条件を変更する理由')).toHaveCount(0);
      await page.getByLabel('変更した項目と内容を確認しました').check();
      await editor.getByRole('button', { name: '確認待ちの案として保存して比較' }).click();
      await expect(page.locator('.jc-comparison')).toBeVisible();
      expect(requests[0].body.base_draft_id).toBe('10000000-0000-4000-8000-000000000001');
      expect(requests[0].body.row['案件名']).toBe('手直しした配送スタッフ');
      expect(requests[0].body.row['基本給与 最小']).toBe('250000');
    });
    test('AirWorkの本文を基に84項目の案へ進み、条件未取得を0にしない', async ({ page }) => {
      await mockFlow(page); await page.goto('/app/job-copy?listing=31&demo=1');
      await findListings(page);
      await expectBodyContains(page, '時給1,800円');
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      await expect(editor.getByLabel('仕事の内容', { exact: true })).toHaveValue('健康を支える看護のお仕事です。時給1,800円。9時〜18時、週休2日。');
      await expect(editor.getByLabel('給与の下限', { exact: true })).toHaveValue('');
      await expect(editor.getByLabel('給与の下限', { exact: true })).toHaveAttribute('placeholder', '（空欄）');
    });
    test('権限なしの操作とサーバーの拒否を確認し保存済みと表示しない', async ({ page }) => {
      const { requests } = await mockFlow(page, false); await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await expect(page.getByRole('button', { name: '現在の文面を手直しする' })).toBeDisabled();
      await expect(page.getByRole('button', { name: '保存済みの案を手直しする' })).toBeDisabled(); expect(requests).toHaveLength(0);
      await mockFlow(page, true, true); await page.reload();
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      await page.getByLabel('変更した項目と内容を確認しました').check();
      await page.getByRole('button', { name: '確認待ちの案として保存して比較' }).click();
      await expect(page.getByRole('region', { name: '求人の手直し' })).toContainText('案を保存する権限がありません');
    });
    test('保存済み案に管理情報がなくても現在の値を保って手直しできる', async ({ page }) => {
      const { requests, drafts } = await mockFlow(page);
      drafts[0]!.row = { ...row, '求人id': '', '店舗id': '', '職種id': '' };
      await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '保存済みの案を手直しする' }).click();
      const editor = page.getByRole('region', { name: '求人の手直し' });
      await editor.getByLabel('求人の見出し').fill('見出しを手直しした配送スタッフ');
      await expect(editor.getByLabel('条件を変更する理由')).toHaveCount(0);
      await page.getByLabel('変更した項目と内容を確認しました').check();
      await editor.getByRole('button', { name: '確認待ちの案として保存して比較' }).click();
      await expect(page.locator('.jc-comparison')).toBeVisible();
      expect(requests[0].body.row).toMatchObject({ '求人id': 'secret-job', '店舗id': 'secret-shop', '職種id': 'secret-role' });
      expect(requests[0].body.reason).toBe('');
    });
    test('現在の文面が未取得の場合は保存を止め、再取得を案内する', async ({ page }) => {
      const { requests, drafts } = await mockFlow(page);
      await page.route('**/api/job-copy/listings/30/versions', route => route.fulfill({ json: { listing: { id: '30', media: 'hrh', media_job_id: 'secret-job', account_id: null, title: '架空配送スタッフ', prefecture: null, municipality: null, category: null, publication_status: null, last_csv_detected_at: null, application_count: null }, versions: [], drafts, current_row: null, current: null, history_counts: {}, history_may_be_incomplete: false, draft_revision: 'a'.repeat(64), can_write_drafts: true } }));
      await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await expect(page.getByRole('button', { name: '現在の文面を手直しする' })).toBeDisabled();
      await expect(page.getByRole('button', { name: '保存済みの案を手直しする' })).toBeDisabled();
      await expect(page.getByText('現在の本文が未取得のため、手直しを保存できません。求人を読み込み直して内容を確認してください。')).toBeVisible();
      expect(requests).toHaveLength(0);
    });
    test('手直しを閉じて既存の求人票表示と比較へ戻れる', async ({ page }) => {
      await mockFlow(page); await page.goto('/app/job-copy?listing=30&demo=1');
      await findListings(page);
      await page.getByRole('button', { name: '現在の文面を手直しする' }).click();
      await page.getByRole('button', { name: '手直しを閉じる' }).click();
      await selectJobFeature(page, 'body'); await expect(page.getByLabel('求人票')).toContainText('250,000円');
    });
  });
}
