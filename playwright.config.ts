import { defineConfig, devices } from '@playwright/test';

/**
 * V2 HW Dashboard Playwright 設定
 *
 * 環境変数:
 * - BASE_URL: テスト対象 URL (default: http://localhost:9216)
 *   本番例: https://hr-hw.onrender.com
 * - E2E_EMAIL: ログイン Email (auth テストで必要)
 * - E2E_PASS: ログイン Password
 *
 * 実行:
 *   npm run test:e2e:regression       — 2026-04-26 回帰シナリオ 13 件
 *   npm run test:e2e:headed           — ブラウザ画面表示
 *   npm run test:e2e:list             — シナリオ列挙のみ
 */
export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false, // ログイン状態共有のため逐次実行
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1, // セッションがランダムに切れるリスクを避けるため 1 worker
  reporter: [
    ['list'],
    ['html', { open: 'never', outputFolder: 'playwright-report' }],
  ],
  timeout: 30_000,
  expect: { timeout: 10_000 },

  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:9216',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 10_000,
    navigationTimeout: 15_000,
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
