import { expect, test } from '@playwright/test';
import { COMPANY_A, COMPANY_B, editText, hsFail, hsMutate, hsPatches, hsRecord, loginAs, openDeal, openPropertyPanel, resetState, tursoDown } from './crm_write_live/helpers';
import { LIVE } from './crm_write_live/live';

/**
 * 架電画面の書き込みを、本物の Rust サーバ (debug ビルド) + 偽 HubSpot (scripts/loadtest/fake_hubspot.mjs --writable) +
 * 偽 Turso (監査 DB の台帳) で端から端まで確かめる。値はすべて架空。本物の HubSpot / Turso / Google にはつながない。
 * 実行: npx playwright test -c tests/e2e/crm_write_live.config.ts
 */
test.describe.configure({ mode: 'serial' });

test.beforeEach(async ({ context }) => {
  await resetState();
  await loginAs(context);
});

test('文字の項目を編集 → 緑「保存済み」。偽 HubSpot の値が変わり、送ったのは変えた項目だけ', async ({ page }) => {
  const before = (await hsPatches()).length;
  await openDeal(page, COMPANY_A);
  const panel = await openPropertyPanel(page);
  await editText(panel, '架電先電話番号', '03-0000-0001');
  await expect(panel.getByText('✓ 保存済み')).toBeVisible();
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).toBe('03-0000-0001');
  const sent = (await hsPatches()).slice(before);
  expect(sent).toHaveLength(1);
  expect(sent[0]).toMatchObject({ object: 'deals', id: LIVE.allowedDealId, properties: { bpo_29: '03-0000-0001' }, status: 200 });
  expect(Object.keys(sent[0].properties)).toEqual(['bpo_29']);
  await expect(panel.getByText('03-0000-0001')).toBeVisible();
});

test('選択式の項目を選ぶ → 選んだ値 (内部値) が送られる', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openPropertyPanel(page);
  await panel.getByRole('button', { name: 'BPO bpo_24を編集' }).click();
  await panel.getByRole('combobox', { name: 'BPO bpo_24' }).selectOption({ label: 'B' });
  await panel.getByRole('button', { name: '保存', exact: true }).click();
  await expect(panel.getByText('✓ 保存済み')).toBeVisible();
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_24).toBe('B');
});

test('編集中に HubSpot 側が変わった → 409 の確認画面。上書きは HubSpot の今の値を base にして送り直す', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openPropertyPanel(page);
  await editText(panel, '架電先電話番号', '03-1111-1111', false);
  // 読み込んだあと・保存の前に、別の人が HubSpot で変えた
  await hsMutate('deals', LIVE.allowedDealId, { bpo_29: '03-9999-9999' });
  const before = (await hsPatches()).length;
  await panel.getByRole('button', { name: '保存', exact: true }).click();
  const dlg = page.getByTestId('conflict-dialog');
  await expect(dlg).toBeVisible();
  await expect(dlg.getByText('03-9999-9999')).toBeVisible();
  await expect(dlg.getByText('03-1111-1111')).toBeVisible();
  expect((await hsPatches()).length, '衝突のときは HubSpot に書かない').toBe(before);
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).toBe('03-9999-9999');
  await dlg.getByRole('button', { name: '自分の値で上書き' }).click();
  await expect(panel.getByText('✓ 保存済み')).toBeVisible();
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).toBe('03-1111-1111');
});

test('ステージを「不通」へ: 必須の項目 (不通時チェック) が空だと 422 で移せない → 入れると dealstage と bpo_10 が 1 回で保存される', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const select = page.getByRole('combobox', { name: 'ステージを変更' });
  await expect(select).toBeVisible();
  await select.selectOption({ value: '1095387443' });
  const dlg = page.getByTestId('stage-modal');
  await expect(dlg.getByText('このステージに移すには次の項目が必要です')).toBeVisible();
  await expect(dlg.getByText(/不通時チェック.*\(必須\)/)).toBeVisible();
  const move = dlg.getByRole('button', { name: '移す' });
  // 未入力のまま移そうとすると、サーバ (stage_rules.json) が 422 missing_required で断る。HubSpot には書かれない
  const before = (await hsPatches()).length;
  await move.click();
  await expect(dlg.getByRole('alert').filter({ hasText: '次の項目を入力してください: 不通時チェック' })).toBeVisible();
  expect((await hsPatches()).length).toBe(before);
  expect((await hsRecord('deals', LIVE.allowedDealId)).dealstage).toBe('1095387442');
  // 入れると 1 回の PATCH でステージと項目が一緒に保存される
  await dlg.getByLabel(/不通時チェック/).check();
  await move.click();
  await expect(dlg).toHaveCount(0);
  await expect(page.getByRole('article', { name: '架電先の詳細' }).getByText('✓ 保存済み')).toBeVisible();
  const deal = await hsRecord('deals', LIVE.allowedDealId);
  expect(deal.dealstage).toBe('1095387443');
  expect(deal.bpo_10).toBe('true');
  const sent = (await hsPatches()).slice(before);
  expect(sent).toHaveLength(1);
  expect(sent[0].properties).toMatchObject({ dealstage: '1095387443', bpo_10: 'true' });
});

