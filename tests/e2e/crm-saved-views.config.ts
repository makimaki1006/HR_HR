import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-saved-views.spec.ts', workers: 1,
  outputDir: process.env.B_TEST_ARTIFACTS ?? 'test-results/saved-views',
  use: { baseURL: 'http://127.0.0.1:19345', viewport: { width: 960, height: 680 } },
  webServer: { command: 'node tests/e2e/crm-saved-views-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19345/static/app/__views_preview', reuseExistingServer: false },
});
