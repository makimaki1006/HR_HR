import { expect, Page, test } from '@playwright/test';
import { SALES_KPI_FIXTURE } from './helpers/fixture_values';
import { login } from './helpers/login';

/**
 * 営業KPI (React 版 /app/sales-kpi) の PR 用 E2E。
 *
 * - **CSP を有効のまま**ブラウザを動かす (bypassCSP は使わない)。サーバが付ける CSP は
 *   `script-src 'self' 'unsafe-inline' …` で `unsafe-eval` が無い。React 版が eval / new Function / インライン外の
 *   スクリプト / 外部 CDN を使っていれば、ここで securitypolicyviolation か外部リクエストとして落ちる。
 * - この画面のグラフ (日別の架電数) は ECharts ではなく素の SVG (`data-testid="daily-chart"`)。
 *   ECharts を使う画面ではないので `data-chart-ready` / `getInstanceByDom` は見ない。代わりに
 *   SVG の棒の数と各棒の値を API (`/api/sales-kpi/data`) から独立に数えた値と比べる。
 * - 期待値は fixture (tests/fixtures/sales_kpi、判定日 2026-09-04) の集計。
 *   サーバは global-setup.ts が SALES_KPI_FIXTURE_DIR / SALES_KPI_FIXTURE_TODAY 付きで起動している
 *   (Sheets を読まない経路。src/handlers/sales_kpi/fixture.rs)。
 */

interface Api {
  generated_at: string;
  by_person: Record<string, Record<string, number>>;
  by_team: Record<string, Record<string, number>>;
  stale: unknown[];
  week_deals: unknown[];
  next_week_deals: unknown[];
  calls: {
    daily: { date: string; calls: number; connected: number }[];
    periods: { this_week: { by_person: Record<string, Record<string, number>>; days: string[] } };
  };
}

async function fetchApi(page: Page): Promise<Api> {
  const res = await page.request.get('/api/sales-kpi/data');
  expect(res.status()).toBe(200);
  return (await res.json()) as Api;
}

/** 領域の .c カードを「ラベル → 値」の組にする (関数形式の evaluate は CSP の eval に当たらない)。 */
async function cards(page: Page, region: string): Promise<[string, string][]> {
  return page.locator(`#${region} .c`).evaluateAll((els) =>
    els.map((c) => [
      c.querySelector('.lab')?.textContent?.trim() ?? '',
      (c.querySelector('.v')?.textContent ?? '').trim(),
    ] as [string, string]),
  );
}

const sum = (bp: Record<string, Record<string, number>>, key: string, ids?: string[]) =>
  Object.entries(bp)
    .filter(([id]) => !ids || ids.includes(id))
    .reduce((a, [, v]) => a + (v[key] ?? 0), 0);

