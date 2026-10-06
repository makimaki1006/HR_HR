import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: 'job_copy_progressive_loading.spec.ts', workers: 1,
  retries: 0, timeout: 60_000, reporter: 'list',
  outputDir: '../../data/job-copy-local/candidate-browser/progressive-results',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5188', trace: 'off', screenshot: 'off', video: 'off' },
});