test('HubSpot が 503 を 1 回返す → 黄「反映待ち」→ 再送 worker が送り直して緑「保存済み」', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openPropertyPanel(page);
  await hsFail(1, 503);
  const before = (await hsPatches()).length;
  await editText(panel, '架電先電話番号', '03-2222-2222');
  const pending = panel.locator('[data-state="queued"]');
  await expect(pending).toContainText('反映待ち');
  // 受付の時点では HubSpot の値は変わっていない
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).not.toBe('03-2222-2222');
  // worker (debug 用に待ちを 2 秒に縮めてある) → 画面は 10 秒ごとに状態を見る
  await expect(panel.getByText('✓ 保存済み')).toBeVisible({ timeout: 60_000 });
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).toBe('03-2222-2222');
  const sent = (await hsPatches()).slice(before);
  expect(sent.map((p) => p.status)).toEqual([503, 200]);
});

test('許可リストにない案件: 編集の入口が出ない。API を直接叩いても 403 writes_disabled で HubSpot は呼ばれない', async ({ page }) => {
  await openDeal(page, COMPANY_B);
  await page.getByRole('tab', { name: 'プロパティ' }).click();
  const panel = page.getByTestId('property-panel');
  await expect(panel.getByText('URL_求人検索')).toBeVisible();
  await expect(panel.getByRole('button', { name: /を編集/ })).toHaveCount(0);
  await expect(page.getByRole('combobox', { name: 'ステージを変更' })).toHaveCount(0);
  const before = (await hsPatches()).length;
  const res = await page.evaluate(async ({ id }) => {
    const r = await fetch(`/api/crm/deals/${id}`, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json', 'X-Requested-With': 'fetch' },
      body: JSON.stringify({ operation_id: crypto.randomUUID(), base: { bpo_29: null }, set: { bpo_29: '03-3333-3333' } }),
    });
    return { status: r.status, body: await r.json() };
  }, { id: LIVE.blockedDealId });
  expect(res.status).toBe(403);
  expect(res.body).toMatchObject({ error: 'writes_disabled' });
  expect((await hsPatches()).length).toBe(before);
});

test('担当者 (Contact) の項目を編集 → Contact に PATCH され、画面は保存後の値 (objects_values) を出す', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openContactJobtitle(page);
  const before = (await hsPatches()).length;
  await editText(panel, '役職', '営業部長(架空)');
  await expect(panel.getByText('✓ 保存済み')).toBeVisible();
  expect((await hsRecord('contacts', '7000000000')).jobtitle).toBe('営業部長(架空)');
  const sent = (await hsPatches()).slice(before);
  expect(sent).toHaveLength(1);
  expect(sent[0]).toMatchObject({ object: 'contacts', id: '7000000000', properties: { jobtitle: '営業部長(架空)' }, status: 200 });
  await expect(panel.getByText('営業部長(架空)')).toBeVisible();
});

test('担当者の項目が編集中に HubSpot で変わった → 409 (object=contact)。HubSpot の値を使うと送らずにその値を出す', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openContactJobtitle(page);
  await editText(panel, '役職', '自分の役職(架空)', false);
  await hsMutate('contacts', '7000000000', { jobtitle: '別の人が変えた役職(架空)' });
  const before = (await hsPatches()).length;
  await panel.getByRole('button', { name: '保存', exact: true }).click();
  const dlg = page.getByTestId('conflict-dialog');
  await expect(dlg.getByText('別の人が変えた役職(架空)')).toBeVisible();
  await expect(dlg.getByText('自分の役職(架空)')).toBeVisible();
  await dlg.getByRole('button', { name: 'HubSpotの値を使う' }).click();
  await expect(dlg).toHaveCount(0);
  expect((await hsPatches()).length).toBe(before);
  expect((await hsRecord('contacts', '7000000000')).jobtitle).toBe('別の人が変えた役職(架空)');
});

test('台帳 (監査 DB) に記録できない → 書かずに 503 queue_unavailable。画面は赤「保存できていません」で、HubSpot は変わらない', async ({ page }) => {
  await openDeal(page, COMPANY_A);
  const panel = await openPropertyPanel(page);
  const before = (await hsPatches()).length;
  await tursoDown(true);
  try {
    await editText(panel, '架電先電話番号', '03-4444-4444');
    const err = panel.locator('[data-state="error"]');
    await expect(err).toContainText('保存できていません');
    await expect(err).toContainText('何も保存されていません');
  } finally {
    await tursoDown(false);
  }
  expect((await hsPatches()).length).toBe(before);
  expect((await hsRecord('deals', LIVE.allowedDealId)).bpo_29).not.toBe('03-4444-4444');
});

/** 「表示する項目を選ぶ」で担当者の「役職」を足して、パネルを返す */
async function openContactJobtitle(page: import('@playwright/test').Page) {
  await page.getByRole('tab', { name: 'プロパティ' }).click();
  const panel = page.getByTestId('property-panel');
  await panel.getByRole('button', { name: '表示する項目を選ぶ' }).click();
  await panel.getByRole('button', { name: /^担当者/ }).click();
  await panel.getByRole('searchbox', { name: '項目名で探す' }).fill('役職');
  await panel.getByRole('checkbox', { name: /役職/ }).check();
  await panel.getByRole('button', { name: 'この項目で表示する' }).click();
  await expect(panel.getByRole('button', { name: '役職を編集' })).toBeVisible();
  return panel;
}
