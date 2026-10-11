import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-email-tracking.spec.ts', workers: 1,
  outputDir: process.env.B_TEST_ARTIFACTS ?? 'test-results/email-tracking',
  use: { baseURL: 'http://127.0.0.1:19342', viewport: { width: 960, height: 680 } },
  webServer: { command: 'node tests/e2e/crm-email-tracking-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19342/static/app/__tracking_preview', reuseExistingServer: false },
});
