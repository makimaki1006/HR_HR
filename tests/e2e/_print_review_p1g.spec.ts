/**
 * tests/e2e/_print_review_p1g.spec.ts
 *
 * P1g (2026-05-06): commit 4657b06 後の本番 PDF 再生成 (アクションバー経由)。
 *  - mi-print-block / mi-print-break-before class 追加
 *  - @media print: break-inside: avoid + break-before: page
 *  - page 25 の 3 表同居解消、給与・生活コスト独立ページ化を期待
 *  - 出力: out/print_review_p1g/mi_via_action_bar.pdf
 *  - 各 variant の HTML probe → prod_html_probe_*.json
 *
 * 実行:
 *   $env:E2E_RUN_PDF="1"; $env:BASE_URL="https://hr-hw.onrender.com";
 *   $env:E2E_EMAIL="..."; $env:E2E_PASS="...";
 *   npx playwright test _print_review_p1g --project=chromium
 */
import { test } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');
const OUT_DIR = path.resolve(__dirname, '..', '..', 'out', 'print_review_p1g');

test.describe.serial('Print review p1g (commit 4657b06 / print page break refinement)', () => {
  test('Generate MI PDF via action bar click + variant HTML probe', async ({ page, browserName, context }) => {
    test.skip(browserName !== 'chromium', 'PDF generation requires Chromium');
    test.skip(process.env.E2E_RUN_PDF !== '1', 'E2E_RUN_PDF=1 required');
    test.setTimeout(360_000);

    if (!fs.existsSync(OUT_DIR)) {
      fs.mkdirSync(OUT_DIR, { recursive: true });
    }

    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');

    // === Step A: 各 variant の HTML probe ===
    const variants: Array<'market_intelligence' | 'full' | 'public'> = [
      'market_intelligence',
      'full',
      'public',
    ];
    const probeResults: Record<string, unknown> = {};
    for (const v of variants) {
      const url = buildReportUrl(sessionId, v);
      const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
      if (!res || res.status() >= 400) {
        throw new Error(`variant=${v} status=${res?.status()}`);
      }
      const html = await page.content();

      const ngTerms = [
        'データ不足',
        '要件再確認',
        'データ準備中',
        '未集計',
        '参考表示なし',
        '本条件では表示対象がありません',
        '実測値準備中',
        '現在取得できません',
        '未投入',
      ];
      const ngHits: Array<{ term: string; count: number }> = [];
      for (const t of ngTerms) {
        const re = new RegExp(t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g');
        const m = html.match(re);
        if (m && m.length > 0) ngHits.push({ term: t, count: m.length });
      }

      // 「代表職種」「ほか N 職種」検出 (MI variant のみで意味あり)
      const reprColPresent = html.includes('代表職種');
      const otherJobPresent = /ほか\s*\d+\s*職種/.test(html);
      // 「mi-living-table」 (給与・生活コスト) の行数 (集約済なら少ない)
      const livingTableMatch = html.match(/<table[^>]*class="[^"]*mi-living[^"]*"[^>]*>([\s\S]*?)<\/table>/i);
      let livingTableRowCount = 0;
      if (livingTableMatch) {
        livingTableRowCount = (livingTableMatch[1].match(/<tr/g) ?? []).length;
      }

      const probe = {
        variant: v,
        miPrintOnlyCount: (html.match(/mi-print-only/g) ?? []).length,
        miPrintSummaryCount: (html.match(/mi-print-summary/g) ?? []).length,
        miPrintAnnotationsCount: (html.match(/mi-print-annotations/g) ?? []).length,
        miParentWardCount: (html.match(/mi-parent-ward-ranking/g) ?? []).length,
        miSectionCount: (html.match(/data-mi-section="market-intelligence"/g) ?? []).length,
        keywordHits: {
          採用マーケットインテリジェンス: html.includes('採用マーケットインテリジェンス'),
          職業地域クロス: /職業[×x]地域/.test(html),
          常住地ベース: html.includes('常住地ベース'),
          従業地ベース: html.includes('従業地ベース'),
          推定β: /検証済み推定\s*β|推定\s*β/.test(html),
        },
        livingCostAggregation: {
          reprColPresent,
          otherJobPresent,
          livingTableRowCount,
        },
        ngHits,
        htmlSize: html.length,
      };
      probeResults[v] = probe;
      fs.writeFileSync(
        path.join(OUT_DIR, `prod_html_probe_${v}.json`),
        JSON.stringify(probe, null, 2),
        'utf-8',
      );
      fs.writeFileSync(path.join(OUT_DIR, `prod_html_${v}.html`), html, 'utf-8');
    }
    fs.writeFileSync(
      path.join(OUT_DIR, 'all_variants_probe.json'),
      JSON.stringify(probeResults, null, 2),
      'utf-8',
    );

    // === Step B: アクションバー経由で MI ボタンを押下 → 新タブ取得 ===
    const reuploadStart = await loginAndUpload(page, FIXTURE, 'indeed');
    const sid2 = reuploadStart.sessionId;

    const miButton = page.locator('a[data-variant="market_intelligence"]').first();
    await miButton.waitFor({ state: 'visible', timeout: 30_000 });
    const href = await miButton.getAttribute('href');
    if (!href || !href.includes('variant=market_intelligence')) {
      throw new Error(`MI button href invalid: ${href}`);
    }

    const [popup] = await Promise.all([
      context.waitForEvent('page', { timeout: 60_000 }).catch(() => null),
      miButton.click(),
    ]);

    let miPage = popup;
    if (!miPage) {
      miPage = page;
      await miPage.goto(buildReportUrl(sid2, 'market_intelligence'), {
        waitUntil: 'domcontentloaded',
      });
    } else {
      await miPage.waitForLoadState('domcontentloaded', { timeout: 60_000 });
    }

    // ECharts SVG 描画完了待ち
    await miPage.waitForTimeout(3000);

    // 印刷モード化 → 全 chart resize 反映待ち
    await miPage.emulateMedia({ media: 'print' });
    await miPage.waitForTimeout(1000);

    // PDF 生成
    const pdfPath = path.join(OUT_DIR, 'mi_via_action_bar.pdf');
    await miPage.pdf({
      path: pdfPath,
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
  });
});
