import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
const port = Number(process.env.JOB_COPY_E2E_PORT ?? '5197');
const baseURL = `http://127.0.0.1:${port}`;
export default defineConfig({
  testDir: '.', testMatch: 'job_copy_browser_history.spec.ts', workers: 1, retries: 0,
  timeout: 60_000, reporter: 'list', outputDir: '../../data/job-copy-local/browser-history/test-results',
  use: { ...devices['Desktop Chrome'], baseURL, trace: 'off', screenshot: 'off', video: 'off' },
  webServer: { cwd: path.resolve(__dirname, '../..'), command: 'node tests/e2e/job-copy-listings-mock-server.mjs', url: `${baseURL}/app/job-copy`, reuseExistingServer: false },
});
