import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: '.', testMatch: 'job_copy_listings.spec.ts', workers: 1, retries: 0,
  timeout: 60_000, reporter: 'list', outputDir: '../../data/job-copy-local/listings/test-results',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5197', trace: 'off', screenshot: 'off', video: 'off' },
  webServer: { cwd: path.resolve(__dirname, '../../frontend'), command: 'npm exec -- vite --host 127.0.0.1 --port 5197 --strictPort', url: 'http://127.0.0.1:5197/static/app/job-copy-preview.html', reuseExistingServer: false },
});
