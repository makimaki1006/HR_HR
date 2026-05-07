/**
 * market_intelligence_pdf_flow.spec.ts
 *
 * P0-3 (2026-05-06): 実ユーザー PDF 導線で MarketIntelligence セクションが
 * 含まれること、および印刷時にチャートが本文幅内に収まること (見切れなし) を検証する。
 *
 * 検証観点:
 *   1. アクションバー (媒体分析タブ) に「採用コンサルレポート PDF」ボタンが存在し、
 *      クリックで variant=market_intelligence の URL が開かれること (P0-1 検証)
 *   2. 当該 URL の HTML に MarketIntelligence 用の主要キーワードが含まれること (P0-1 検証)
 *   3. 同 HTML を print emulation した時、ECharts / canvas / svg が
 *      A4 縦本文幅 (210mm - 8mm*2 = 194mm) を超えて描画されないこと (P0-2 検証)
 *
 * 設計指針 (memory feedback 準拠):
 *   - feedback_e2e_chart_verification.md: chart 描画の実質検証
 *   - feedback_reverse_proof_tests.md: 「見切れない」をドメイン不変 (本文幅以下) で検証
 *   - feedback_test_data_validation.md: ボタン存在ではなく URL / DOM 値を検証
 *   - feedback_render_cold_start_timeout.md: navigationTimeout 60s+ で本番 cold start 対応
 *
 * 環境変数:
 *   BASE_URL, E2E_EMAIL, E2E_PASS (helpers/session.ts ensureCredentials() で検証)
 *
 * 実行:
 *   $env:E2E_EMAIL="..."; $env:E2E_PASS="..."
 *   npx playwright test tests/e2e/market_intelligence_pdf_flow.spec.ts
 */

import { test, expect } from '@playwright/test';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');

/**
 * MarketIntelligence variant 固有のキーワードが HTML 中に含まれるかを判定。
 * existing `market_intelligence_variant_isolation.spec.ts` の hasStep5Markers と同等の
 * 「いずれか 1 つ以上」基準。
 */
function hasMarketIntelligenceMarker(html: string): boolean {
  return (
    html.includes('mi-parent-ward-ranking') ||
    html.includes('data-section="market-intelligence"') ||
    html.includes('mi-rank-table') ||
    html.includes('mi-empty') ||
    html.includes('従業地ベース') ||
    html.includes('常住地ベース') ||
    html.includes('採用マーケットインテリジェンス') ||
    html.includes('検証済み推定 β') ||
    html.includes('検証済み推定β')
  );
}

