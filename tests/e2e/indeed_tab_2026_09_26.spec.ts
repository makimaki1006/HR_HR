/**
 * tests/e2e/indeed_tab_2026_09_26.spec.ts
 *
 * 採用市場タブ（Indeed）の E2E。
 *
 * # なぜ足したか
 * このタブには E2E が 1 本も無かった。単体テストは「関数がその値を返す」までしか
 * 見ないので、**図が描かれたか・絞り込みが効いたか・横にはみ出していないか**は
 * 画面を動かさないと分からない。実際、単体テストが全部通っている状態で
 * 次の 3 つが見つかっている。
 *
 *   - 127 行の表に行のホバー反応が無い（地色が rgba(0,0,0,0) のまま）
 *   - 左の列を足したら表が画面からはみ出した（折り返し判定が画面幅を見ていた）
 *   - 節を指した URL を開いても画面が先頭のまま（印がまだ DOM に無いうちに送られる）
 *
 * # 実行
 *   BASE_URL=http://localhost:8090 E2E_EMAIL=... E2E_PASS=... \
 *     npx playwright test tests/e2e/indeed_tab_2026_09_26.spec.ts
 *
 * # データが要る
 * `data/indeed_insights.db` が読めることが前提。無い環境では採用市場タブが
 * 空になるので、最初のテストで気づけるようにしてある。
 */

import { test, expect, Page } from '@playwright/test';
import { login } from './helpers/session';

const BASE = process.env.BASE_URL ?? 'http://localhost:9216';

/** 採用市場タブの面を開く。`?tab=` 経由なので素のアクセスと同じ道を通る。 */
async function openView(page: Page, view: string): Promise<void> {
  const target = `/tab/indeed?view=${view}`;
  await page.goto(`${BASE}/?tab=${encodeURIComponent(target)}`, {
    waitUntil: 'networkidle',
  });
  // 図の初期化は htmx の差し込み後に走る。見出しが出るまで待つ
  await expect(page.locator('h2:has-text("Indeed 採用市場")')).toBeVisible({
    timeout: 60_000,
  });
}

/** ECharts が実際に作られた図の数と、作られなかった数。 */
async function chartState(page: Page): Promise<{ total: number; dead: number }> {
  return page.evaluate(() => {
    const els = [...document.querySelectorAll('.echart')];
    const ec = (window as unknown as { echarts?: { getInstanceByDom(e: Element): unknown } })
      .echarts;
    return {
      total: els.length,
      dead: ec ? els.filter((e) => !ec.getInstanceByDom(e)).length : els.length,
    };
  });
}

