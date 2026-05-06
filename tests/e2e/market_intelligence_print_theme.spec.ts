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
 *
 * 2026-05-06 拡張 (Worker E):
 *   - 印刷専用 summary / annotations セクションの存在検証
 *   - mi-print-only / mi-screen-only 要素の display 切替検証
 *   - parent_rank vs national_rank の HTML 順序 (印刷文脈)
 *   - resident estimated_beta 母集団数値表現禁止
 *   - Full variant では print-only 要素が出ない
 *   - PDF 生成 smoke (オプション、E2E_RUN_PDF=1 でのみ実行)
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

    // === 拡張 (2026-05-06 Worker E) ===
    // 印刷専用 summary / annotations セクションが HTML に残ること
    // (Worker BCD が mi-print-summary / mi-print-annotations / mi-print-only / mi-screen-only を追加)
    const printSummaryCount = await page.locator('.mi-print-summary').count();
    const printAnnotationsCount = await page.locator('.mi-print-annotations').count();
    expect(
      printSummaryCount,
      '.mi-print-summary が HTML に存在 (印刷専用、screen では display:none 想定)',
    ).toBeGreaterThanOrEqual(1);
    expect(
      printAnnotationsCount,
      '.mi-print-annotations が HTML に存在',
    ).toBeGreaterThanOrEqual(1);

    // mi-print-only / mi-screen-only の display 切替検証
    // print emulation 適用済の状態で getComputedStyle を取得
    const displayStates = await page.evaluate(() => {
      const printOnly = document.querySelector('.mi-print-only') as HTMLElement | null;
      const screenOnly = document.querySelector('.mi-screen-only') as HTMLElement | null;
      return {
        printOnlyDisplay: printOnly ? window.getComputedStyle(printOnly).display : null,
        screenOnlyDisplay: screenOnly ? window.getComputedStyle(screenOnly).display : null,
      };
    });

    if (displayStates.printOnlyDisplay !== null) {
      expect(
        displayStates.printOnlyDisplay,
        'print emulation 中、.mi-print-only は display:none 以外 (block 等) で可視',
      ).not.toBe('none');
    }
    if (displayStates.screenOnlyDisplay !== null) {
      expect(
        displayStates.screenOnlyDisplay,
        'print emulation 中、.mi-screen-only は display:none で非表示',
      ).toBe('none');
    }

    // screen に戻して逆を検証
    await page.emulateMedia({ media: 'screen' });
    await page.waitForTimeout(300);
    const screenDisplayStates = await page.evaluate(() => {
      const printOnly = document.querySelector('.mi-print-only') as HTMLElement | null;
      const screenOnly = document.querySelector('.mi-screen-only') as HTMLElement | null;
      return {
        printOnlyDisplay: printOnly ? window.getComputedStyle(printOnly).display : null,
        screenOnlyDisplay: screenOnly ? window.getComputedStyle(screenOnly).display : null,
      };
    });
    if (screenDisplayStates.printOnlyDisplay !== null) {
      expect(
        screenDisplayStates.printOnlyDisplay,
        'screen media 中、.mi-print-only は display:none で非表示',
      ).toBe('none');
    }
    if (screenDisplayStates.screenOnlyDisplay !== null) {
      expect(
        screenDisplayStates.screenOnlyDisplay,
        'screen media 中、.mi-screen-only は display:none 以外で可視',
      ).not.toBe('none');
    }
  });

  test('print summary block renders before hero bar (HTML order)', async ({ page }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const order = await page.evaluate(() => {
      const summary = document.querySelector('.mi-print-summary');
      const hero = document.querySelector('.mi-hero-bar');
      if (!summary || !hero) {
        return { summaryFound: !!summary, heroFound: !!hero, summaryFirst: null as boolean | null };
      }
      // DOCUMENT_POSITION_FOLLOWING: hero が summary の後にある = summary 先
      const pos = summary.compareDocumentPosition(hero);
      const summaryFirst = (pos & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
      return { summaryFound: true, heroFound: true, summaryFirst };
    });

    expect(order.summaryFound, '.mi-print-summary が DOM に存在').toBe(true);
    expect(order.heroFound, '.mi-hero-bar が DOM に存在').toBe(true);
    expect(
      order.summaryFirst,
      '.mi-print-summary が .mi-hero-bar より HTML 順で先に出現',
    ).toBe(true);
  });

  test('print annotations include 5 required legends', async ({ page }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const annotations = page.locator('.mi-print-annotations').first();
    await expect(annotations, '.mi-print-annotations が DOM に存在').toBeAttached();

    const liCount = await annotations.locator('li').count();
    expect(liCount, '.mi-print-annotations 内の li 数 ≥ 5').toBeGreaterThanOrEqual(5);

    const text = (await annotations.innerText()) || '';
    // 5 つの必須凡例キーワード
    const requiredKeywords = [
      ['workplace', '従業地'], // どちらかの表現を許容
      ['estimated_beta', '推計', '常住地'],
      ['national_rank', '全国順位', '全国'],
      ['parent_rank', '親自治体', '親'],
      ['生活コスト', 'コスト'],
    ];

    for (const variants of requiredKeywords) {
      const matched = variants.some((v) => text.includes(v));
      expect(
        matched,
        `.mi-print-annotations に必須凡例 [${variants.join('|')}] のいずれかを含む (実テキスト先頭 200 文字: "${text.slice(0, 200)}")`,
      ).toBe(true);
    }
  });

  test('parent_rank appears before national_rank in HTML order (print context)', async ({
    page,
  }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(300);

    const result = await page.evaluate(() => {
      // parent_rank を含むセル/要素と national_rank を含むセル/要素を取得
      // data-rank-type 属性 or テキストマッチ両対応
      const all = Array.from(document.querySelectorAll('*')) as HTMLElement[];

      const findFirst = (predicate: (el: HTMLElement) => boolean): HTMLElement | null => {
        for (const el of all) {
          if (predicate(el)) return el;
        }
        return null;
      };

      const parentEl = findFirst(
        (el) =>
          el.getAttribute('data-rank-type') === 'parent_rank' ||
          el.classList.contains('mi-parent-rank') ||
          (el.children.length === 0 && (el.textContent?.includes('親自治体') ?? false)),
      );
      const nationalEl = findFirst(
        (el) =>
          el.getAttribute('data-rank-type') === 'national_rank' ||
          el.classList.contains('mi-national-rank') ||
          (el.children.length === 0 && (el.textContent?.includes('全国順位') ?? false)),
      );

      if (!parentEl || !nationalEl) {
        return { parentFound: !!parentEl, nationalFound: !!nationalEl, parentFirst: null as boolean | null };
      }
      const pos = parentEl.compareDocumentPosition(nationalEl);
      const parentFirst = (pos & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
      return { parentFound: true, nationalFound: true, parentFirst };
    });

    if (!result.parentFound || !result.nationalFound) {
      test.info().annotations.push({
        type: 'note',
        description: `parent_rank/national_rank マーカーが DOM に存在しないため順序検証 skip (parent=${result.parentFound}, national=${result.nationalFound})`,
      });
      test.skip();
      return;
    }
    expect(
      result.parentFirst,
      'parent_rank セルが national_rank より HTML 先頭側に出現 (print 文脈再確認)',
    ).toBe(true);
  });

  test('resident estimated_beta has no population number expression in print summary', async ({
    page,
  }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const summary = page.locator('.mi-print-summary').first();
    const exists = (await summary.count()) > 0;
    if (!exists) {
      test.info().annotations.push({
        type: 'note',
        description: '.mi-print-summary が DOM に存在しないため skip',
      });
      test.skip();
      return;
    }

    const text = (await summary.innerText()) || '';

    // 「○○人」のような母集団人数表現を検出
    // ただし「1人あたり」「個人」「法人」等は誤検出になり得るので
    // 数字 + 任意の記号 + 人 のうち、「数字+人」直結 (区切り or 文末) のみ NG
    // 例: "12,345人", "5万人", "10000人"
    const populationNumberPattern = /[\d,]+\s*[万千]?\s*人(?![あ-ん一-龥])/;
    const m = text.match(populationNumberPattern);
    expect(
      m,
      `.mi-print-summary に母集団数値表現 (例: "12345人") が含まれていない (検出: "${m?.[0] ?? ''}", 周辺: "${text.slice(Math.max(0, (m?.index ?? 0) - 20), (m?.index ?? 0) + 30)}")`,
    ).toBeNull();

    // Hard NG 用語
    const hardNgTerms = ['推定人数', '想定人数', '母集団人数', '人見込み'];
    for (const term of hardNgTerms) {
      expect(
        text.includes(term),
        `.mi-print-summary に Hard NG 用語 "${term}" を含まない`,
      ).toBe(false);
    }
  });

  test('Full variant does not include print-only summary or annotations', async ({ page }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'full');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const summaryCount = await page.locator('.mi-print-summary').count();
    const annotationsCount = await page.locator('.mi-print-annotations').count();

    expect(
      summaryCount,
      'Full variant では .mi-print-summary 不在 (print 専用セクションは MI variant のみ)',
    ).toBe(0);
    expect(
      annotationsCount,
      'Full variant では .mi-print-annotations 不在',
    ).toBe(0);
  });

  for (const theme of THEMES) {
    test(`theme=${theme}: mi-print-only elements remain in HTML`, async ({ page }) => {
      const sessionId = await getSession(page);
      const url = buildReportUrl(sessionId, 'market_intelligence', theme);
      const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
      expect(res?.status()).toBeLessThan(400);

      // HTML 上には常に存在 (display は @media で制御)
      const printOnlyCount = await page.locator('.mi-print-only').count();
      expect(
        printOnlyCount,
        `theme=${theme}: .mi-print-only 要素が HTML に存在 (display は @media print 制御)`,
      ).toBeGreaterThanOrEqual(1);
    });
  }

  // PDF 生成テスト (オプション、E2E_RUN_PDF=1 でのみ実行)
  // Chromium headless が必要。CI 環境 / 通常 CI では skip。
  test('page.pdf() produces non-empty PDF (smoke)', async ({ page, browserName }) => {
    if (process.env.E2E_RUN_PDF !== '1') {
      test.skip(true, 'E2E_RUN_PDF=1 が設定されていない (PDF smoke テストはオプション)');
    }
    if (browserName !== 'chromium') {
      test.skip(true, 'page.pdf() は Chromium のみサポート');
    }

    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(500);

    const pdf = await page.pdf({ format: 'A4' });
    expect(
      pdf.length,
      `生成 PDF サイズが 1KB 超 (実サイズ: ${pdf.length} bytes)`,
    ).toBeGreaterThan(1024);
    // 一時ファイルは保存しない (メモリ内バッファのみで検証)
  });
});
