import { defineConfig, devices } from '@playwright/test';
import { PR_BASE_URL } from './helpers/fixture_values';

/**
 * PR ごとの E2E 設定 (nightly / regression 用の tests/e2e/*.spec.ts とは別物)。
 * 実行: npx playwright test -c tests/e2e/pr/playwright.pr.config.ts
 *   前提: サーババイナリ (E2E_BIN、既定 .e2e-bin/rust_dashboard) と、React を使う spec なら frontend の build。
 *   globalSetup が fixture DB を作り、PORT=9217 でサーバを起動・停止する (global-setup.ts)。
 *   ブラウザが無い環境: E2E_CHROME_PATH に chrome.exe 等を指定 (CI では不要)。
 * fixture にテーブルを足す: scripts/e2e/make_fixture_db.py の TABLES に create_xxx を足す。
 *   既知値は tests/e2e/pr/helpers/fixture_values.ts に書き、spec で具体値 assert する。
 * spec の置き場所: tests/e2e/pr/<画面>.spec.ts。旧新一致は legacy_vs_app.example.spec.ts をコピーする。
 * 注意: 認証後の POST は CSRF の許可 Origin (localhost:3000/8080 等) 以外を 403 にする。
 *   POST を打つ spec は helpers/login.ts の postJson(page, url, body) を使う。
 */
export default defineConfig({
  testDir: '.',
  testMatch: /.*\.spec\.ts/,
  globalSetup: './global-setup.ts',
  fullyParallel: false,
  workers: 1, // サーバと DB を 1 つ共有する
  forbidOnly: !!process.env.CI,
  retries: 1,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [['list'], ['html', { open: 'never', outputFolder: '../../../playwright-report-pr' }]],
  outputDir: '../../../test-results-pr',
  use: {
    baseURL: PR_BASE_URL,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    launchOptions: process.env.E2E_CHROME_PATH ? { executablePath: process.env.E2E_CHROME_PATH } : {},
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
