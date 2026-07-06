import { defineConfig, devices } from '@playwright/test';

/**
 * VRT (Visual Regression Test) 専用 Playwright 設定
 *
 * 【重要】baseline は CI (ubuntu-latest) でのみ生成・比較する。
 * Windows ローカルではフォントレンダリングの差異があるため比較は行わない。
 * ローカルでは以下コマンドで構文確認・テスト一覧の確認のみ行うこと:
 *   npx playwright test -c playwright.vrt.config.ts --list
 *
 * snapshot ストアは OS suffix を含めない (CI 専用 baseline):
 *   tests/vrt/__screenshots__/{projectName}/{arg}{ext}
 *
 * サンセット基準:
 *   8週間でリグレッション検出実績ゼロのプロジェクト/テストは縮小を検討する。
 *
 * 実行 (CI):
 *   npx playwright test -c playwright.vrt.config.ts --update-snapshots  # baseline 生成
 *   npx playwright test -c playwright.vrt.config.ts                      # 比較
 *
 * 参考: 既存 E2E 設定は playwright.config.ts を使用。本ファイルは VRT 専用。
 */
export default defineConfig({
  testDir: './tests/vrt',
  timeout: 120_000,

  // OS suffix を含めない — CI (ubuntu-latest) 専用 baseline のため。
  // {platform} や {snapshotSuffix} を除外することでクロス環境での意図しない diff を防ぐ。
  snapshotPathTemplate: '{testDir}/__screenshots__/{projectName}/{arg}{ext}',

  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: process.env.CI ? 2 : 1,

  reporter: [
    ['list'],
    ['html', { open: 'never', outputFolder: 'vrt-report' }],
  ],

  expect: {
    // mobile-375 のフルページは数万 px 高になり、既定 5s では安定化キャプチャが
    // 完了しない (CI 実測 2026-07-07)。assertion polling の上限を 60s に拡大。
    // ※ ここは expect ブロック内でないと toHaveScreenshot に効かない
    //   (トップレベル timeout はテスト全体の制限で assertion polling には無関係)。
    timeout: 60_000,
    toHaveScreenshot: {
      // 1% 以内のピクセル差は許容 (アンチエイリアス等の微細差を吸収)
      maxDiffPixelRatio: 0.01,
      // CSS/JS アニメーションを無効化して決定論的スクリーンショットを保証
      animations: 'disabled',
    },
  },

  use: {
    // file:// で静的 HTML を直接開くため baseURL 不要
    trace: 'on-first-retry',
    launchOptions: {
      args: [
        // フォントレンダリングを無効化して CI/ローカル間の差異を最小化
        '--font-render-hinting=none',
        // デバイスピクセル比を固定して HiDPI 環境での差異を排除
        '--force-device-scale-factor=1',
        // GPU 無効化 (ヘッドレス CI での安定性向上)
        '--disable-gpu',
      ],
    },
  },

  projects: [
    {
      // スマートフォン縦表示 (375×812)
      name: 'mobile-375',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 375, height: 812 },
      },
    },
    {
      // A4 縦 (794×1123) — 印刷プレビュー・PDF 検証用
      name: 'a4-794',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 794, height: 1123 },
      },
    },
    {
      // デスクトップ標準 (1280×900)
      name: 'desktop-1280',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1280, height: 900 },
      },
    },
  ],
});