test.describe('採用市場タブ', () => {
  test.beforeEach(async ({ page }) => {
    await login(page, BASE);
  });

  test('4 つの面がすべて開き、図が作られ、横にはみ出さない', async ({ page }) => {
    for (const view of ['overview', 'titles', 'industry', 'people']) {
      await openView(page, view);
      await page.waitForTimeout(2500); // 図の初期化を待つ

      const { total, dead } = await chartState(page);
      expect(total, `${view}: 図が 1 枚も無い（データが読めていない可能性）`).toBeGreaterThan(0);
      expect(dead, `${view}: 作られなかった図がある`).toBe(0);

      // 表は自前の枠の中で横に流す。**ページ全体**が横に動いてはいけない
      const overflows = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth + 1,
      );
      expect(overflows, `${view}: ページが横にはみ出している`).toBe(false);
    }
  });

  test('左の列から 4 つの面を選べる', async ({ page }) => {
    await openView(page, 'overview');
    await page.waitForTimeout(2000);

    const nav = page.locator('.indeed-sidenav');
    await expect(nav).toBeVisible();

    for (const label of ['全体', '職種', '業界・分類', '求職者']) {
      await expect(
        nav.locator(`a:text-is("${label}")`),
        `左の列に「${label}」が無い`,
      ).toHaveCount(1);
    }

    // いま見ている面の下にだけ節がぶら下がる
    const secLinks = await nav.locator('a.indeed-sec-link').count();
    expect(secLinks, '全体の面には節が複数あるはず').toBeGreaterThan(0);
  });

  test('節へ飛べて、その URL を開き直しても節まで送られる', async ({ page }) => {
    await openView(page, 'overview');
    await page.waitForTimeout(2500);

    const first = page.locator('.indeed-sidenav a.indeed-sec-link').first();
    await expect(first).toBeVisible();
    const href = await first.getAttribute('href');
    expect(href, '節のリンクが # で始まっていない').toMatch(/^#sec-\d+$/);

    await first.click();
    await page.waitForTimeout(800);
    const movedY = await page.evaluate(() => Math.round(window.scrollY));
    expect(movedY, '節を押しても動かない').toBeGreaterThan(100);

    // ここが本題。同じ URL を**まっさらな文脈**で開き直す。
    // 印は htmx が差し込んだ後にしか存在しないので、ブラウザ既定のアンカー送りでは
    // 空振りする。手当てが効いていることを見る。
    const url = page.url();
    expect(url, 'URL にハッシュが残っていない').toContain('#sec-');

    const ctx = await page.context().browser()!.newContext({
      viewport: { width: 1280, height: 900 },
      storageState: await page.context().storageState(),
    });
    const fresh = await ctx.newPage();
    await fresh.goto(url, { waitUntil: 'networkidle' });
    await fresh.waitForTimeout(3500);
    const y = await fresh.evaluate(() => Math.round(window.scrollY));
    expect(y, '節を指した URL を開き直しても先頭のまま').toBeGreaterThan(100);
    await ctx.close();
  });

  test('職種の一覧を文字で絞り込める', async ({ page }) => {
    await openView(page, 'titles');
    await page.waitForTimeout(2500);

    const find = page.locator('#indeed-title-find');
    await expect(find).toBeVisible();

    const all = await page.locator('#indeed-title-wrap tbody tr').count();
    expect(all, '職種の一覧が空').toBeGreaterThan(50);

    const visible = async (): Promise<number> =>
      page.evaluate(
        () =>
          [...document.querySelectorAll('#indeed-title-wrap tbody tr')].filter(
            (t) => (t as HTMLElement).offsetHeight > 0,
          ).length,
      );

    await find.fill('事務');
    await page.waitForTimeout(400);
    const byName = await visible();
    expect(byName, '職種名で絞れない').toBeGreaterThan(0);
    expect(byName, '職種名で絞っても減っていない').toBeLessThan(all);

    // 絞り込みを消したら戻る
    await find.fill('');
    await page.waitForTimeout(400);
    expect(await visible()).toBe(all);
  });

  test('127 行の表は行にカーソルを当てると色が変わる', async ({ page }) => {
    await openView(page, 'titles');
    await page.waitForTimeout(2500);

    const row = page.locator('#indeed-title-wrap tbody tr').nth(3);
    await row.scrollIntoViewIfNeeded();
    const before = await row.evaluate((el) => getComputedStyle(el).backgroundColor);
    await row.hover();
    await page.waitForTimeout(400);
    const after = await row.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(after, '行にカーソルを当てても地色が変わらない').not.toBe(before);
  });

  test('狭い画面でも面の切り替えが消えない', async ({ page }) => {
    // 以前は 1023px 未満で左の列ごと display:none にしていた。
    // 面を変える手段が画面から無くなるので、横に流す形に変えてある。
    for (const width of [800, 620, 400]) {
      await page.setViewportSize({ width, height: 900 });
      await openView(page, 'overview');
      await page.waitForTimeout(2200);

      const nav = page.locator('.indeed-sidenav');
      await expect(nav, `${width}px で左の列が消えている`).toBeVisible();

      for (const label of ['全体', '職種', '業界・分類', '求職者']) {
        await expect(
          nav.locator(`a:text-is("${label}")`),
          `${width}px で「${label}」が見えない`,
        ).toBeVisible();
      }

      const overflows = await page.evaluate(
        () =>
          document.documentElement.scrollWidth >
          document.documentElement.clientWidth + 1,
      );
      expect(overflows, `${width}px でページが横にはみ出している`).toBe(false);
    }
  });

  test('職種詳細に、打たれている語と求人票の職種名の食い違いが出る', async ({ page }) => {
    // 「ホールスタッフ」は実データで 2.0% しか打たれておらず、
    // 「カフェ」が 10.8% ある。そのズレが画面に出ることを見る。
    await page.goto(`${BASE}/tab/indeed/title?name=${encodeURIComponent('ホールスタッフ')}`, {
      waitUntil: 'networkidle',
    });
    await page.waitForTimeout(2200);

    const card = page.locator('h3:has-text("求人票の職種名を決めるための語")');
    await expect(card).toBeVisible({ timeout: 30_000 });

    const box = card.locator('xpath=..');
    // 指摘は「直す」か「見る」の印つきで出る
    await expect(box.locator('p:has-text("［直す］"), p:has-text("［見る］")').first()).toBeVisible();

    // 語には「どこに書くか」が並ぶ。
    // カードには表が 2 つある（いま打たれている語 / 14 か月で動いた語）。
    // 2 列目の意味が違うので、**前の表だけ**を見る。
    const kinds = await box
      .locator('div:has(> table) >> nth=0')
      .locator('tbody tr td:nth-child(2)')
      .allTextContents();
    expect(kinds.length, '語が 1 つも出ていない').toBeGreaterThan(0);
    for (const k of kinds) {
      expect(['職種名', '雇用形態', '条件', '書けない']).toContain(k.trim());
    }

    // LLM へ渡す文には数字を入れない（工程⑦の数値照合に掛からないようにするため）
    const guide = await box
      .locator('p:has-text("打っている職種名は")')
      .first()
      .textContent();
    expect(guide, '渡す文が無い').toBeTruthy();
    expect(guide!, `渡す文に数字が入っている: ${guide}`).not.toMatch(/[0-9]/);
  });

  test('求人票作成から引く口が、数字を含まない文を返す', async ({ page }) => {
    const res = await page.request.get(
      `${BASE}/api/indeed/wordbrief?title=${encodeURIComponent('事務')}`,
    );
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.found, '語が引けない').toBe(true);
    expect(typeof body.guide).toBe('string');
    expect(body.guide.length).toBeGreaterThan(10);
    expect(body.guide, '渡す文に数字が入っている').not.toMatch(/[0-9]/);

    // 語は書く場所で分かれて返る
    for (const key of ['titles', 'koyou', 'joken']) {
      expect(Array.isArray(body[key]), `${key} が配列でない`).toBe(true);
    }
    // 画面用の数字は別に返る（こちらには数字がある）
    expect(Array.isArray(body.moved)).toBe(true);
  });
});
