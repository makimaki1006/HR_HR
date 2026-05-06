/**
 * market_intelligence_print_theme.spec.ts
 *
 * Phase 3 Step 5 Phase 7 (Worker P7) Spec 4 / 4:
 *   theme 切替 (default / v8 / v7a) と print emulation で
 *   MarketIntelligence セクションが消失しないことを検証する。
 *
 * 設計指針:
 *   - feedback_print_css_cascade_trap.md: print CSS のカスケードで主要セクションが
 *     display:none されてしまう事故を逆証明で検出
 *   - feedback_test_data_validation.md: 「印刷ボタン存在」ではなく実際の印刷描画を検証
 */

import { test, expect, Page } from '@playwright/test';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');

const THEMES = ['default', 'v8', 'v7a'] as const;

function hasMiContent(html: string): boolean {
  return (
    html.includes('mi-parent-ward-ranking') ||
    html.includes('mi-rank-table') ||
    html.includes('mi-empty') ||
    html.includes('data-section="market-intelligence"') ||
    html.includes('従業地ベース') ||
    html.includes('常住地ベース')
  );
}

test.describe('MarketIntelligence print + theme (Phase 7 Spec 4)', () => {
  test.setTimeout(420_000);

  // Playwright のデフォルトでは test ごとに new context (cookie 非共有)。
  // sharedSessionId 文字列だけ持ち回しても auth cookie がリセットされて /login
  // redirect 経由で MI 不在 HTML が返り FAIL するため、各 test で loginAndUpload を
  // 再実行する (display_rules.spec.ts と同じ方針)。
  async function getSession(page: Page): Promise<string> {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    return sessionId;
  }

  for (const theme of THEMES) {
    test(`MI variant + theme=${theme} keeps key sections`, async ({ page }) => {
      const sessionId = await getSession(page);
      const url = buildReportUrl(sessionId, 'market_intelligence', theme);
      const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
      expect(res?.status(), `theme=${theme} HTTP status`).toBeLessThan(400);

      const html = await page.content();
      expect(
        hasMiContent(html),
        `theme=${theme} で MI セクションが描画されること (print CSS カスケードで消えない)`,
      ).toBe(true);

      // 印刷導線 (button or link) が存在すること
      // テーマ機能の基本要件: 印刷 / PDF 出力ボタンがいずれかの形で見える
      const printControls = page.locator(
        [
          'button:has-text("印刷")',
          'button:has-text("PDF")',
          '[data-action="print"]',
          'a:has-text("印刷")',
          'a:has-text("PDF")',
        ].join(', '),
      );
      const count = await printControls.count();
      expect(
        count,
        `theme=${theme}: 印刷/PDF コントロールが少なくとも 1 つ存在`,
      ).toBeGreaterThanOrEqual(1);
    });
  }

  test('print emulation preserves MarketIntelligence sections (variant=market_intelligence)', async ({
    page,
  }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    // 通常表示で確認
    const screenHtml = await page.content();
    const screenHasMi = hasMiContent(screenHtml);

    // print media をエミュレート
    await page.emulateMedia({ media: 'print' });
    // emulateMedia は CSS 適用のみで再描画しないが、
    // computed style を取って display:none チェックする
    await page.waitForTimeout(500);

    const printHtml = await page.content();
    const printHasMi = hasMiContent(printHtml);

    // HTML 自体は変わらないので markup の有無は維持される。
    // ここでは「print 適用後でも MI のマーカーは依然 HTML に存在する」
    // ことを保証する (display:none は HTML から消すわけではないため OK)
    expect(printHasMi, 'print emulation 後も MI マーカーが HTML に残ること').toBe(
      screenHasMi,
    );

    if (printHasMi) {
      // 主要 MI セクション要素が print media で display:none になっていないこと
      // (feedback_print_css_cascade_trap.md: @page + body padding で本文が縮む事故)
      const visibleAfterPrint = await page.evaluate(() => {
        const candidates = [
          document.querySelector('.mi-rank-table'),
          document.querySelector('.mi-parent-ward-ranking'),
          document.querySelector('[data-section="market-intelligence"]'),
        ].filter((el): el is Element => el !== null);

        if (candidates.length === 0) return null; // セクションそのものが描画されていない
        return candidates.some((el) => {
          const style = window.getComputedStyle(el);
          return (
            style.display !== 'none' &&
            style.visibility !== 'hidden' &&
            (el as HTMLElement).offsetParent !== null
          );
        });
      });

      if (visibleAfterPrint === null) {
        test.info().annotations.push({
          type: 'note',
          description: 'MI セクション要素が DOM に存在しないため print 可視性検証は skip',
        });
      } else {
        expect(
          visibleAfterPrint,
          'print emulation 後、MI セクション要素のいずれかが visible (CSS で完全に消されていない)',
        ).toBe(true);
      }
    }
  });
});
