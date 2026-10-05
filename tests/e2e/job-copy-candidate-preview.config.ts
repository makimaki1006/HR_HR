import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: ['job_copy_candidate.spec.ts', 'job_copy_candidate_errors.spec.ts'], workers: 1,
  retries: 0, timeout: 120000, reporter: 'list',
  outputDir: '../../data/job-copy-local/candidate-browser/test-results',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5188', trace: 'off', screenshot: 'off', video: 'off' },
});
