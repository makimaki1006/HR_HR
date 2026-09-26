/**
 * tests/e2e/indeed_title_filter_2026_09_26.spec.ts
 *
 * 職種の一覧を「動き方」でも絞り込めることの E2E。
 *
 * # なぜ別ファイルか
 * 左の列や語の監査とは別のブランチで入れたので、片方だけ取り込んだときに
 * 落ちないよう分けてある。
 *
 * # 何を見るか
 * 各行は動き方をちょうど 1 つ持つので、**動き方ごとの行数を全部足すと全行数になる**。
 * 数を直に書かず恒等式で見るので、実データが変わっても条件は変わらない。
 *
 * # 実行
 *   BASE_URL=http://localhost:8090 E2E_EMAIL=... E2E_PASS=... \
 *     npx playwright test tests/e2e/indeed_title_filter_2026_09_26.spec.ts
 */

import { test, expect } from '@playwright/test';
import { login } from './helpers/session';

const BASE = process.env.BASE_URL ?? 'http://localhost:9216';

/** `src/indeed/wording.rs` が返しうる動き方の語。文言が変わったらここも変わる。 */
const TRENDS = [
  '振れながら増えた',
  '振れながら減った',
  '増え続けている',
  '減り続けている',
  '月ごとにばらつく',
  'データ不足',
];

test('職種の一覧を動き方で絞り込める（合計が全行数と一致する）', async ({ page }) => {
  await login(page, BASE);
  await page.goto(`${BASE}/?tab=${encodeURIComponent('/tab/indeed?view=titles')}`, {
    waitUntil: 'networkidle',
  });
  await expect(page.locator('h2:has-text("Indeed 採用市場")')).toBeVisible({
    timeout: 60_000,
  });
  await page.waitForTimeout(2500);

  const find = page.locator('#indeed-title-find');
  await expect(find).toBeVisible();
  await expect(find, '入力欄の説明に動き方が入っていない').toHaveAttribute(
    'placeholder',
    /動き方/,
  );

  const all = await page.locator('#indeed-title-wrap tbody tr').count();
  expect(all, '職種の一覧が空（データが読めていない可能性）').toBeGreaterThan(50);

  const visible = async (): Promise<number> =>
    page.evaluate(
      () =>
        [...document.querySelectorAll('#indeed-title-wrap tbody tr')].filter(
          (t) => (t as HTMLElement).offsetHeight > 0,
        ).length,
    );

  let sum = 0;
  const each: Record<string, number> = {};
  for (const t of TRENDS) {
    await find.fill(t);
    await page.waitForTimeout(350);
    const n = await visible();
    each[t] = n;
    sum += n;
  }

  expect(
    sum,
    `動き方ごとの合計 ${sum} が全行数 ${all} と合わない: ${JSON.stringify(each)}`,
  ).toBe(all);

  // 少なくとも 1 つは実際に絞れていること（全部 0 でも合計は 0 で一致しない、
  // という前提が崩れた場合に備える）
  const nonzero = Object.values(each).filter((n) => n > 0).length;
  expect(nonzero, '動き方でひとつも絞れていない').toBeGreaterThan(0);

  await find.fill('');
  await page.waitForTimeout(400);
  expect(await visible(), '絞り込みを消しても戻らない').toBe(all);
});
