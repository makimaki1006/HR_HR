/**
 * tests/vrt/report.spec.ts
 *
 * Navy Report VRT (Visual Regression Tests)
 *
 * 【重要】baseline は CI (ubuntu-latest) でのみ生成・比較する。
 * Windows ローカルではフォントレンダリングの差異があるため比較は行わない。
 * ローカルでの構文確認・テスト一覧確認は以下コマンドのみ使用:
 *   npx playwright test -c playwright.vrt.config.ts --list
 *
 * fixture:
 *   vrt/fixtures/report_mi.html — Rust バイナリ (gen_vrt_fixtures) が合成データで生成する決定論的 HTML
 *   DB/サーバ/ネットワーク不要。実企業名・実スクレイピングデータを含まない (公開リポ対応)。
 *
 * サンセット基準:
 *   8週間でリグレッション検出実績ゼロのテストは対象から縮小を検討する。
 *
 * セクション別スクリーンショット構成 (差分の局所化):
 *   report-mi-full.png    — ページ全体 (全プロジェクト)
 *   section-075.png       — §07.5 年間休日×給与詳細 (.navy-jobbox-detail)
 *   section-076.png       — §07.6 人気度シグナル (.navy-popularity)
 *   table-4b.png          — 表4-B 産業別採用ニーズ密度を含む逼迫度セクション (.navy-tightness)
 *   table-2b.png          — 表2-B 地域基礎情報を含む地域セクション (.navy-region)
 *   report-mi-print.png   — 印刷メディア全体 (a4-794 プロジェクトのみ)
 */

import { test, expect } from '@playwright/test';
import * as path from 'path';

// fixture HTML のパス (process.cwd() = HR_HR/ ルート基準)
const FIXTURE_PATH = path.resolve(process.cwd(), 'vrt/fixtures/report_mi.html');
// Windows パス区切りを / に正規化して file:// URL を構築
const FIXTURE_URL = `file:///${FIXTURE_PATH.replace(/\\/g, '/')}`;

/**
 * 動的要素マスク用ロケータを返す。
 * [data-vrt-mask] 属性を持つ要素をスクリーンショットからマスクする。
 * 該当要素が存在しない場合は空マッチとなり no-op。
 */
function getMask(page: import('@playwright/test').Page) {
  return [page.locator('[data-vrt-mask]'), page.locator('.theme-toggle') /* fixed UI: スクショ写り込みノイズ (2026-07-07) */];
}

// ---------------------------------------------------------------------------
// Screen tests — 全プロジェクト (mobile-375 / a4-794 / desktop-1280) で実行
// ---------------------------------------------------------------------------

test.describe('report-mi: screen', () => {
  test.beforeEach(async ({ page }) => {
    // file:// 直接参照のためネットワーク待機は networkidle で十分
    await page.goto(FIXTURE_URL, { waitUntil: 'networkidle' });
  });

  /**
   * ページ全体のフルスクリーンショット。
   * 全体的なレイアウト崩れ・色変化・文言変更を検出する。
   */
  test('full page', async ({ page }) => {
    const mask = getMask(page);
    await expect(page).toHaveScreenshot('report-mi-full.png', {
      fullPage: true,
      mask,
    });
  });

  /**
   * §07.5 年間休日 × 給与 詳細セクション。
   * KPI カード / 分布グラフ / 散布図 / 個別求人テーブル / セグメント別統計を対象とする。
   * 差分がこのセクション内に限定されていることを確認するために分離。
   */
  test('section §07.5 (jobbox-detail)', async ({ page }) => {
    const mask = getMask(page);
    const section = page.locator('section.navy-jobbox-detail');
    await expect(section).toHaveScreenshot('section-075.png', { mask });
  });

  /**
   * §07.6 人気度シグナルセクション。
   * Indeed (SP) の「人気」「超人気」タグ集計を対象とする。
   * Indeed SP 以外のソースでは全件タグなしとなりスキップされるため、
   * fixture データは人気タグが存在する合成データを使用すること。
   */
  test('section §07.6 (popularity)', async ({ page }) => {
    const mask = getMask(page);
    const section = page.locator('section.navy-popularity');
    await expect(section).toHaveScreenshot('section-076.png', { mask });
  });

  /**
   * 表4-B 産業別採用ニーズ密度 (件数最多 8 産業) を含む採用市場逼迫度セクション全体。
   * 有効求人倍率 / 欠員補充率 / 産業別テーブルの変化を検出する。
   */
  test('表4-B (navy-tightness section)', async ({ page }) => {
    const mask = getMask(page);
    const section = page.locator('section.navy-tightness');
    await expect(section).toHaveScreenshot('table-4b.png', { mask });
  });

  /**
   * 表2-B 地域基礎情報 (可住地面積・人口密度) を含む地域セクション全体。
   * 地域 × 求人媒体データ連携テーブル群の変化を検出する。
   */
  test('表2-B (navy-region section)', async ({ page }) => {
    const mask = getMask(page);
    const section = page.locator('section.navy-region');
    await expect(section).toHaveScreenshot('table-2b.png', { mask });
  });
});

// ---------------------------------------------------------------------------
// Print tests — a4-794 プロジェクトのみ
// ---------------------------------------------------------------------------

test.describe('report-mi: print', () => {
  /**
   * 印刷メディア全体スクリーンショット。
   * @media print CSS の適用による改ページ制御・セクション表示/非表示を検証する。
   *
   * a4-794 プロジェクト以外はスキップ:
   *   - mobile-375: 印刷レイアウトと画面サイズが乖離するため除外
   *   - desktop-1280: A4 縦レイアウトとのピクセル比較が無意味なため除外
   */
  test('full page (print media)', async ({ page }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'a4-794',
      '印刷 VRT は A4 縦 (794×1123) でのみ実行。フォント・改ページ差異を吸収するため他プロジェクトは除外。',
    );

    await page.goto(FIXTURE_URL, { waitUntil: 'networkidle' });

    // @media print を有効化して CSS 印刷スタイルを適用
    await page.emulateMedia({ media: 'print' });

    const mask = getMask(page);
    await expect(page).toHaveScreenshot('report-mi-print.png', {
      fullPage: true,
      mask,
    });
  });
});
