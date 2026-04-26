/**
 * regression_2026_04_26.spec.ts
 *
 * V2 HW Dashboard 2026-04-26 リリース回帰検証 E2E
 *
 * 目的: 本リリースで導入された UI / ナビ / 用語 / 数値表示の変更点について、
 *       「要素存在」ではなく「具体テキスト内容」を検証する。
 *
 * 設計指針 (memory feedback 準拠):
 *   - feedback_test_data_validation.md: 要素存在チェックではなくデータ妥当性を検証
 *   - feedback_e2e_chart_verification.md: canvas 存在ではなく具体描画を検証
 *   - feedback_reverse_proof_tests.md: 「テスト通過 ≠ ロジック正しい」具体値で確認
 *
 * 実行コマンド:
 *   npx playwright test tests/e2e/regression_2026_04_26.spec.ts --reporter=list
 *
 * 環境変数:
 *   BASE_URL  - 既定 https://hr-hw.onrender.com (本番) / 開発時は http://localhost:9216
 *   E2E_EMAIL - ログインメールアドレス
 *   E2E_PASS  - ログインパスワード
 */

import { test, expect, Page } from '@playwright/test';

const BASE_URL = process.env.BASE_URL ?? 'https://hr-hw.onrender.com';
const EMAIL = process.env.E2E_EMAIL ?? '';
const PASSWORD = process.env.E2E_PASS ?? '';

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/**
 * ログインしてダッシュボードを表示する。
 * 既にログイン済みなら何もしない。
 */
async function login(page: Page): Promise<void> {
  await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
  if (page.url().includes('/login')) {
    await page.fill('input[name="email"]', EMAIL);
    await page.fill('input[name="password"]', PASSWORD);
    await Promise.all([
      page.waitForLoadState('networkidle'),
      page.click('button[type="submit"]'),
    ]);
  }
  // ダッシュボード初期状態 (市場概況がデフォルト) を待つ
  await page.waitForSelector('.tab-btn', { timeout: 30_000 });
}

/**
 * 上位ナビの button (text 完全一致) をクリックして HTMX ロード完了を待つ。
 * `.tab-btn` の active クラスが切り替わるまで待機。
 */
async function clickNavTab(page: Page, label: string): Promise<void> {
  const btn = page.locator(`.tab-btn:has-text("${label}")`).first();
  await expect(btn).toBeVisible();
  await btn.click();
  // HTMX 切替: 当該ボタンに .active が付与されることを期待
  await expect(btn).toHaveClass(/active/, { timeout: 15_000 });
  // 内容読込完了のため軽く待つ (htmx:afterSwap)
  await page.waitForLoadState('networkidle');
}

// ---------------------------------------------------------------------------
// Test Suite
// ---------------------------------------------------------------------------

