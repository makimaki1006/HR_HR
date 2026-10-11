import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-email-thread.spec.ts', workers: 1,
  outputDir: process.env.B_TEST_ARTIFACTS ?? 'test-results/email-thread',
  use: { baseURL: 'http://127.0.0.1:19344', viewport: { width: 960, height: 680 } },
  webServer: { command: 'node tests/e2e/crm-email-thread-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19344/static/app/__thread_preview', reuseExistingServer: false },
});
