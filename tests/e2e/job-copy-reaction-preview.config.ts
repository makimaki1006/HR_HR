import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: ['job_copy_actual_reactions.spec.ts'], workers: 1,
  timeout: 120000, retries: 0, reporter: 'list',
  outputDir: '../../data/job-copy-local/reaction-browser/test-results',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5189', trace: 'off', screenshot: 'off', video: 'off' },
});
