import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: '.', testIgnore: '**/pr/**', testMatch: 'job_copy_manage_edit_flow.spec.ts', workers: 1, retries: 0,
  timeout: 60_000, reporter: 'list', outputDir: '../../data/job-copy-local/manage-flow/test-results',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5197', trace: 'off', screenshot: 'off', video: 'off' },
  webServer: { cwd: path.resolve(__dirname, '../..'), command: 'node tests/e2e/job-copy-listings-mock-server.mjs', url: 'http://127.0.0.1:5197/app/job-copy', reuseExistingServer: true },
});
