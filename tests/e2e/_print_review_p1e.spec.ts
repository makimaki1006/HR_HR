/**
 * tests/e2e/_print_review_p1e.spec.ts
 *
 * P1e (2026-05-07): commit 556d960 後の本番 PDF 再生成 (アクションバー経由)。
 *  - アクションバー「採用コンサルレポート PDF」ボタンクリック → 新タブで MI variant
 *  - 開いた新タブで PDF 化 → out/print_review_p1e/mi_via_action_bar.pdf
 *  - 各 variant の HTML probe → prod_html_probe_*.json
 *  - 期待: P0-1 / P0-2 / P0-3 修正反映確認
 *
 * 実行:
 *   $env:E2E_RUN_PDF="1"; $env:BASE_URL="https://hr-hw.onrender.com";
 *   $env:E2E_EMAIL="..."; $env:E2E_PASS="...";
 *   npx playwright test _print_review_p1e --project=chromium
 */
import { test } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');
const OUT_DIR = path.resolve(__dirname, '..', '..', 'out', 'print_review_p1e');

test.describe.serial('Print review p1e (commit 556d960 / via action bar)', () => {
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
    // upload 直後の状態に戻る (実 UI フロー)
    const reuploadStart = await loginAndUpload(page, FIXTURE, 'indeed');
    const sid2 = reuploadStart.sessionId;

    const miButton = page.locator('a[data-variant="market_intelligence"]').first();
    await miButton.waitFor({ state: 'visible', timeout: 30_000 });
    const href = await miButton.getAttribute('href');
    if (!href || !href.includes('variant=market_intelligence')) {
      throw new Error(`MI button href invalid: ${href}`);
    }

    // openVariantReport は target=_blank で window.open を呼ぶため新ページを待つ
    const [popup] = await Promise.all([
      context.waitForEvent('page', { timeout: 60_000 }).catch(() => null),
      miButton.click(),
    ]);

    let miPage = popup;
    if (!miPage) {
      // popup blocked → 直接 navigate (fallback)
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