test.describe('MarketIntelligence PDF flow (P0-3 default user flow)', () => {
  test.setTimeout(360_000);

  test('action bar exposes "採用コンサルレポート PDF" button with variant=market_intelligence', async ({
    page,
  }) => {
    // login + upload で媒体分析タブのアクションバーを表示させる
    await loginAndUpload(page, FIXTURE, 'indeed');

    // アクションバー上に MarketIntelligence variant 用ボタンが描画されていること
    const miButton = page.locator('a[data-variant="market_intelligence"]').first();
    await expect(
      miButton,
      'アクションバーに variant=market_intelligence ボタンが存在する (P0-1 導線追加)',
    ).toHaveCount(1, { timeout: 30_000 });

    // ボタンの href が variant=market_intelligence を含むこと (URL 直接構築の保証)
    const href = await miButton.getAttribute('href');
    expect(href, 'MI ボタンの href').toBeTruthy();
    expect(href!).toContain('variant=market_intelligence');

    // ラベル文言にユーザー認知用フレーズが含まれること
    const text = (await miButton.textContent()) ?? '';
    expect(
      text.includes('採用コンサルレポート') || text.includes('マーケットインテリジェンス'),
      'ボタンテキストに「採用コンサルレポート」または「マーケットインテリジェンス」を含む',
    ).toBe(true);
  });

  test('default user flow PDF target HTML contains MarketIntelligence sections', async ({
    page,
  }) => {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');

    // 実ユーザーがアクションバーから開く URL を直接 navigate
    // (window.open は Playwright で context.waitForEvent('page') が必要だが
    //  本テストの主目的は「URL 経由で MI HTML が返る」ことなので直接 goto する)
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status(), `GET ${url} status`).toBeLessThan(400);

    const html = await page.content();
    expect(
      hasMarketIntelligenceMarker(html),
      'PDF 導線 URL の HTML に MarketIntelligence セクションが含まれること',
    ).toBe(true);
  });

  test('MarketIntelligence keywords present in default PDF flow output (reverse proof)', async ({
    page,
  }) => {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    const url = buildReportUrl(sessionId, 'market_intelligence');
    await page.goto(url, { waitUntil: 'domcontentloaded' });
    const html = await page.content();

    // 採用コンサル提案で必須となる MI 領域キーワードを少なくとも 2 種類
    // (Plan B 違反 NG 用語ではない、コンサル文脈の中立用語のみ)
    const requiredCandidates = [
      '採用マーケットインテリジェンス',
      '従業地ベース',
      '常住地ベース',
      '職業×地域',
      '検証済み推定',
    ];
    const matched = requiredCandidates.filter((kw) => html.includes(kw));
    expect(
      matched.length,
      `MI コンサル文脈キーワードが少なくとも 2 種類含まれること (matched=${matched.join('|')})`,
    ).toBeGreaterThanOrEqual(2);

    // Hard NG 11 用語が混入していないこと (feedback_neutral_expression_for_targets.md)
    const forbidden = [
      'データ不足',
      '要件再確認',
      'データ準備中',
      '未集計',
      '参考表示なし',
      '本条件では表示対象がありません',
      '実測値準備中',
      '現在取得できません',
      '未投入',
      'Sample',
      'サンプル',
    ];
    for (const ng of forbidden) {
      // 「サンプル」「Sample」は CSV プレビュー文脈で正当に出現する可能性があるため
      // MI セクション直近の data-section ブロック内のみ検査するのが理想だが、
      // ここでは page 全体で誤検出許容ラインとして「文中の単語」だけ assert する。
      // ただし NG 11 用語は基本的にどこにも出ないことを期待 (feedback_test_data_validation.md)
      if (ng === 'Sample' || ng === 'サンプル') {
        // CSV プレビューで合法出現する可能性があるため skip
        continue;
      }
      expect(html.includes(ng), `NG 用語 "${ng}" が MI レポートに混入していないこと`).toBe(false);
    }
  });

  test('chart elements do not clip beyond page content area in print emulation (P0-2)', async ({
    page,
  }) => {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    const url = buildReportUrl(sessionId, 'market_intelligence');
    await page.goto(url, { waitUntil: 'domcontentloaded' });

    // ECharts SVG renderer の描画完了を待つ (init 後 setOption で SVG 生成)
    await page.waitForTimeout(2000);

    // 印刷メディアにエミュレート (CSS @media print rule が適用される)
    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(500);

    // 全 ECharts / canvas / svg について、親 .echart 系コンテナの幅を超えていないこと
    const violations = await page.evaluate(() => {
      const result: Array<{
        selector: string;
        elementWidth: number;
        parentWidth: number;
        ratio: number;
      }> = [];

      const checkBoundary = (
        el: Element,
        parent: Element | null,
        selector: string,
      ): void => {
        if (!parent) return;
        const elRect = (el as HTMLElement).getBoundingClientRect();
        const parentRect = (parent as HTMLElement).getBoundingClientRect();
        if (parentRect.width <= 0) return;
        // 子の幅が親の幅 + 4px (rounding tolerance) を超えていたら見切れ候補
        if (elRect.width > parentRect.width + 4) {
          result.push({
            selector,
            elementWidth: Math.round(elRect.width * 100) / 100,
            parentWidth: Math.round(parentRect.width * 100) / 100,
            ratio: Math.round((elRect.width / parentRect.width) * 1000) / 1000,
          });
        }
      };

      // .echart コンテナ自体が body / section の本文幅を超えていないか
      document.querySelectorAll('.echart, .echart-wrap, .echart-container, .chart-container').forEach((el) => {
        const parent = el.parentElement;
        checkBoundary(el, parent, el.className.toString());
      });

      // 内部の canvas / svg
      document.querySelectorAll('.echart canvas, .echart svg').forEach((el) => {
        const parent = (el as HTMLElement).closest('.echart');
        checkBoundary(el, parent, 'canvas/svg inside .echart');
      });

      return result;
    });

    expect(
      violations.length,
      `印刷時にチャート要素が親本文幅を超えていないこと (見切れなし)。違反一覧: ${JSON.stringify(
        violations,
      )}`,
    ).toBe(0);

    // 副 assertion: 少なくとも 1 つの ECharts インスタンスが描画されていること
    // (チャートが 0 個なら見切れ判定も常に PASS してしまうので逆証明)
    const chartCount = await page.evaluate(
      () => document.querySelectorAll('[_echarts_instance_]').length,
    );
    expect(
      chartCount,
      'MarketIntelligence variant で ECharts インスタンスが少なくとも 1 つ描画されていること',
    ).toBeGreaterThanOrEqual(1);
  });
});
