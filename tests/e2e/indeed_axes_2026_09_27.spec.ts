/**
 * tests/e2e/indeed_axes_2026_09_27.spec.ts
 *
 * 職種詳細に足した 2 つの軸の E2E。
 *
 * # 何を見るか
 * どちらも「職種によって出るものが変わる」ことが要点なので、
 * 性質の違う職種を並べて、**出し分けが起きているか**を見る。
 * 数を直に書かず、職種どうしの関係で見るので実データが変わっても条件は変わらない。
 */

import { test, expect, Page } from '@playwright/test';
import { login } from './helpers/session';

const BASE = process.env.BASE_URL ?? 'http://localhost:9216';

async function openTitle(page: Page, name: string): Promise<void> {
  await page.goto(`${BASE}/tab/indeed/title?name=${encodeURIComponent(name)}`, {
    waitUntil: 'networkidle',
  });
  await expect(page.locator(`h2:has-text("${name}")`).first()).toBeVisible({ timeout: 60_000 });
  await page.waitForTimeout(1800);
}

async function styleOf(page: Page): Promise<string> {
  return page.evaluate(() => {
    const p = [...document.querySelectorAll('p')].find((x) =>
      (x.textContent ?? '').trim().startsWith('探され方:'));
    return p ? (p.textContent ?? '').trim() : '';
  });
}

async function auditCount(page: Page): Promise<number> {
  return page.evaluate(
    () =>
      [...document.querySelectorAll('p')].filter((x) => {
        const t = (x.textContent ?? '').trim();
        return t.startsWith('［直す］') || t.startsWith('［見る］');
      }).length,
  );
}

test.describe('職種詳細に足した軸', () => {
  test.beforeEach(async ({ page }) => {
    await login(page, BASE);
  });

  test('探され方が出て、条件で探される職種には職種名の監査を出さない', async ({ page }) => {
    // ホールスタッフは「条件で探される」側。職種名を直しても効かないので出さない
    await openTitle(page, 'ホールスタッフ');
    const s1 = await styleOf(page);
    expect(s1, '探され方が出ていない').toContain('探され方:');
    expect(s1).toContain('条件で探される');
    expect(await auditCount(page), '条件で探される職種に職種名の監査が出ている').toBe(0);

    // 事務は「混在」側。監査が出る
    await openTitle(page, '事務');
    const s2 = await styleOf(page);
    expect(s2).toContain('探され方:');
    expect(s2).not.toContain('条件で探される');
    expect(await auditCount(page), '監査が出るべき職種で出ていない').toBeGreaterThan(0);
  });

  test('給与の幅が出て、職種によって広い狭いが分かれる', async ({ page }) => {
    const spreadOf = async (): Promise<{ wide: boolean; ratio: number; prefs: number }> =>
      page.evaluate(() => {
        const h = [...document.querySelectorAll('h3')].find((x) =>
          (x.textContent ?? '').includes('給与で差を付けられる'));
        if (!h || !h.parentElement) return { wide: false, ratio: -1, prefs: -1 };
        const tr = h.parentElement.querySelector('tbody tr');
        if (!tr) return { wide: false, ratio: -1, prefs: -1 };
        const cells = [...tr.cells].map((c) => (c.textContent ?? '').trim());
        return {
          wide: cells.some((c) => c === '広い'),
          ratio: parseFloat((cells.find((c) => c.includes('倍')) ?? '').replace(/[^0-9.]/g, '')),
          prefs: parseInt((cells.find((c) => c.includes('県')) ?? '').replace(/[^0-9]/g, ''), 10),
        };
      });

    // 配送ドライバー: 幅が広い側
    await openTitle(page, '配送ドライバー');
    const wide = await spreadOf();
    expect(wide.ratio, '給与の幅が出ていない').toBeGreaterThan(0);
    expect(wide.prefs, '県が 20 未満のものを出している').toBeGreaterThanOrEqual(20);

    // ホールスタッフ: 幅が狭い側
    await openTitle(page, 'ホールスタッフ');
    const narrow = await spreadOf();
    expect(narrow.ratio).toBeGreaterThan(0);

    // 職種によって分かれていること（両方同じなら軸として機能していない）
    expect(wide.ratio, `幅が職種で分かれていない: ${wide.ratio} vs ${narrow.ratio}`)
      .toBeGreaterThan(narrow.ratio);
    expect(wide.wide, '広い側が広いと判定されていない').toBe(true);
    expect(narrow.wide, '狭い側が広いと判定されている').toBe(false);
  });

  test('画面に内部の英語や列名を出さない', async ({ page }) => {
    await openTitle(page, 'ホールスタッフ');
    const body = await page.evaluate(() => document.body.innerText);
    for (const ng of ['HOURLY', 'MONTHLY', 'search_style', 'median_salary', 'salary_period']) {
      expect(body, `内部の語が表に出ている: ${ng}`).not.toContain(ng);
    }
  });
});
