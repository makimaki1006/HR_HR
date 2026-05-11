/**
 * Round 8 P0-1 (2026-05-09): 地域 × 職業分類 × 性別 × 年齢 セグメント PDF 検証 (ローカル版)
 *
 * 実行:
 *   $env:E2E_RUN_PDF="1"; $env:BASE_URL="http://localhost:9217";
 *   $env:E2E_EMAIL="test@f-a-c.co.jp"; $env:E2E_PASS="test123";
 *   npx playwright test _round8_p0_1_prod --project=chromium
 */
import { test } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';
import { preparePdfRender } from './helpers/pdf_helper';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');
const OUT_DIR = path.resolve(__dirname, '..', '..', 'out', 'round8_p0_1_prod');

test.describe.serial('Round 8 P0-1: occupation_segment_summary local PDF check', () => {
  test('Generate MI PDF prod (port 9217)', async ({ page, browserName, context }) => {
    test.skip(browserName !== 'chromium', 'PDF generation requires Chromium');
    test.setTimeout(360_000);

    if (!fs.existsSync(OUT_DIR)) {
      fs.mkdirSync(OUT_DIR, { recursive: true });
    }

    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    void context;

    // ローカルではアクションバーの popup ではなく直接 MI variant URL を開く
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const miPage = page;
    await miPage.goto(url, { waitUntil: 'domcontentloaded', timeout: 120_000 });

    await miPage.waitForTimeout(3500);

    // 新セクション存在確認 (HTML レベル、PDF生成前)
    const segCount = await miPage.locator('[data-mi-section="occupation-segment"]').count();
    const cellCount = await miPage.locator('[data-mi-section="occupation-cells"]').count();
    console.log(`occupation-segment sections: ${segCount}`);
    console.log(`occupation-cells (legacy) sections: ${cellCount}`);

    // セグメント表のテキスト断片を抽出
    if (segCount > 0) {
      const segText = await miPage
        .locator('[data-mi-section="occupation-segment"]')
        .first()
        .innerText();
      console.log('=== SEGMENT SECTION TEXT (first 800 chars) ===');
      console.log(segText.slice(0, 800));
    }

    await miPage.emulateMedia({ media: 'print' });
    await miPage.waitForTimeout(800);

    try {
      await preparePdfRender(miPage, 760);
      console.log('preparePdfRender PASS');
    } catch (e) {
      console.log('preparePdfRender FAIL (continuing for diagnostic PDF):', String(e).slice(0, 200));
    }

    const pdfPath = path.join(OUT_DIR, 'mi_prod.pdf');
    await miPage.pdf({
      path: pdfPath,
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });

    const stat = fs.statSync(pdfPath);
    if (stat.size < 10_000) {
      throw new Error(`PDF too small: ${stat.size} bytes`);
    }
    console.log(`PDF generated: ${pdfPath} (${stat.size} bytes)`);
  });
});