test.describe('営業KPI (React) /app/sales-kpi', () => {
  test('CSP 有効のまま表示でき、CSP 違反・外部リクエスト・コンソールエラーが 0 件', async ({ page }) => {
    await login(page);
    // ログイン直後の / は旧シェル (htmx / ECharts を CDN から読む) なので、観測は /app/sales-kpi への遷移から始める
    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];
    const external: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error') consoleErrors.push(m.text());
    });
    page.on('pageerror', (e) => pageErrors.push(String(e)));
    page.on('request', (r) => {
      const u = r.url();
      if (!u.startsWith('data:') && !u.startsWith('blob:') && !u.startsWith('http://localhost:')) external.push(u);
    });
    await page.addInitScript(() => {
      const w = window as unknown as { __csp: string[] };
      w.__csp = [];
      document.addEventListener('securitypolicyviolation', (e) => {
        w.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
      });
    });
    const res = await page.goto('/app/sales-kpi');
    // CSP が付いていること (付いていなければこの E2E が CSP 下の動作を見ていない)
    const csp = res?.headers()['content-security-policy'] ?? '';
    expect(csp).toContain("script-src 'self'");
    expect(csp).not.toContain('unsafe-eval');

    await expect(page.locator('#cards1 .c')).toHaveCount(7);
    await expect(page.locator('#range')).toContainText('2026年9月');
    expect(await page.evaluate(() => (window as unknown as { __csp: string[] }).__csp)).toEqual([]);
    expect(external).toEqual([]);
    expect(pageErrors).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  test('このブラウザでは CSP が効いている (ページ内の eval は止まる)。上のテストが「違反 0」を見ている前提の確認', async ({ page }) => {
    await login(page);
    // page.evaluate は CDP 経由で CSP の対象外なので、実ページのインラインスクリプトから eval を呼ぶ。
    // 応答の本文にだけプローブを足し、CSP ヘッダーはサーバが付けたものをそのまま使う。
    await page.route('**/app/sales-kpi', async (route) => {
      const resp = await route.fetch();
      const probe =
        "<script>try{window.__evalProbe=String(eval('1'))}catch(e){window.__evalProbe='blocked: '+e.name}</script>";
      await route.fulfill({ response: resp, body: (await resp.text()).replace('</body>', probe + '</body>') });
    });
    await page.goto('/app/sales-kpi');
    await expect(page.locator('#cards1 .c')).toHaveCount(7);
    const r = await page.evaluate(() => (window as unknown as { __evalProbe?: string }).__evalProbe);
    expect(r).toMatch(/^blocked: EvalError/);
  });

  test('fixture の主要 KPI の値 (全社 / 伊壺チーム / 架電)', async ({ page }) => {
    await login(page);
    const api = await fetchApi(page);
    await page.goto('/app/sales-kpi');
    await expect(page.locator('#cards1 .c')).toHaveCount(7);

    // API の by_person から独立に足した値 (画面の計算とは別経路)
    const k = SALES_KPI_FIXTURE;
    expect(sum(api.by_person, 'apo')).toBe(k.all.apo);
    expect(sum(api.by_person, 'pool')).toBe(k.all.pool);
    const done = sum(api.by_person, '実施');
    const den = done + sum(api.by_person, '未実施') + sum(api.by_person, '未処理') + sum(api.by_person, '要判定');
    expect([done, den]).toEqual([k.all.done, k.all.den]);

    // 今月の成績 (全社)
    const month = new Map(await cards(page, 'cards1'));
    expect(month.get('① 取ったアポ')).toBe(`${k.all.apo}件`);
    expect(month.get('③ 商談の予定')).toBe(`${k.all.pool}件`);
    expect(month.get('④ 日が過ぎた分')).toBe(`${k.all.den}件`);
    expect(month.get('② やった商談')).toBe(`${k.all.done}件`);
    expect(month.get('⑥ 商談化率')).toBe('72.4%'); // 163 / 225
    expect(month.get('⑤ アンケート回収率')).toBe('2.7%'); // 6 / 225
    expect(month.get('⑨ 持っているCヨミ')).toBe(`${k.all.cyomi}件`);

    // いま手を打てること (件数は API の配列の長さと一致する)
    const act = new Map(await cards(page, 'cards2'));
    expect(act.get('⑦ ステージが止まっている')).toBe(`${api.stale.length}件`);
    expect(api.stale.length).toBe(k.stale);
    expect(act.get('③ 今週の商談')).toBe(`${api.week_deals.length}件`);
    expect(act.get('③ 来週の商談')).toBe(`${api.next_week_deals.length}件`);

    // 架電 (今週 = 8/31〜9/4 の 5 日、全社)
    const tw = api.calls.periods.this_week;
    const conn = sum(tw.by_person, 'connected');
    expect(conn).toBe(k.calls.connected);
    const call = new Map(await cards(page, 'cards3'));
    expect(call.get('架電数')).toBe(`${k.calls.connected.toLocaleString('en-US')}件`);
    expect(call.get('発信した回数')).toBe(`${k.calls.calls.toLocaleString('en-US')}件`);
    expect(call.get('5分超の通話')).toBe(`${k.calls.long}件`);
    expect(call.get('1日あたりの架電数')).toBe(`${Math.round(conn / tw.days.length).toLocaleString('en-US')}件`);

    // 伊壺チームに絞ると今月の成績が by_team['伊壺チーム'] と一致する
    await page.locator('#teams .chip', { hasText: /^伊壺チーム$/ }).click();
    const t = api.by_team['伊壺チーム'] ?? {};
    const team = new Map(await cards(page, 'cards1'));
    expect(team.get('① 取ったアポ')).toBe(`${k.iduboTeam.apo}件`);
    expect(t.apo).toBe(k.iduboTeam.apo);
    expect(team.get('③ 商談の予定')).toBe(`${k.iduboTeam.pool}件`);
    expect(team.get('② やった商談')).toBe(`${k.iduboTeam.done}件`);
    expect(team.get('⑥ 商談化率')).toBe('63.3%'); // 19 / 30
  });

  test('日別の架電グラフ (SVG) の棒の数と値が API と一致する', async ({ page }) => {
    await login(page);
    const api = await fetchApi(page);
    await page.goto('/app/sales-kpi');
    const svg = page.locator('[data-testid="daily-chart"]');
    await expect(svg).toHaveCount(1);
    const expected = api.calls.daily.filter((d) => d.calls > 100);
    expect(expected.length).toBeGreaterThan(0);
    await expect(svg).toHaveAttribute('data-bars', String(expected.length));
    const bars = await svg.locator('g[data-bar-date]').evaluateAll((els) =>
      els.map((g) => ({
        date: g.getAttribute('data-bar-date'),
        calls: Number(g.getAttribute('data-calls')),
        connected: Number(g.getAttribute('data-connected')),
        h: Math.max(...[...g.querySelectorAll('rect')].map((r) => Number(r.getAttribute('height')))),
      })),
    );
    expect(bars.map(({ date, calls, connected }) => ({ date, calls, connected }))).toEqual(
      expected.map((d) => ({ date: d.date, calls: d.calls, connected: d.connected })),
    );
    // 一番多い日の棒がいちばん高い (描かれた高さが値に比例している)
    const maxCalls = Math.max(...expected.map((d) => d.calls));
    const tallest = bars.reduce((a, b) => (b.h > a.h ? b : a));
    expect(tallest.calls).toBe(maxCalls);
  });

  test('旧画面 /sales-kpi と新画面の全社の既定表示のカードが一致する', async ({ page }) => {
    await login(page);
    const regions = ['cards1', 'cards2', 'cards3', 'cards3b'];
    await page.goto('/sales-kpi');
    await expect(page.locator('#cards1 .c')).toHaveCount(7);
    const oldCards = Object.fromEntries(await Promise.all(regions.map(async (r) => [r, await cards(page, r)] as const)));
    await page.goto('/app/sales-kpi');
    await expect(page.locator('#cards1 .c')).toHaveCount(7);
    const newCards = Object.fromEntries(await Promise.all(regions.map(async (r) => [r, await cards(page, r)] as const)));
    expect(newCards).toEqual(oldCards);
    expect(Object.values(oldCards).flat().length).toBeGreaterThan(20);
  });

  test.describe('今月の成績カードの内訳 (#45)', () => {
    /** パネルの表を チーム → 担当者 → 一覧 と全部降りて、一覧の行数を足す。表の合計 (data-sum) と一覧の行数も確かめる。 */
    async function walk(page: Page): Promise<number> {
      const table = page.locator('#panel1 table.cdrill');
      if ((await table.count()) === 0) {
        const n = Number(await page.locator('#panel1-listhead').getAttribute('data-n'));
        expect(await page.locator('#panel1 .list a.item').count()).toBe(n);
        return n;
      }
      const sumAttr = Number(await table.locator('tfoot [data-sum]').getAttribute('data-sum'));
      const rows = await table.locator('tbody tr').count();
      let total = 0;
      for (let i = 0; i < rows; i++) {
        await page.locator('#panel1 table.cdrill tbody tr').nth(i).locator('button').click();
        total += await walk(page);
        await page.getByRole('button', { name: /に戻る/ }).click();
      }
      expect(total).toBe(sumAttr);
      return total;
    }

    test('7 枚すべて: カードの値 == 見出しの件数 == 表の合計 == 一覧の行数の合計 (全社)。CSP 違反 0', async ({ page }) => {
      await login(page);
      await page.addInitScript(() => {
        const w = window as unknown as { __csp: string[] };
        w.__csp = [];
        document.addEventListener('securitypolicyviolation', (e) => {
          w.__csp.push(`${e.violatedDirective} ${e.blockedURI}`);
        });
      });
      await page.goto('/app/sales-kpi');
      await expect(page.locator('#cards1 .c')).toHaveCount(7);
      const k = SALES_KPI_FIXTURE;
      // 左から ①アポ ③予定 ④日が過ぎた分 ②実施 ⑥率 ⑤率 ⑨Cヨミ。件数のカードの値は fixture の既知値
      const expectTotal = [k.all.apo, k.all.pool, k.all.den, k.all.done, k.all.den, k.all.den, k.all.cyomi];
      for (let i = 0; i < 7; i++) {
        const card = page.locator('#cards1 .c').nth(i);
        const label = (await card.locator('.lab').textContent()) ?? '';
        await card.click();
        const title = page.locator('#panel1-title');
        await expect(title).toBeVisible();
        const total = Number(await title.getAttribute('data-total'));
        expect(total, label).toBe(expectTotal[i]);
        if (i === 4) expect(Number(await title.getAttribute('data-num'))).toBe(k.all.done);
        if (i === 5) expect(Number(await title.getAttribute('data-num'))).toBe(6);
        if (i !== 4 && i !== 5) expect((await card.locator('.v').textContent()) ?? '', label).toContain(String(total));
        expect(await walk(page), `${label}: 一覧の行数の合計`).toBe(total);
        await expect(card.locator('.open')).toHaveText('閉じる ▲');
        await card.click();
        await expect(page.locator('#panel1')).toHaveClass(/hide/);
        await expect(card.locator('.open')).toHaveText('一覧を見る ▾');
      }
      expect(await page.evaluate(() => (window as unknown as { __csp: string[] }).__csp)).toEqual([]);
    });

    test('④: 区分の件数の和 == 分母、区分を押した一覧の行数 == その区分の件数。内 BPO を押した一覧 == 内 BPO', async ({ page }) => {
      await login(page);
      await page.goto('/app/sales-kpi');
      await page.locator('#cards1 .c').nth(2).click();
      const total = Number(await page.locator('#panel1-title').getAttribute('data-total'));
      const segs = await page.locator('#panel1 [data-seg]').evaluateAll((els) =>
        els.map((e) => ({ k: e.getAttribute('data-seg') ?? '', n: Number(e.getAttribute('data-n')) })),
      );
      expect(segs.filter((x) => x.k).map((x) => x.k)).toEqual(['実施', '未実施', '未処理', '要判定']);
      expect(segs.filter((x) => x.k).reduce((a, b) => a + b.n, 0)).toBe(total);
      expect(segs.filter((x) => x.k).map((x) => x.n)).toEqual([163, 53, 4, 5]);
      for (const sg of segs.filter((x) => x.k)) {
        await page.locator(`#panel1 [data-seg="${sg.k}"]`).click();
        expect(await walk(page), sg.k).toBe(sg.n);
        await page.locator(`#panel1 [data-seg="${sg.k}"]`).click(); // 外す
      }
      const bpo = Number(await page.locator('#panel1-bpo').getAttribute('data-n'));
      expect(bpo).toBeGreaterThan(0);
      await page.locator('#panel1-bpo').click();
      expect(await walk(page)).toBe(bpo);
    });

    test('チームを選ぶと担当者の表から始まり、個人を選ぶと一覧から始まる。旧画面の内訳と文字が一致する', async ({ page }) => {
      await login(page);
      const grab = async (): Promise<string[]> =>
        page.locator('#panel1').evaluate((el) =>
          [...el.querySelectorAll('#panel1-title, table.cdrill thead th, table.cdrill tbody tr, table.cdrill tfoot th, #panel1-listhead')].map(
            (n) => (n.textContent ?? '').replace(/\s+/g, ' ').trim(),
          ),
        );
      const scenario = async (url: string): Promise<Record<string, string[]>> => {
        await page.goto(url);
        await expect(page.locator('#cards1 .c')).toHaveCount(7);
        const out: Record<string, string[]> = {};
        await page.locator('#teams .chip', { hasText: /^伊壺チーム$/ }).click();
        await page.locator('#cards1 .c').nth(1).click();
        out['team'] = await grab();
        const person = (await page.locator('#person option').nth(1).getAttribute('value')) ?? '';
        await page.selectOption('#person', person);
        await page.locator('#cards1 .c').nth(1).click();
        out['person'] = await grab();
        return out;
      };
      const oldScreen = await scenario('/sales-kpi');
      const newScreen = await scenario('/app/sales-kpi');
      expect(newScreen).toEqual(oldScreen);
      expect(oldScreen['team']?.[0]).toBe('③ 商談の予定（66件）');
      expect(oldScreen['team']?.length).toBeGreaterThan(5);
    });
  });
});
