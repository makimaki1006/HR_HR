import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面 (/app/crm?mode=fixture) の項目の書き換え。架空サンプルのメモリの中だけで動く (/api/crm/* は呼ばない)。
 * - プロパティ: 編集できる項目を直して保存 → 緑の「保存済み」と新しい値
 * - 案件の概要: 必須項目のあるステージへ移す → 項目の入力画面 → 入力 → 保存済み
 * 値はすべて frontend/src/screens/crm/fakeWrite.ts / workspaceFixture.ts の架空の値。
 */

const SCREEN_URL = '/app/crm?mode=fixture';

async function openFirst(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  const list = page.getByRole('list', { name: '架電キュー' });
  await expect(list.locator('.cq-row-company').first()).toBeVisible();
  await list.locator('li.cq-row button.cq-row-button').first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' })).toBeVisible();
}

test.describe('CRM 架電画面: 項目の書き換え (架空サンプル)', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('項目を編集して保存すると、緑の「保存済み」と新しい値が出る。HubSpot の API は呼ばない', async ({ page }) => {
    const crm: string[] = [];
    page.on('request', (r) => { if (new URL(r.url()).pathname.startsWith('/api/crm/')) crm.push(r.url()); });
    await openFirst(page);
    await page.getByRole('tab', { name: 'プロパティ' }).click();
    const panel = page.getByTestId('property-panel');
    // 読み取りだけの項目 (※編集不可) には「編集」が付かない
    await expect(panel.getByRole('button', { name: /URL_求人検索.*を編集/ })).toHaveCount(0);

    await panel.getByRole('button', { name: '募集職種（リストデータ）を編集' }).click();
    const input = panel.getByRole('textbox', { name: '募集職種（リストデータ）' });
    await input.fill('事務スタッフ(架空)');
    await panel.getByRole('button', { name: '保存', exact: true }).click();
    await expect(panel.getByText('✓ 保存済み')).toBeVisible();
    await expect(panel.getByText('事務スタッフ(架空)')).toBeVisible();
    expect(crm).toEqual([]);
  });

  test('必須項目のあるステージへ移す: 項目の入力画面 → 入力するまで「移す」は押せない → 保存済み', async ({ page }) => {
    await openFirst(page);
    const select = page.getByRole('combobox', { name: 'ステージを変更' });
    await expect(select).toBeVisible();
    await select.selectOption({ label: 'アポ日確定' });
    const dlg = page.getByRole('dialog');
    await expect(dlg.getByText('このステージに移すには次の項目が必要です')).toBeVisible();
    await expect(dlg.getByText(/アポ取得日.*\(必須\)/)).toBeVisible();
    const move = dlg.getByRole('button', { name: '移す' });
    await expect(move).toBeDisabled();
    await dlg.getByLabel(/アポ取得日/).fill('2026-11-01');
    await expect(move).toBeEnabled();
    await move.click();
    await expect(dlg).toHaveCount(0);
    await expect(page.getByRole('article', { name: '架電先の詳細' }).getByText('✓ 保存済み')).toBeVisible();
    await expect(select.locator('option:checked')).toHaveText('アポ日確定');
  });
});
