/**
 * tests/e2e/labor_flow_headcount_gate.spec.ts
 *
 * 地図タブ「人材フロー」パネルの人員増減率が、ブラウザ上で正しく描画されるかの E2E。
 *
 * 2026-08-12 に集計方法を変えた:
 *   旧 `avg_delta_1y` (各社の増減率の単純平均) → 新 `headcount_rate_1y` (人数加重、抑制時 null)
 * 単純平均は分母の小さい企業に支配され、東京都 × 人材・アウトソーシングで
 * +752.7% と表示されていた (実測でその 99.4% が 1 社由来)。
 *
 * ここで守りたいのは次の 3 点:
 *   1. 抑制された業種で `undefined%` / `NaN%` / `null%` が画面に出ないこと
 *   2. 抑制された理由が利用者に伝わること (title 属性 / tooltip)
 *   3. 表示された値が人数加重の妥当な範囲に収まること (単純平均時代の 3 桁 % が出ない)
 *
 * 実行前提:
 *   BASE_URL   稼働中のサーバ
 *   E2E_EMAIL / E2E_PASS  ログイン情報
 *   企業データ (SalesNow 相当) が接続されていること。未接続なら test は skip する。
 */

import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/session';

const BASE = process.env.BASE_URL ?? 'http://localhost:9216';

/**
 * 地図タブを開く。
 *
 * 「地図」は `#explore-subnav`(「調べる ▾」グループ) の中にあり、既定では
 * `hidden` クラスが付いていて不可視。共通 helper の clickNavTab は可視性を
 * 待つため使えないので、グループを開いてから遷移する。
 */
async function openJobmapTab(page: Page): Promise<void> {
  const group = page.locator('#explore-group-btn');
  if (await group.count()) {
    await group.click();
    await expect(page.locator('#explore-subnav')).not.toHaveClass(/hidden/, { timeout: 10_000 });
  }
  await page.locator('.tab-btn[hx-get="/tab/jobmap"]').first().click();
  // 地図タブの実体 (人材フローパネル) が swap されるまで待つ
  await page.waitForSelector('#jm-labor-flow-table', { state: 'attached', timeout: 60_000 });
  await page.waitForFunction(() => typeof (window as any).loadLaborFlow === 'function', null, {
    timeout: 60_000,
  });
}

type Industry = {
  sn_industry: string;
  companies: number;
  total_emp: number;
  net_change_1y: number;
  headcount_rate_1y: number | null;
  headcount_notice: string | null;
  top1_share_pct: number | null;
};

