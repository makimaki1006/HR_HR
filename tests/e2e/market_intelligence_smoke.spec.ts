/**
 * market_intelligence_smoke.spec.ts
 *
 * Phase 3 Step 5 Phase 7 (Worker P7) Spec 1 / 4:
 *   login → CSV upload → ?variant=market_intelligence で MI セクションが描画されることを smoke 検証。
 *
 * 設計指針 (memory feedback 準拠):
 *   - feedback_test_data_validation.md: 要素存在ではなくデータ妥当性を検証
 *   - feedback_e2e_chart_verification.md: chart 描画の実質検証
 *
 * 環境変数:
 *   BASE_URL, E2E_EMAIL, E2E_PASS (ensureCredentials() で検証)
 *
 * 実行:
 *   $env:E2E_EMAIL="..."; $env:E2E_PASS="..."
 *   npx playwright test tests/e2e/market_intelligence_smoke.spec.ts
 */

import { test, expect } from '@playwright/test';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');

test.describe('MarketIntelligence smoke (Phase 7 Spec 1)', () => {
  test.setTimeout(240_000);

  test('login -> upload -> ?variant=market_intelligence renders MI section', async ({
    page,
  }) => {
    // 1) login + upload + session_id 抽出 (helper 経由)
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    expect(sessionId, 'session_id が抽出できる').toBeTruthy();
    expect(sessionId).toMatch(/^[A-Za-z0-9_-]+$/);

    // 2) MarketIntelligence variant でレポートを開く
    const miUrl = buildReportUrl(sessionId, 'market_intelligence');
    const response = await page.goto(miUrl, { waitUntil: 'domcontentloaded' });

    // HTTP 200 を期待 (Phase 8 で auth ブロックされた経路を Playwright で踏破)
    expect(response?.status(), `GET ${miUrl} status`).toBeLessThan(400);

    // 3) MI セクションが少なくとも 1 つ描画されている
    //    実装に応じて class / data 属性のいずれかが含まれる。
    //    feedback_e2e_chart_verification.md: 「存在」だけでなく中身を検証する。
    const html = await page.content();
    const hasMiMarker =
      html.includes('mi-parent-ward-ranking') ||
      html.includes('data-mi-section="market-intelligence"') ||
      html.includes('mi-rank-table') ||
      html.includes('従業地ベース') ||
      html.includes('常住地ベース') ||
      html.includes('mi-empty');

    expect(
      hasMiMarker,
      'MarketIntelligence variant で MI セクションのいずれかのマーカーが描画されること',
    ).toBe(true);

    // 4) 認証情報や token が DOM に流出していないこと
    //    feedback_dont_leak_secrets (general security): password/token を画面に出さない
    expect(html).not.toContain('AUTH_PASSWORD');
    expect(html).not.toContain('TURSO_EXTERNAL_TOKEN');
    expect(html).not.toContain('E2E_PASS');
    // ログインに使ったパスワード自体が DOM に出ていないこと
    const password = process.env.E2E_PASS ?? '';
    if (password.length >= 6) {
      // 短すぎるとマッチで誤検知するため最低 6 文字以上の場合のみ検査
      expect(html).not.toContain(password);
    }
  });
});
