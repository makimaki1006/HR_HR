import { defineConfig, devices } from '@playwright/test';

// 実際の Rust API + 開発用の架空シート。外部サービスの鍵は渡さない。
export default defineConfig({
  testDir: '.', testMatch: '*.spec.ts', globalSetup: './setup.ts',
  workers: 1, fullyParallel: false, retries: 0,
  timeout: 45_000, expect: { timeout: 10_000 }, reporter: 'list',
  outputDir: '/private/tmp/hr-hr-portal-e2e-results',
  use: {
    baseURL: 'http://localhost:9230', trace: 'retain-on-failure', screenshot: 'only-on-failure',
    launchOptions: process.env.E2E_CHROME_PATH ? { executablePath: process.env.E2E_CHROME_PATH } : {},
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
