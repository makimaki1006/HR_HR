/**
 * market_intelligence_variant_isolation.spec.ts
 *
 * Phase 3 Step 5 Phase 7 (Worker P7) Spec 3 / 4:
 *   variant ごとの分離を検証 (Step 5 セクションは MarketIntelligence variant 専用)
 *
 *   - Full     (variant 未指定): Step 5 セクションを含まないこと
 *   - Public   (variant=public): Step 5 セクションを含まないこと
 *   - MI       (variant=market_intelligence): Step 5 セクションを含むこと
 *
 * 設計指針:
 *   - feedback_reverse_proof_tests.md: variant の包含/除外を相互検証 (取り違えで両方通らないように)
 *   - feedback_partial_commit_verify.md: variant 切替で他 variant に副作用が無いことを保証
 */

import { test, expect, Page } from '@playwright/test';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');

/**
 * Step 5 (MarketIntelligence) 固有のマーカーが HTML に含まれるかを判定。
 * 描画方法 (table 名 / セクション名 / fallback) のいずれかが入る。
 */
function hasStep5Markers(html: string): boolean {
  return (
    html.includes('mi-parent-ward-ranking') ||
    html.includes('mi-rank-table') ||
    html.includes('data-section="market-intelligence"') ||
    html.includes('mi-empty') ||
    // fallback: Step 5 でしか出ない用語
    html.includes('検証済み推定 β') ||
    html.includes('検証済み推定β') ||
    html.includes('estimated_beta')
  );
}

test.describe('MarketIntelligence variant isolation (Phase 7 Spec 3)', () => {
  test.setTimeout(360_000);

  // Playwright のデフォルトでは test ごとに new context (cookie 非共有)。
  // session_id 文字列を URL に乗せても auth cookie がリセットされ /login redirect
  // 経由で /login HTML が返るため、Public/Full の test (negative check) は
  // 偶然 PASS、MI の test (positive check) は FAIL する。
  // 各 test で loginAndUpload を再実行することで test isolation 整合性を保つ。
  async function getSession(page: Page): Promise<string> {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    return sessionId;
  }

  test('Full variant (default) does not include Step 5 sections', async ({ page }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId); // variant 未指定 = Full
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const html = await page.content();
    expect(
      hasStep5Markers(html),
      'Full variant に Step 5 (MarketIntelligence) セクションが混入していないこと',
    ).toBe(false);

    // 「市内順位」「検証済み推定」のような Step 5 専用語句も無いこと (逆証明)
    expect(html).not.toContain('mi-parent-ward-ranking');
    expect(html).not.toContain('検証済み推定 β');
    expect(html).not.toContain('検証済み推定β');
  });

  test('Public variant does not include Step 5 sections', async ({ page }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'public');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const html = await page.content();
    expect(
      hasStep5Markers(html),
      'Public variant に Step 5 (MarketIntelligence) セクションが混入していないこと',
    ).toBe(false);

    expect(html).not.toContain('mi-parent-ward-ranking');
    expect(html).not.toContain('検証済み推定 β');
  });

  test('MarketIntelligence variant exclusively includes Step 5 sections', async ({
    page,
  }) => {
    const sessionId = await getSession(page);
    const url = buildReportUrl(sessionId, 'market_intelligence');
    const res = await page.goto(url, { waitUntil: 'domcontentloaded' });
    expect(res?.status()).toBeLessThan(400);

    const html = await page.content();
    expect(
      hasStep5Markers(html),
      'MarketIntelligence variant に Step 5 セクション (mi-* / 従業地ベース等 / mi-empty) のいずれかが描画されること',
    ).toBe(true);
  });
});