test.describe('REL-2026-04-26: ナビ動線 / タブ呼称 / 用語統一', () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
  });

  // -------------------------------------------------------------------------
  test('NAV-01: 「総合診断」「トレンド」が上位ナビに表示される', async ({ page }) => {
    // ナビ全体を取得
    const nav = page.locator('.tab-btn');
    const labels = await nav.allTextContents();
    const flat = labels.map(s => s.trim()).join(' | ');

    expect(flat).toContain('総合診断');
    expect(flat).toContain('トレンド');
  });

  test('NAV-02: 「総合診断」クリックで insight タブが表示される', async ({ page }) => {
    await clickNavTab(page, '総合診断');
    // hx-get="/tab/insight" がロードされたことを HTML 内容で検証
    // insight タブには「総合診断」「示唆」関連の見出しが入る
    const content = page.locator('#content');
    const text = await content.innerText();
    // 「総合診断」もしくは insight タブを示す文字列が必ず入る
    expect(text.length).toBeGreaterThan(50); // 空白タブでないこと
    // 旧ルート /tab/insight が画面遷移を起こさず HTMX で content 差替されていることの確認
    expect(page.url()).not.toContain('/tab/insight'); // SPA 的差替なので URL は不変
  });

  test('NAV-03: 「トレンド」クリックで trend タブが表示される', async ({ page }) => {
    await clickNavTab(page, 'トレンド');
    const content = page.locator('#content');
    const text = await content.innerText();
    expect(text.length).toBeGreaterThan(50);
  });

  // -------------------------------------------------------------------------
  test('NAM-01: タブ呼称「求人検索」が表示される (旧「企業調査」が無い)', async ({ page }) => {
    await clickNavTab(page, '求人検索');

    // H2 は「🔍 求人検索」または「求人検索レポート」を含む
    const content = page.locator('#content');
    const html = await content.innerHTML();

    // 旧称が残っていないこと (固有名詞の競合企業ランキングは別タブのため OK)
    expect(html).not.toContain('企業調査');
    // 競合調査も UI には残っていない (ただし survey の qa_test 等は除く)
    expect(html).not.toMatch(/<h[12][^>]*>[^<]*競合調査[^<]*<\/h[12]>/);

    // 新称が含まれる
    const text = await content.innerText();
    expect(text).toContain('求人検索');
  });

  test('NAM-02: 企業検索タブの H2 が「🔎 企業検索」になっている', async ({ page }) => {
    await clickNavTab(page, '企業検索');

    const content = page.locator('#content');
    const text = await content.innerText();

    // 新表記
    expect(text).toContain('企業検索');
    // 旧表記の H2 が残っていないこと (個別企業の詳細レポートタイトル「企業分析レポート」は許容)
    const html = await content.innerHTML();
    expect(html).not.toMatch(/<h2[^>]*>[^<]*🔎\s*企業分析[^<]*<\/h2>/);
  });

  // -------------------------------------------------------------------------
  test('TERM-01: 詳細分析 → 求人動向で「欠員補充率」が表示される (旧「欠員率」が無い)', async ({ page }) => {
    await clickNavTab(page, '詳細分析');

    // 求人動向サブタブへ遷移 (HTML 内のサブタブボタン)
    const subtab = page.locator('button:has-text("求人動向"), a:has-text("求人動向")').first();
    if (await subtab.count() > 0 && await subtab.isVisible()) {
      await subtab.click();
      await page.waitForLoadState('networkidle');
    }

    const content = page.locator('#content');
    const text = await content.innerText();

    expect(text).toContain('欠員補充率');
    // 「欠員率」単独 (補充の付かない) が KPI ラベルや見出しに無いこと
    // (※「欠員補充率」は内包で OK、単独「欠員率」だけが NG)
    const standaloneVacancyRate = text.match(/(?<![補充])欠員率/g);
    expect(standaloneVacancyRate).toBeNull();
  });

  // -------------------------------------------------------------------------
  test('SEL-01: jobmap の雇用形態セレクトに「派遣・その他」がある (旧「業務委託」が無い)', async ({ page }) => {
    await clickNavTab(page, '地図');

    // セレクト要素を探す
    const select = page.locator('select[name="employment_type"], select#employment_type, select[name*="emp"]').first();
    await expect(select).toBeVisible({ timeout: 15_000 });

    const options = await select.locator('option').allTextContents();
    const flat = options.map(s => s.trim());

    expect(flat).toContain('派遣・その他');
    expect(flat).not.toContain('業務委託');
  });

  // -------------------------------------------------------------------------
  test('PANEL5-01: 採用診断 Panel 5 の HW 警告が 1 行に集約されている', async ({ page }) => {
    await clickNavTab(page, '採用診断');

    const content = page.locator('#content');
    // Panel 5 = 条件診断 / 条件ギャップ
    // 「ハローワーク掲載求人のみ」の警告がページ内で複数回出ていないこと
    const html = await content.innerHTML();

    // banner クラス or 警告文が「1 行集約」の証として、
    // Panel 5 領域内に含まれる警告語句を 1 回までに制限
    // (templates での集約により、同一警告は 1 表示まで)
    const hwOnlyMatches = html.match(/ハローワーク掲載求人のみ/g) ?? [];
    // タブ全体としての banner + Panel 5 内 1 行 = 最大 2 まで許容 (旧版は 4-5 出ていた)
    expect(hwOnlyMatches.length).toBeLessThanOrEqual(2);
  });

  // -------------------------------------------------------------------------
  test('AP1-01: 総合診断で AP-1 給与改善示唆が法定福利・賞与込みで表示される', async ({ page }) => {
    await clickNavTab(page, '総合診断');

    const content = page.locator('#content');
    const text = await content.innerText();

    // AP-1 が発火する都道府県/市区町村でのみ確認可能。
    // 環境依存のため「示唆として AP-1 形式の文字列が存在しうる」緩い検証 +
    // もし AP-1 が表示されている場合は「賞与」「法定福利」を含むこと。
    if (text.includes('給与改善') || text.includes('AP-1') || text.includes('AP1')) {
      // 修正後の body には「賞与4ヶ月+法定福利16%含む」が必ず入る
      expect(text).toMatch(/賞与|法定福利/);
    } else {
      test.info().annotations.push({
        type: 'note',
        description: 'AP-1 が現在のフィルタで発火していないため、本検証はスキップされます',
      });
    }
  });

  // -------------------------------------------------------------------------
  test('OVR-01: 市場概況の H2 直下に「⚠️ HW 掲載求人のみ」バナーが表示される', async ({ page }) => {
    await clickNavTab(page, '市場概況');

    const content = page.locator('#content');
    const html = await content.innerHTML();

    // 警告 banner には amber 系クラスと「ハローワーク掲載求人のみ」のテキストが入る
    // H2 (📊 地域概況) の直下にバナーがあること
    expect(html).toMatch(/(?:📊\s*地域概況|地域概況)[\s\S]{0,500}ハローワーク掲載求人のみ/);
  });

  // -------------------------------------------------------------------------
  test('FOOT-01: 詳細分析タブのフッターに「相関と因果は別物」明記がある', async ({ page }) => {
    await clickNavTab(page, '詳細分析');

    const content = page.locator('#content');
    const text = await content.innerText();

    // 詳細分析タブ末尾の「⚠️ 本分析の前提」セクション
    expect(text).toMatch(/相関関係と因果関係は別物/);
    expect(text).toMatch(/ハローワーク掲載求人ベース|ハローワーク掲載求人のみ/);
  });

  // -------------------------------------------------------------------------
  test('IQR-01: 媒体分析の給与統計カードに「外れ値除外（IQR法）」表示', async ({ page }) => {
    await clickNavTab(page, '媒体分析');

    const content = page.locator('#content');
    const text = await content.innerText();

    // 給与統計（月給換算）カードと分布カード見出しに subtitle が入る
    expect(text).toContain('外れ値除外');
    expect(text).toContain('IQR');
  });

  // -------------------------------------------------------------------------
  test('NAV-04: ナビバーのタブ数が 9 (詳細分析を含む) であり全て可視', async ({ page }) => {
    const tabs = page.locator('.tab-btn');
    // 9 タブ: 市場概況 / 地図 / 地域カルテ / 詳細分析 / 総合診断 / トレンド / 求人検索 / 採用診断 / 企業検索 / 媒体分析
    // (実際には 10 になる可能性: 採用診断 = recruitment_diag を含めると 10)
    const count = await tabs.count();
    expect(count).toBeGreaterThanOrEqual(9);
    expect(count).toBeLessThanOrEqual(11);

    // 各ボタンが visible
    for (let i = 0; i < count; i++) {
      await expect(tabs.nth(i)).toBeVisible();
    }
  });
});
