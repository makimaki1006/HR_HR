import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
const port = Number(process.env.JOB_COPY_E2E_PORT ?? '5197');
const baseURL = `http://127.0.0.1:${port}`;
export default defineConfig({
  testDir: '.', testMatch: ['job_copy_listings.spec.ts', 'job_copy_listings_workspace.spec.ts', 'job_copy_sidebar.spec.ts', 'job_copy_ui_polish.spec.ts', 'rikurozi_draft.spec.ts', 'job_copy_listing_events.spec.ts'], workers: 1, retries: 0,
  // ユーザー指示: 今回はパソコンのみ確認（スマホの既存テストは実行しない）。
  grepInvert: /390/,
  testIgnore: '**/pr/**',
  timeout: 60_000, reporter: 'list', outputDir: '../../data/job-copy-local/listings/test-results',
  use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 }, baseURL, trace: 'off', screenshot: 'off', video: 'off' },
  webServer: { cwd: path.resolve(__dirname, '../..'), command: 'node tests/e2e/job-copy-listings-mock-server.mjs', url: `${baseURL}/static/app/job-copy-preview.html`, reuseExistingServer: false },
});
