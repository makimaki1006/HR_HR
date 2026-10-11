import { defineConfig } from '@playwright/test';
import path from 'node:path';
export default defineConfig({
  testDir: '.', testMatch: 'crm-email-compose.spec.ts', workers: 1,
  outputDir: 'test-results/email-compose', timeout: 60000,
  use: { baseURL: 'http://127.0.0.1:19343', viewport: { width: 1440, height: 900 } },
  webServer: { command: 'node tests/e2e/crm-email-compose-server.mjs', cwd: path.resolve(__dirname, '../..'), url: 'http://127.0.0.1:19343/static/app/__compose_preview', reuseExistingServer: false },
});
