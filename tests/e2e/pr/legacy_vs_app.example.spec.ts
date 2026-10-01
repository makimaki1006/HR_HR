import { expect, test } from '@playwright/test';
import { FIXTURE } from './helpers/fixture_values';
import { login } from './helpers/login';

/**
 * 旧新一致 spec の雛形。各チームは自分の画面用にコピーして使う:
 *   1. ファイル名を legacy_vs_app.<画面>.spec.ts にする (.example は付けない)。
 *   2. legacyValue() を、旧画面 (/tab/xxx や /api/... の HTML) から値を取り出す処理に置き換える。
 *   3. appValue() を、/app/<screen> の React 画面から同じ値を取り出す処理に置き換える。
 *   4. 期待値は fixture の既知値 (scripts/e2e/make_fixture_db.py の docstring) で固定し、
 *      旧 == 新 == 既知値 の 3 点で比べる。要素の有無ではなく数値・文字列で判定する。
 *   5. わざと値を変えて (fixture や描画) 落ちることを確認してからコミットする。
 * この雛形は旧画面側だけ実装済み (新画面は未実装なので fixme)。
 */

/** 旧画面: /api/municipalities_cascade?prefecture=東京都 が返す <option> の表示名。 */
async function legacyValue(page: import('@playwright/test').Page): Promise<string[]> {
  const res = await page.request.get('/api/municipalities_cascade?prefecture=' + encodeURIComponent('東京都'));
  const html = await res.text();
  return [...html.matchAll(/<option[^>]*>([^<]+)<\/option>/g)].map((m) => m[1].trim());
}

/** 新画面: /app/<screen> から同じ値を取り出す。担当チームが実装する。 */
async function appValue(_page: import('@playwright/test').Page): Promise<string[]> {
  throw new Error('未実装: /app/<screen> から値を取り出す処理を書く');
}

test.describe('旧画面と新画面の値一致 (雛形)', () => {
  test('旧画面の値が fixture の既知値と一致する', async ({ page }) => {
    await login(page);
    expect(await legacyValue(page)).toEqual([...FIXTURE.municipalities.東京都]);
  });

  test('旧画面 == 新画面', async ({ page }) => {
    test.fixme(true, '雛形。新画面が出来たチームが appValue() を実装して外す');
    await login(page);
    expect(await appValue(page)).toEqual(await legacyValue(page));
  });
});