test.describe('人材フロー: 人員増減率の表示ゲート', () => {
  test.beforeEach(async ({ page }) => {
    await login(page, BASE);
    await openJobmapTab(page);
  });

  test('抑制セルに undefined%/NaN% を出さず、理由を添える', async ({ page }) => {
    // 千代田区は「表示される業種」と「1 社集中で抑制される業種」が両方出る地域
    const data: { industries?: Industry[]; error?: string } = await page.evaluate(async () => {
      const r = await fetch(
        '/api/jobmap/labor-flow?prefecture=' +
          encodeURIComponent('東京都') +
          '&municipality=' +
          encodeURIComponent('千代田区'),
      );
      return r.json();
    });

    test.skip(!!data.error, `企業データ未接続のため skip: ${data.error}`);
    const industries = data.industries ?? [];
    expect(industries.length, '業種が 1 件も返っていない').toBeGreaterThan(0);

    // パネルを実際に描画させる (window.loadLaborFlow は jobmap.html から呼ばれる公開関数)
    await page.evaluate(() => {
      (window as any).loadLaborFlow('東京都', '千代田区');
    });
    await page.waitForFunction(
      () => {
        const el = document.querySelector('#jm-labor-flow-table');
        return !!el && (el as HTMLElement).innerHTML.includes('<table');
      },
      null,
      { timeout: 30_000 },
    );

    const tableHtml = await page.locator('#jm-labor-flow-table').innerHTML();
    const tableText = await page.locator('#jm-labor-flow-table').innerText();

    // --- 1. 壊れた表示が無いこと ---
    for (const bad of ['undefined', 'NaN', 'null%', '[object Object]']) {
      expect(tableText, `テーブルに ${bad} が出ている`).not.toContain(bad);
    }

    // --- 2. 廃止フィールドの痕跡が無いこと ---
    expect(tableHtml).not.toContain('avg_delta_1y');

    // --- 3. 抑制セルは「—」で、理由が title 属性に入っていること ---
    const suppressed = industries.filter((i) => i.headcount_rate_1y === null);
    const shown = industries.filter((i) => i.headcount_rate_1y !== null);
    expect(suppressed.length + shown.length).toBe(industries.length);

    if (suppressed.length > 0) {
      expect(tableText, '抑制セルがあるのに「—」が描画されていない').toContain('—');
      // 抑制された業種のうち 1 件について、理由の文言が DOM に載っているか
      const notice = suppressed.find((i) => !!i.headcount_notice)?.headcount_notice;
      expect(notice, '抑制セルに notice が付いていない').toBeTruthy();
      expect(tableHtml, '抑制理由が title 属性に入っていない').toContain(
        (notice as string).slice(0, 12),
      );
    }

    // --- 4. 表示値が人数加重として妥当な範囲か ---
    // 単純平均時代は 1 社の外れ値で 3 桁 % が出ていた。人数加重ではそうならない。
    for (const i of shown) {
      const rate = i.headcount_rate_1y as number;
      expect(
        Math.abs(rate),
        `${i.sn_industry} の増減率 ${rate}% が大きすぎる (人数加重で 3 桁は異常)`,
      ).toBeLessThan(100);
      // 表示されたセルは必ずゲートを通っている
      expect(i.companies, `${i.sn_industry}: 企業数ゲート違反`).toBeGreaterThanOrEqual(30);
      if (i.top1_share_pct !== null) {
        expect(i.top1_share_pct, `${i.sn_industry}: 集中度ゲート違反`).toBeLessThan(50);
      }
      // 画面に実際にその数値が出ているか (小数第 1 位まで)
      const label = (rate >= 0 ? '+' : '') + rate.toFixed(1) + '%';
      expect(tableText, `${i.sn_industry} の ${label} が描画されていない`).toContain(label);
    }

    // --- 5. 抑制セルは棒グラフ側でも区別されていること ---
    // 増減「人数」は抑制されないので棒は立つ。率だけ伏せて棒をそのまま緑/赤で
    // 描くと、1 社の増員が地域の傾向に見える (千代田区コンサルティングは
    // 137 社あるが 1 社が変動の 56% を占め +7,224 人の棒が立つ)。
    if (suppressed.length > 0) {
      expect(tableText, '抑制セルに ※ 印が付いていない').toContain('※');
      expect(tableText, '※ の意味を説明する注記が無い').toContain(
        '1 社の増減が大半を占める業種',
      );
      // ECharts は canvas 描画で DOM にテキストが出ないため、option を読んで確認する
      const chart = await page.evaluate(() => {
        const el = document.querySelector('#jm-labor-flow-chart');
        const inst = (window as any).echarts?.getInstanceByDom(el);
        const opt = inst?.getOption();
        return {
          labels: (opt?.yAxis?.[0]?.data ?? []) as string[],
          colors: ((opt?.series?.[0]?.data ?? []) as any[]).map(
            (d) => d?.itemStyle?.color ?? '',
          ),
        };
      });
      expect(chart.colors.length, 'series データが取れていない').toBeGreaterThan(0);
      expect(chart.labels.length, 'y 軸ラベルが取れていない').toBe(chart.colors.length);
      expect(
        chart.labels.some((l) => l.includes('※')),
        'グラフの軸ラベルに ※ が無い',
      ).toBe(true);
      // 抑制セルの棒が緑/赤ではなく灰色になっているか
      expect(chart.colors, '抑制セル用の灰色が 1 本も無い').toContain('#64748b');
      // 灰色の本数が抑制セルの件数と一致すること (塗り分けの取り違えを弾く)
      expect(
        chart.colors.filter((c) => c === '#64748b').length,
        '灰色の棒の本数が抑制セル数と合わない',
      ).toBe(suppressed.length);
    }

    await page.locator('#jm-labor-flow').screenshot({
      path: 'playwright-report/labor_flow_chiyoda.png',
    });
  });

  test('企業数が極端に少ない地域では値を出さない', async ({ page }) => {
    // 道志村: 全 9 社。1 社の増員 (+570 人) が地域の純増の 98% を占める
    const data: { industries?: Industry[]; error?: string } = await page.evaluate(async () => {
      const r = await fetch(
        '/api/jobmap/labor-flow?prefecture=' +
          encodeURIComponent('山梨県') +
          '&municipality=' +
          encodeURIComponent('南都留郡道志村'),
      );
      return r.json();
    });
    test.skip(!!data.error, `企業データ未接続のため skip: ${data.error}`);

    const industries = data.industries ?? [];
    for (const i of industries) {
      expect(
        i.headcount_rate_1y,
        `${i.sn_industry} (n=${i.companies}) に増減率が出ている。` +
          '企業数が少ない地域で 1 社の事情を地域の傾向として示してはならない',
      ).toBeNull();
      expect(i.headcount_notice, `${i.sn_industry} に抑制理由が無い`).toBeTruthy();
    }

    await page.evaluate(() => {
      (window as any).loadLaborFlow('山梨県', '南都留郡道志村');
    });
    await page.waitForFunction(
      () => {
        const el = document.querySelector('#jm-labor-flow-table');
        return !!el && (el as HTMLElement).innerHTML.length > 100;
      },
      null,
      { timeout: 30_000 },
    );
    const text = await page.locator('#jm-labor-flow-table').innerText();
    for (const bad of ['undefined', 'NaN', 'null%']) {
      expect(text, `テーブルに ${bad} が出ている`).not.toContain(bad);
    }
    // 値は出さないが、増減人数そのものは事実として出てよい
    expect(text).toContain('—');
  });

  test('画面に社内略語やサービス名が出ていない', async ({ page }) => {
    await page.evaluate(() => {
      (window as any).loadLaborFlow('東京都', '千代田区');
    });
    await page.waitForTimeout(3_000);
    const panel = await page.locator('#jm-labor-flow').innerText();
    for (const forbidden of ['SalesNow', 'salesnow']) {
      expect(panel, `画面に ${forbidden} が出ている`).not.toContain(forbidden);
    }
  });
});
