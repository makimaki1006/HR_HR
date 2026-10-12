import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-caller-metrics.spec.ts', workers: 1,
  outputDir: process.env.B_TEST_ARTIFACTS ?? 'test-results/caller-metrics',
  use: { baseURL: 'http://127.0.0.1:19346', viewport: { width: 1440, height: 900 } },
  webServer: { command: 'node tests/e2e/crm-caller-metrics-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19346/static/app/__metrics_crm', reuseExistingServer: false },
});
