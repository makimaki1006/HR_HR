import { defineConfig, devices } from '@playwright/test';
// Local demo preview of the job timeline (vite dev server, fictional data, no Rust backend).
// Start it first: (cd frontend && npx vite --port 5297 --strictPort --host 127.0.0.1)
export default defineConfig({
  testDir: '.', testMatch: ['job_copy_timeline_preview.spec.ts'], workers: 1,
  timeout: 120000, retries: 0, reporter: 'list',
  outputDir: '../../data/job-copy-local/timeline-preview/test-results',
  use: { ...devices['Desktop Chrome'], baseURL: process.env.JOB_COPY_PREVIEW_URL ?? 'http://127.0.0.1:5297', trace: 'off', screenshot: 'off', video: 'off' },
});
