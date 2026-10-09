import { defineConfig, devices } from '@playwright/test';

/**
 * CRM 書き込みの実サーバ E2E (本物の Rust サーバ + 偽 HubSpot + 偽 Turso)。PR ごとの E2E (tests/e2e/pr) とは別。
 * 実行: npx playwright test -c tests/e2e/crm_write_live.config.ts
 *   前提: debug ビルドのサーババイナリ (E2E_BIN、既定 target-private/debug/rust_dashboard → 無ければ target/debug/...) と frontend の build。
 *   globalSetup が偽 HubSpot (scripts/loadtest/fake_hubspot.mjs --writable)、偽 Turso (tests/e2e/crm_write_live/fake_turso.mjs)、
 *   サーバを起動・停止する (crm_write_live/global-setup.ts)。本物の HubSpot / Turso / Google には一切つながない。
 */
export default defineConfig({
  testDir: '.',
  testMatch: /crm_write_live\.spec\.ts/,
  globalSetup: './crm_write_live/global-setup.ts',
  fullyParallel: false,
  workers: 1, // 偽 HubSpot の状態を 1 つ共有する
  retries: 0,
  timeout: 90_000,
  expect: { timeout: 15_000 },
  reporter: [['list']],
  outputDir: '../../test-results-crm-write-live',
  use: {
    baseURL: 'http://localhost:9422',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: process.env.E2E_CHROME_PATH ? { executablePath: process.env.E2E_CHROME_PATH } : {},
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
