import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-property-layout.spec.ts', workers: 1,
  outputDir: process.env.B_TEST_ARTIFACTS ?? 'test-results/property-layout',
  use: { baseURL: 'http://127.0.0.1:19343', viewport: { width: 960, height: 680 } },
  webServer: { command: 'node tests/e2e/crm-property-layout-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19343/static/app/__layout_preview', reuseExistingServer: false },
});
