import path from 'node:path';
import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: 'pr', testMatch: ['job_copy_consultant_risk.spec.ts', 'job_copy_consultant_good.spec.ts'], workers: 1, retries: 0, reporter: 'list',
  use: { baseURL: 'http://127.0.0.1:5197', trace: 'off', viewport: { width: 1440, height: 900 } },
  webServer: { cwd: path.resolve(__dirname, '../..'), command: 'node tests/e2e/job-copy-listings-mock-server.mjs', url: 'http://127.0.0.1:5197/app/job-copy', reuseExistingServer: false },
});
